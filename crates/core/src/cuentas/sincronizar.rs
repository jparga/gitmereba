//! Sincronización periódica de una cuenta ya dada de alta.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::almacen::{Almacen, NuevaSincronizacion, ResultadoSincronizacion};
use crate::config::RutasCuenta;
use crate::gitea::ApiGitea;
use crate::github::{ApiGithub, ClienteGithub};
use crate::modelo::{Cuenta, EstadoRepo, IdRepo, Nombre, RepoLocal, RepoOrigen};
use crate::secretos::{ClaveSecreto, Llavero, Secreto};
use crate::snapshots::{CambioDestructivo, PoliticaRetencion};
use crate::sync::{
    Accion, EstadoPrevio, FaseSync, OpcionesSync, ProgresoSync, ResultadoAccion, TipoAccion,
    ejecutar_con_progreso,
};
use crate::verificacion::{
    ContextoVerificacion, InformeVerificacion, OpcionesVerificacion, verificar_cuenta,
};

use super::comun::cargar_cuenta;
use super::contexto::Contexto;
use super::error::ErrorCuentas;
use super::proteccion::{self, CambioDetectado, InformeProteccion};

pub use crate::sync::InformeSync;

/// Sincroniza el Gitea local de `login` con lo que hay en GitHub.
///
/// Carga la cuenta, lee del llavero los tokens de GitHub y de Gitea, construye los
/// clientes reales y comprueba `salud()` antes de nada: si Gitea no responde, devuelve
/// [`ErrorCuentas::GiteaParado`] sin haber tocado GitHub. Reconstruye el
/// [`EstadoPrevio`] a partir de `almacen.estados_de` (los huérfanos conocidos) y persiste
/// el resultado.
pub async fn sincronizar<L: Llavero>(
    contexto: &Contexto<'_, L>,
    login: &Nombre,
    opciones: &OpcionesSync,
) -> Result<InformeSync, ErrorCuentas> {
    let cuenta = cargar_cuenta(contexto.rutas, login)?;

    let token_github = leer_secreto(contexto.llavero, login, ClaveSecreto::TokenGithub)?;
    let token_gitea = leer_secreto(contexto.llavero, login, ClaveSecreto::TokenGitea)?;

    let github = ClienteGithub::nuevo(token_github.clone())?;
    let gitea = super::comun::cliente_gitea_de_cuenta(&cuenta, token_gitea)?;

    sincronizar_con(
        contexto.almacen,
        &cuenta,
        &github,
        &gitea,
        &token_github,
        opciones,
    )
    .await
}

/// Lee un secreto obligatorio; si falta, se trata como un fallo del propio llavero (no
/// debería ocurrir para una cuenta ya dada de alta).
fn leer_secreto<L: Llavero>(
    llavero: &L,
    login: &Nombre,
    clave: ClaveSecreto,
) -> Result<Secreto, ErrorCuentas> {
    llavero
        .leer(login, clave)?
        .ok_or_else(|| ErrorCuentas::TokenGiteaNoGenerado(login.clone()))
}

/// El núcleo, genérico sobre [`ApiGithub`]/[`ApiGitea`] para poder probarlo con dobles
/// (`crate::cuentas::dobles`) sin red ni Gitea real. [`sincronizar`] y [`super::alta::alta`]
/// son finas envolturas sobre esta función.
pub async fn sincronizar_con<G: ApiGithub, T: ApiGitea>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    github: &G,
    gitea: &T,
    token_github: &Secreto,
    opciones: &OpcionesSync,
) -> Result<InformeSync, ErrorCuentas> {
    sincronizar_con_progreso(
        almacen,
        cuenta,
        github,
        gitea,
        token_github,
        opciones,
        &|_| {},
    )
    .await
}

/// Igual que [`sincronizar_con`], pero además informa del avance real de la pasada
/// llamando a `al_progresar` con un [`ProgresoSync`]: lo que emite
/// [`crate::sync::ejecutar_con_progreso`], reenviado tal cual (aquí no hay fase
/// `Verificando`: la añade [`sincronizar_y_verificar_con_progreso`], que es quien también
/// verifica).
pub async fn sincronizar_con_progreso<G: ApiGithub, T: ApiGitea>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    github: &G,
    gitea: &T,
    token_github: &Secreto,
    opciones: &OpcionesSync,
    al_progresar: &dyn Fn(ProgresoSync),
) -> Result<InformeSync, ErrorCuentas> {
    if !gitea.salud().await.map_err(ErrorCuentas::from)? {
        return Err(ErrorCuentas::GiteaParado(cuenta.login.clone()));
    }

    let estado_previo = reconstruir_estado_previo(almacen, &cuenta.login)?;
    let informe = ejecutar_con_progreso(
        github,
        gitea,
        cuenta,
        token_github,
        opciones,
        &estado_previo,
        al_progresar,
    )
    .await?;
    persistir_informe(almacen, &informe)?;

    // Reconciliación de los permisos LAN: un fallo aquí nunca hace fallar la
    // propia sincronización, que ya ha terminado con éxito. Sin ningún usuario LAN no
    // escribe nada en Gitea (ver `usuarios_lan::reconciliar_permisos_lan`).
    if let Err(error) = super::usuarios_lan::reconciliar_permisos_lan(gitea, &[]).await {
        tracing::warn!(
            cuenta = %cuenta.login,
            error = %error,
            "no se pudo reconciliar los permisos LAN tras sincronizar"
        );
    }

    Ok(informe)
}

/// Resultado de [`sincronizar_y_verificar`]/[`sincronizar_y_verificar_con`]: el informe
/// de sincronización y, si se ha podido ejecutar, el de verificación.
///
/// No se llama `ResultadoSincronizacion` porque
/// ese nombre ya lo usa [`crate::almacen::ResultadoSincronizacion`] (el `enum`
/// `Ok`/`ConFallos`/`Error` del histórico), importado en este mismo fichero.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultadoSincronizacionYVerificacion {
    pub sync: InformeSync,
    /// `None` si la verificación no se ha podido ejecutar (p. ej. un fallo al leer el
    /// almacén): no es fatal para la sincronización, que ya ha terminado con éxito.
    pub verificacion: Option<InformeVerificacion>,
    /// `None` si la protección de snapshots no se ha podido ejecutar en esta pasada;
    /// tampoco es fatal para la sincronización.
    ///
    /// `skip_deserializing`: `InformeProteccion` incluye `CambioDestructivo`, de
    /// `crate::snapshots`, que solo implementa `Serialize` (no hace falta releer este
    /// campo desde JSON en ningún sitio; solo se serializa hacia fuera, p. ej. para
    /// depuración).
    #[serde(default, skip_deserializing)]
    pub proteccion: Option<InformeProteccion>,
}

/// Igual que [`sincronizar`], pero además ejecuta una pasada de
/// [`crate::verificacion::verificar_cuenta`] tras sincronizar y persiste sus diagnósticos con `guardar_estado_repo`.
pub async fn sincronizar_y_verificar<L: Llavero>(
    contexto: &Contexto<'_, L>,
    login: &Nombre,
    opciones: &OpcionesSync,
    opciones_verificacion: &OpcionesVerificacion,
) -> Result<ResultadoSincronizacionYVerificacion, ErrorCuentas> {
    let cuenta = cargar_cuenta(contexto.rutas, login)?;

    let token_github = leer_secreto(contexto.llavero, login, ClaveSecreto::TokenGithub)?;
    let token_gitea = leer_secreto(contexto.llavero, login, ClaveSecreto::TokenGitea)?;

    let github = ClienteGithub::nuevo(token_github.clone())?;
    let gitea = super::comun::cliente_gitea_de_cuenta(&cuenta, token_gitea)?;

    sincronizar_y_verificar_con(
        contexto.almacen,
        &cuenta,
        &github,
        &gitea,
        &token_github,
        opciones,
        opciones_verificacion,
    )
    .await
}

/// Núcleo genérico de [`sincronizar_y_verificar`], sobre los mismos traits que
/// [`sincronizar_con`] (del que es una envoltura: primero sincroniza, luego verifica).
///
/// `verificar_cuenta` recibe el listado de GitHub que `sync::ejecutar` deja en
/// `InformeSync::origen` (no se vuelve a pedir: no gasta cuota dos veces) y el de Gitea,
/// que se vuelve a listar porque es local y barato. Dos salvaguardas evitan falsos
/// huérfanos: los repos locales sin entrada en el listado (p. ej. porque falló el listado
/// de su organización o saltó la alerta del plan) reciben una entrada mínima sin rama, así
/// que se evalúan por corrupción y obsolescencia pero no por SHA; y los ya marcados
/// huérfanos se excluyen para no sobrescribir su estado.
pub async fn sincronizar_y_verificar_con<G: ApiGithub, T: ApiGitea>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    github: &G,
    gitea: &T,
    token_github: &Secreto,
    opciones: &OpcionesSync,
    opciones_verificacion: &OpcionesVerificacion,
) -> Result<ResultadoSincronizacionYVerificacion, ErrorCuentas> {
    sincronizar_y_verificar_con_progreso(
        almacen,
        cuenta,
        github,
        gitea,
        token_github,
        opciones,
        opciones_verificacion,
        &|_| {},
    )
    .await
}

/// Igual que [`sincronizar_y_verificar_con`], pero además informa del avance real de la
/// pasada llamando a `al_progresar` con un [`ProgresoSync`]: lo de
/// [`sincronizar_con_progreso`] tal cual, más `Verificando {0,0}` justo antes de ejecutar
/// la verificación (se emite se pueda o no ejecutar de verdad: p. ej. si listar Gitea de
/// nuevo falla, ver más abajo, la fase igualmente «se ha alcanzado»).
#[allow(clippy::too_many_arguments)]
pub async fn sincronizar_y_verificar_con_progreso<G: ApiGithub, T: ApiGitea>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    github: &G,
    gitea: &T,
    token_github: &Secreto,
    opciones: &OpcionesSync,
    opciones_verificacion: &OpcionesVerificacion,
    al_progresar: &dyn Fn(ProgresoSync),
) -> Result<ResultadoSincronizacionYVerificacion, ErrorCuentas> {
    let informe_sync = sincronizar_con_progreso(
        almacen,
        cuenta,
        github,
        gitea,
        token_github,
        opciones,
        al_progresar,
    )
    .await?;

    // Se lista una única vez y se comparte entre la verificación y la protección de
    // snapshots: ninguna de las dos vuelve a listar Gitea.
    let local = match local_de_la_pasada(almacen, cuenta, gitea).await {
        Ok(local) => Some(local),
        Err(error) => {
            tracing::warn!(
                cuenta = %cuenta.login,
                error = %error,
                "no se pudo listar Gitea tras sincronizar: se omiten la verificación y la \
                 protección de snapshots de esta pasada"
            );
            None
        }
    };

    al_progresar(ProgresoSync {
        fase: FaseSync::Verificando,
        hechos: 0,
        total: 0,
    });

    let verificacion = match &local {
        Some(local) => {
            match ejecutar_verificacion(
                almacen,
                cuenta,
                github,
                &informe_sync,
                local,
                opciones_verificacion,
            )
            .await
            {
                Ok(informe) => {
                    if let Err(error) = persistir_verificacion(almacen, &informe) {
                        tracing::warn!(
                            cuenta = %cuenta.login,
                            error = %error,
                            "no se pudo persistir el resultado de la verificación periódica"
                        );
                    }
                    Some(informe)
                }
                Err(error) => {
                    tracing::warn!(
                        cuenta = %cuenta.login,
                        error = %error,
                        "no se pudo ejecutar la verificación periódica tras sincronizar"
                    );
                    None
                }
            }
        }
        None => None,
    };

    let proteccion = match &local {
        Some(local) => {
            let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
            let informe = proteccion::proteger_cuenta(
                &rutas_cuenta,
                local,
                OffsetDateTime::now_utc(),
                &PoliticaRetencion::default(),
            )
            .await;
            registrar_proteccion(almacen, cuenta, &informe);
            Some(informe)
        }
        None => None,
    };

    Ok(ResultadoSincronizacionYVerificacion {
        sync: informe_sync,
        verificacion,
        proteccion,
    })
}

/// Repos locales de `cuenta` para esta pasada (tras sincronizar), sin los huérfanos ya
/// conocidos. Listado una única vez y compartido entre `ejecutar_verificacion` y
/// `proteccion::proteger_cuenta`.
async fn local_de_la_pasada<T: ApiGitea>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    gitea: &T,
) -> Result<Vec<RepoLocal>, ErrorCuentas> {
    let huerfanos = huerfanos_conocidos(almacen, &cuenta.login)?;
    Ok(listar_local(gitea, cuenta)
        .await
        .into_iter()
        .filter(|repo| !huerfanos.contains(&repo.id))
        .collect())
}

/// Registra en la auditoría cada cambio destructivo detectado y ya protegido en esta
/// pasada: repo, tipo de cambio y marca protegida, con los SHA
/// completos (no son secretos, y son imprescindibles para poder recuperar la captura). Un
/// fallo al auditar un cambio concreto se registra con `tracing::warn!` y no impide
/// auditar los demás ni afecta al resultado de la sincronización.
fn registrar_proteccion(almacen: &Almacen, cuenta: &Cuenta, informe: &InformeProteccion) {
    for cambio in &informe.cambios {
        if let Err(error) = almacen.auditar(
            Some(&cuenta.login),
            "snapshots.cambio-destructivo",
            &detalle_cambio(cambio),
        ) {
            tracing::warn!(
                cuenta = %cuenta.login,
                error = %error,
                "no se pudo auditar un cambio destructivo detectado en los snapshots"
            );
        }
    }
}

/// Detalle de auditoría de un [`CambioDetectado`]: repo, marca protegida y cada cambio
/// (con sus SHA completos).
fn detalle_cambio(cambio: &CambioDetectado) -> String {
    let tipos: Vec<String> = cambio.cambios.iter().map(descripcion_cambio).collect();
    format!(
        "repo={} marca-protegida={} cambios=[{}]",
        cambio.id,
        cambio.marca_protegida,
        tipos.join("; ")
    )
}

fn descripcion_cambio(cambio: &CambioDestructivo) -> String {
    match cambio {
        CambioDestructivo::RamaBorrada { rama } => format!("rama-borrada:{rama}"),
        CambioDestructivo::TagBorrado { tag } => format!("tag-borrado:{tag}"),
        CambioDestructivo::TagMovido { tag, antes, ahora } => {
            format!("tag-movido:{tag}:{antes}->{ahora}")
        }
        CambioDestructivo::HistoriaReescrita { rama, antes, ahora } => {
            format!("historia-reescrita:{rama}:{antes}->{ahora}")
        }
    }
}

/// Repos de `cuenta` (login + organizaciones en alcance) ya conocidos como huérfanos en
/// el almacén. Se consulta **después** de que [`sincronizar_con`] haya persistido el
/// resultado de esta pasada, así que ya incluye los recién marcados huérfanos y excluye
/// los recién reanudados.
fn huerfanos_conocidos(
    almacen: &Almacen,
    login: &Nombre,
) -> Result<BTreeSet<IdRepo>, ErrorCuentas> {
    Ok(almacen
        .estados_de(login)?
        .into_iter()
        .filter(|estado| estado.estado == EstadoRepo::Huerfano)
        .map(|estado| estado.id)
        .collect())
}

/// Repos locales de `cuenta` (login + organizaciones), listados de nuevo contra Gitea.
/// Un fallo al listar un dueño concreto no es fatal aquí: sencillamente no aporta sus
/// repos a esta pasada de verificación (la propia sincronización ya ha tratado ese fallo
/// como corresponde).
async fn listar_local<T: ApiGitea>(gitea: &T, cuenta: &Cuenta) -> Vec<RepoLocal> {
    let mut duenos: Vec<Nombre> = vec![cuenta.login.clone()];
    duenos.extend(cuenta.alcance.organizaciones.iter().cloned());

    let mut local = Vec::new();
    for dueno in &duenos {
        if let Ok(repos) = gitea.repos_de(dueno).await {
            local.extend(repos);
        }
    }
    local
}

fn clave(id: &IdRepo) -> (String, String) {
    (
        id.dueno.as_str().to_lowercase(),
        id.nombre.as_str().to_lowercase(),
    )
}

/// `origen` a partir del plan de esta pasada (repos recién creados, con datos reales de
/// GitHub), completado con una entrada mínima para cada repo de `local` que no estuviera
/// en el plan: sin `rama_por_defecto`, así que `verificar_cuenta` no le compara el SHA
/// contra GitHub, pero tampoco lo trata como huérfano solo por no tener datos frescos.
fn completar_origen(informe_sync: &InformeSync, local: &[RepoLocal]) -> Vec<RepoOrigen> {
    // El listado real de GitHub de esta pasada manda: con él sí se comparan los SHAs.
    let mut mapa: BTreeMap<(String, String), RepoOrigen> = informe_sync
        .origen
        .iter()
        .cloned()
        .chain(
            informe_sync
                .plan
                .acciones
                .iter()
                .filter_map(|accion| match accion {
                    Accion::CrearMirror(repo) => Some(repo.clone()),
                    _ => None,
                }),
        )
        .map(|repo| (clave(&repo.id), repo))
        .collect();

    for repo in local {
        mapa.entry(clave(&repo.id)).or_insert_with(|| RepoOrigen {
            id: repo.id.clone(),
            url_clon: String::new(),
            privado: repo.privado,
            es_fork: false,
            archivado: false,
            rama_por_defecto: None,
            descripcion: None,
            tamano_kb: repo.tamano_kb,
        });
    }
    mapa.into_values().collect()
}

/// Ejecuta `verificar_cuenta` para `cuenta` sobre `local` (ya listado por el llamador,
/// ver [`sincronizar_y_verificar_con`]), completando `origen` según se explica ahí.
async fn ejecutar_verificacion<G: ApiGithub>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    github: &G,
    informe_sync: &InformeSync,
    local: &[RepoLocal],
    opciones_verificacion: &OpcionesVerificacion,
) -> Result<InformeVerificacion, ErrorCuentas> {
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let origen = completar_origen(informe_sync, local);

    let ahora = OffsetDateTime::now_utc();
    let antiguedad_de_la_cuenta = almacen
        .primera_sincronizacion(&cuenta.login)?
        .map(|primera| ahora - primera.inicio);
    let contexto = ContextoVerificacion {
        antiguedad_de_la_cuenta,
        en_contingencia: almacen
            .contingencias_de(&cuenta.login)?
            .into_iter()
            .collect(),
        ..ContextoVerificacion::default()
    };

    Ok(verificar_cuenta(
        github,
        cuenta,
        &rutas_cuenta,
        &origen,
        local,
        &contexto,
        opciones_verificacion,
    )
    .await)
}

/// Persiste los diagnósticos de una pasada de verificación con `guardar_estado_repo`,
/// igual que [`persistir_informe`] hace con los de sincronización. El detalle guardado
/// es la lista de motivos en español (ver [`crate::verificacion::Motivo`]).
fn persistir_verificacion(
    almacen: &Almacen,
    informe: &InformeVerificacion,
) -> Result<(), ErrorCuentas> {
    for diagnostico in &informe.diagnosticos {
        let detalle = if diagnostico.motivos.is_empty() {
            None
        } else {
            Some(
                diagnostico
                    .motivos
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        almacen.guardar_estado_repo(
            &informe.cuenta.login,
            &diagnostico.id,
            diagnostico.estado,
            detalle.as_deref(),
        )?;
    }
    Ok(())
}

/// Reconstruye el [`EstadoPrevio`] que necesita `sync::planificar`/`sync::ejecutar` a
/// partir de lo guardado en el almacén. Solo se reconstruyen los huérfanos: no hay
/// ninguna columna que guarde el último intervalo aplicado a cada mirror, así que
/// `intervalos_aplicados` queda vacío (ver el informe de la tarea).
fn reconstruir_estado_previo(
    almacen: &Almacen,
    login: &Nombre,
) -> Result<EstadoPrevio, ErrorCuentas> {
    let mut previo = EstadoPrevio::default();
    for estado in almacen.estados_de(login)? {
        if estado.estado == EstadoRepo::Huerfano {
            previo.huerfanos.insert(estado.id);
        }
    }
    Ok(previo)
}

/// Persiste un [`InformeSync`]: el estado de cada repo tocado, el histórico de la
/// sincronización y, solo si hubo cambios o fallos, una entrada de auditoría (para no
/// inundarla cada 30 minutos).
pub(super) fn persistir_informe(
    almacen: &Almacen,
    informe: &InformeSync,
) -> Result<(), ErrorCuentas> {
    for resultado in &informe.resultados {
        let estado = estado_repo_de(resultado.accion, &resultado.resultado);
        let detalle = match &resultado.resultado {
            ResultadoAccion::Error(mensaje) => Some(mensaje.as_str()),
            ResultadoAccion::Ok => None,
        };
        almacen.guardar_estado_repo(&informe.cuenta.login, &resultado.id, estado, detalle)?;
    }
    // Visibilidad real, fork y archivado: la copia local no los conserva.
    almacen.guardar_origenes(&informe.cuenta.login, &informe.origen)?;

    let hubo_fallos = informe.hay_fallos();
    let resultado_global = if hubo_fallos {
        ResultadoSincronizacion::ConFallos
    } else {
        ResultadoSincronizacion::Ok
    };
    let creados = contar(&informe.plan.acciones, |a| {
        matches!(a, Accion::CrearMirror(_))
    });
    let huerfanos = contar(&informe.plan.acciones, |a| {
        matches!(a, Accion::MarcarHuerfano(_))
    });
    let fallos = informe
        .resultados
        .iter()
        .filter(|r| matches!(r.resultado, ResultadoAccion::Error(_)))
        .count() as u32;
    let detalle_json = serde_json::to_string(informe).ok();

    almacen.registrar_sincronizacion(&NuevaSincronizacion {
        cuenta: informe.cuenta.login.clone(),
        inicio: informe.inicio,
        fin: informe.fin,
        resultado: resultado_global,
        creados,
        huerfanos,
        fallos,
        resumen: informe.resumen(),
        detalle_json,
    })?;

    let hubo_cambios = !informe.plan.acciones.is_empty();
    if hubo_cambios || hubo_fallos {
        almacen.auditar(
            Some(&informe.cuenta.login),
            "sync.ejecutada",
            &informe.resumen(),
        )?;
    }

    Ok(())
}

fn estado_repo_de(accion: TipoAccion, resultado: &ResultadoAccion) -> EstadoRepo {
    if matches!(resultado, ResultadoAccion::Error(_)) {
        return EstadoRepo::Fallo;
    }
    match accion {
        TipoAccion::MarcarHuerfano => EstadoRepo::Huerfano,
        TipoAccion::CrearMirror
        | TipoAccion::Reanudar
        | TipoAccion::AjustarIntervalo
        | TipoAccion::ForzarSincronizacion => EstadoRepo::Ok,
    }
}

fn contar<F: Fn(&Accion) -> bool>(acciones: &[Accion], f: F) -> u32 {
    acciones.iter().filter(|a| f(a)).count() as u32
}

/// Fuerza la sincronización inmediata de un único mirror ya existente (acción
/// «sincronizar ahora» de un repo concreto en la ventana), sin volver a listar GitHub ni
/// aplicar el resto del plan de la cuenta. `id` debe pertenecer a `cuenta` (login u
/// organizaciones en alcance): el llamador lo comprueba antes de llegar aquí.
///
/// Genérico sobre [`ApiGitea`] para poder probarlo con [`crate::cuentas::dobles::GiteaDoble`]
/// sin Gitea real; [`sincronizar_repo`] es la envoltura fina que construye el cliente real.
pub async fn sincronizar_repo_con<T: ApiGitea>(
    almacen: &Almacen,
    cuenta: &Cuenta,
    gitea: &T,
    id: &IdRepo,
) -> Result<SincronizacionDeRepo, ErrorCuentas> {
    // Un mirror en contingencia debe seguir congelado (es la base contra la que se
    // reconcilia) y uno excluido está en pausa por decisión del usuario.
    if almacen.punto_de_partida(&cuenta.login, id)?.is_some() {
        return Err(ErrorCuentas::RepoEnContingencia(id.clone()));
    }
    if cuenta.alcance.excluidos.contains(id) {
        return Err(ErrorCuentas::RepoExcluido(id.clone()));
    }
    if !gitea.salud().await.map_err(ErrorCuentas::from)? {
        return Err(ErrorCuentas::GiteaParado(cuenta.login.clone()));
    }
    let Some(local) = gitea.repo(id).await.map_err(ErrorCuentas::from)? else {
        return Err(ErrorCuentas::RepoNoExiste(id.clone()));
    };

    if clonado_inicial_fallido(almacen, cuenta, &local)? {
        // Única excepción a «cuentas nunca borra un repo»: no hay contenido que perder.
        gitea.borrar_repo(id).await.map_err(ErrorCuentas::from)?;
        almacen.borrar_estado_repo(&cuenta.login, id)?;
        almacen.auditar(Some(&cuenta.login), "repo.reclonar", &format!("id={id}"))?;
        return Ok(SincronizacionDeRepo::PendienteDeReclonar);
    }

    gitea
        .sincronizar_mirror(id)
        .await
        .map_err(ErrorCuentas::from)?;
    almacen.auditar(Some(&cuenta.login), "repo.sincronizar", &format!("id={id}"))?;
    Ok(SincronizacionDeRepo::Sincronizado)
}

/// Qué ha hecho [`sincronizar_repo_con`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SincronizacionDeRepo {
    /// Se ha pedido a Gitea que actualice el mirror.
    Sincronizado,
    /// El clonado inicial había fallado: se ha descartado el mirror vacío y la siguiente
    /// pasada de la cuenta lo creará de nuevo.
    PendienteDeReclonar,
}

/// Un mirror cuyo primer clonado se cortó (red caída, Gitea reiniciado) queda en Gitea
/// vacío para siempre: Gitea no lo reintenta y el plan no lo recrea porque «ya existe».
/// Solo se considera fallido si además la verificación ya lo marcó como fallo, es decir,
/// pasado el periodo de gracia: uno recién creado también está vacío mientras clona.
fn clonado_inicial_fallido(
    almacen: &Almacen,
    cuenta: &Cuenta,
    local: &RepoLocal,
) -> Result<bool, ErrorCuentas> {
    if !(local.es_mirror && local.vacio && local.ultima_sync.is_none()) {
        return Ok(false);
    }
    let en_fallo = almacen
        .estados_de(&cuenta.login)?
        .iter()
        .any(|guardado| guardado.id == local.id && guardado.estado == EstadoRepo::Fallo);
    Ok(en_fallo)
}

/// Igual que [`sincronizar_repo_con`], pero construye el cliente real de Gitea con el
/// token guardado en el llavero de `cuenta`.
pub async fn sincronizar_repo<L: Llavero>(
    contexto: &Contexto<'_, L>,
    cuenta: &Cuenta,
    id: &IdRepo,
) -> Result<SincronizacionDeRepo, ErrorCuentas> {
    let token_gitea = leer_secreto(contexto.llavero, &cuenta.login, ClaveSecreto::TokenGitea)?;
    let gitea = super::comun::cliente_gitea_de_cuenta(cuenta, token_gitea)?;
    sincronizar_repo_con(contexto.almacen, cuenta, &gitea, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::cuentas::dobles::{GiteaDoble, GithubDoble};
    use crate::modelo::{Alcance, IdRepo};
    use std::cell::RefCell;
    use std::path::PathBuf;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    fn cuenta(login: &str) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: PathBuf::from("/tmp/gitmereba-test-cuenta"),
            puerto: 33042,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    #[tokio::test]
    async fn gitea_parado_no_toca_github() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        let gitea = GiteaDoble::nueva();
        gitea.con_salud(false);

        let resultado = sincronizar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await;

        assert!(matches!(resultado, Err(ErrorCuentas::GiteaParado(_))));
        assert!(github.llamadas().is_empty(), "{:?}", github.llamadas());
    }

    #[tokio::test]
    async fn sincronizar_con_sin_usuarios_lan_no_crea_ningun_equipo() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["jparga"]);

        sincronizar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await
        .expect("sincronizar_con no falla");

        assert_eq!(
            gitea.miembros_de("jparga", super::super::usuarios_lan::EQUIPO_LECTURA),
            None,
            "sin usuarios LAN no debe crearse ningún equipo"
        );
    }

    #[tokio::test]
    async fn sincronizar_con_reconcilia_una_organizacion_nueva_con_los_usuarios_lan_ya_conocidos() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["jparga"]);

        // «ana» ya es un usuario LAN conocido (miembro del equipo de otra organización).
        super::super::usuarios_lan::reconciliar_permisos_lan(&gitea, &[nombre("ana")])
            .await
            .expect("preparar el usuario LAN ya conocido");

        // Ahora aparece una organización nueva: la propia sincronización debe reconciliarla.
        gitea.con_organizaciones(&["jparga", "nueva"]);

        sincronizar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await
        .expect("sincronizar_con no falla");

        assert_eq!(
            gitea.miembros_de("nueva", super::super::usuarios_lan::EQUIPO_LECTURA),
            Some(vec![nombre("ana")])
        );
    }

    #[tokio::test]
    async fn una_sincronizacion_feliz_se_registra_en_el_almacen() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "nuevo", false, false)]);
        let gitea = GiteaDoble::nueva();

        let informe = sincronizar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await
        .expect("sincronizar_con no falla");

        assert!(!informe.hay_fallos());
        assert_eq!(
            almacen
                .ultimas_sincronizaciones(Some(&nombre("jparga")), 10)
                .expect("leer histórico")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn la_sincronizacion_guarda_visibilidad_y_forks_de_github() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![
            GithubDoble::repo("jparga", "publico", false, false),
            GithubDoble::repo("jparga", "secreto", true, false),
            // Fuera del alcance (la cuenta no incluye forks): se guarda igualmente, para
            // que la ventana sepa que existe.
            GithubDoble::repo("jparga", "un-fork", false, true),
        ]);
        let gitea = GiteaDoble::nueva();

        sincronizar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await
        .expect("sincronizar_con no falla");

        let origenes = almacen.origenes_de(&nombre("jparga")).expect("leer");
        let resumen: Vec<(&str, bool, bool)> = origenes
            .iter()
            .map(|d| (d.id.nombre.as_str(), d.privado, d.es_fork))
            .collect();
        assert_eq!(
            resumen,
            vec![
                ("publico", false, false),
                ("secreto", true, false),
                ("un-fork", false, true),
            ]
        );
    }

    #[tokio::test]
    async fn los_huerfanos_persisten_entre_pasadas() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        let gitea = GiteaDoble::nueva();
        gitea.con_repos(
            "jparga",
            vec![crate::modelo::RepoLocal {
                id: id("jparga", "desaparecido"),
                es_mirror: true,
                vacio: false,
                privado: true,
                tamano_kb: 1,
                ultima_sync: None,
            }],
        );
        let c = cuenta("jparga");

        // Primera pasada: GitHub no informa nada, así que se marcaría huérfano... salvo
        // que la salvaguarda de `sync::planificar` (listado vacío) lo impida con un solo
        // mirror local. Añadimos un repo que sigue existiendo para desactivarla.
        gitea.con_repos(
            "jparga",
            vec![
                crate::modelo::RepoLocal {
                    id: id("jparga", "sigue"),
                    es_mirror: true,
                    vacio: false,
                    privado: true,
                    tamano_kb: 1,
                    ultima_sync: None,
                },
                crate::modelo::RepoLocal {
                    id: id("jparga", "desaparecido"),
                    es_mirror: true,
                    vacio: false,
                    privado: true,
                    tamano_kb: 1,
                    ultima_sync: None,
                },
            ],
        );
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "sigue", false, false)]);

        let primero = sincronizar_con(
            &almacen,
            &c,
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await
        .expect("primera pasada no falla");
        assert!(
            primero
                .plan
                .acciones
                .iter()
                .any(|a| matches!(a, Accion::MarcarHuerfano(_)))
        );

        // Segunda pasada: mismo estado en GitHub y Gitea. No debe repetir la acción de
        // marcar huérfano (ya lo sabe por `estados_de`).
        let segundo = sincronizar_con(
            &almacen,
            &c,
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
        )
        .await
        .expect("segunda pasada no falla");
        assert!(
            !segundo
                .plan
                .acciones
                .iter()
                .any(|a| matches!(a, Accion::MarcarHuerfano(_))),
            "no debería repetir MarcarHuerfano: {:?}",
            segundo.plan.acciones
        );
    }

    fn repo_local(
        dueno: &str,
        nombre_repo: &str,
        ultima_sync: Option<time::OffsetDateTime>,
    ) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre_repo),
            es_mirror: true,
            vacio: false,
            privado: true,
            tamano_kb: 5,
            ultima_sync,
        }
    }

    #[tokio::test]
    async fn sincronizar_y_verificar_con_devuelve_los_dos_informes() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "nuevo", false, false)]);
        let gitea = GiteaDoble::nueva();

        let resultado = sincronizar_y_verificar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
            &OpcionesVerificacion::default(),
        )
        .await
        .expect("no falla");

        assert!(!resultado.sync.hay_fallos());
        assert!(resultado.verificacion.is_some());
        // La protección de snapshots también se ejecuta tras la verificación, aunque en este doble no haya ningún bare real que capturar.
        assert!(resultado.proteccion.is_some());
    }

    #[tokio::test]
    async fn sincronizar_y_verificar_con_progreso_emite_verificando_tras_el_ultimo_aplicando() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "nuevo", false, false)]);
        let gitea = GiteaDoble::nueva();
        let eventos: RefCell<Vec<ProgresoSync>> = RefCell::new(Vec::new());
        let al_progresar = |evento: ProgresoSync| eventos.borrow_mut().push(evento);

        let resultado = sincronizar_y_verificar_con_progreso(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
            &OpcionesVerificacion::default(),
            &al_progresar,
        )
        .await
        .expect("no falla");

        assert!(!resultado.sync.hay_fallos());
        assert!(resultado.verificacion.is_some());

        let eventos = eventos.into_inner();
        let posicion_ultimo_aplicando = eventos
            .iter()
            .rposition(|evento| evento.fase == FaseSync::Aplicando)
            .expect("hay al menos un evento de la fase Aplicando");
        let posicion_verificando = eventos
            .iter()
            .position(|evento| evento.fase == FaseSync::Verificando)
            .expect("hay un evento de la fase Verificando");
        assert!(
            posicion_verificando > posicion_ultimo_aplicando,
            "Verificando debe llegar después del último Aplicando: {eventos:?}"
        );
    }

    #[tokio::test]
    async fn la_verificacion_no_sobrescribe_el_estado_huerfano_de_un_repo_recien_marcado() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        // "sigue" continúa en GitHub; "desaparecido" ya no está.
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "sigue", false, false)]);
        let gitea = GiteaDoble::nueva();
        gitea.con_repos(
            "jparga",
            vec![
                repo_local("jparga", "sigue", None),
                repo_local("jparga", "desaparecido", None),
            ],
        );

        let resultado = sincronizar_y_verificar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
            &OpcionesVerificacion::default(),
        )
        .await
        .expect("no falla");

        assert!(
            resultado
                .sync
                .plan
                .acciones
                .iter()
                .any(|a| matches!(a, Accion::MarcarHuerfano(_)))
        );

        let estados = almacen.estados_de(&nombre("jparga")).expect("leer estados");
        let de_desaparecido = estados
            .iter()
            .find(|e| e.id == id("jparga", "desaparecido"))
            .expect("desaparecido tiene estado guardado");
        assert_eq!(
            de_desaparecido.estado,
            EstadoRepo::Huerfano,
            "la verificación no debe sobrescribir el huérfano que acaba de marcar sync"
        );
    }

    #[tokio::test]
    async fn la_verificacion_persiste_el_estado_de_un_repo_no_tocado_por_sync() {
        // "sigue" ya existía como mirror y no necesita ninguna acción de `sync` en esta
        // pasada (no aparece en `resultados`, así que `persistir_informe` no lo toca).
        // La verificación, en cambio, sí lo evalúa (con el `origen` mínimo completado) y
        // lo persiste como `Ok`.
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "sigue", false, false)]);
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local("jparga", "sigue", None)]);

        let resultado = sincronizar_y_verificar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
            &OpcionesVerificacion::default(),
        )
        .await
        .expect("no falla");

        // `sync` no ha tenido que hacer nada con "sigue": ninguna acción ni resultado.
        assert!(resultado.sync.resultados.is_empty());

        let verificacion = resultado.verificacion.expect("hay informe de verificación");
        assert_eq!(verificacion.diagnosticos.len(), 1);
        assert_eq!(verificacion.diagnosticos[0].id, id("jparga", "sigue"));
        assert_eq!(verificacion.diagnosticos[0].estado, EstadoRepo::Ok);

        let estados = almacen.estados_de(&nombre("jparga")).expect("leer estados");
        let de_sigue = estados
            .iter()
            .find(|e| e.id == id("jparga", "sigue"))
            .expect("sigue tiene estado guardado gracias a la verificación");
        assert_eq!(de_sigue.estado, EstadoRepo::Ok);
    }

    #[test]
    fn completar_origen_usa_el_listado_real_de_github_para_los_mirrors_no_tocados() {
        let ahora = time::OffsetDateTime::now_utc();
        let informe = InformeSync {
            cuenta: cuenta("jparga"),
            inicio: ahora,
            fin: ahora,
            caduca_token: None,
            plan: crate::sync::Plan {
                acciones: Vec::new(),
                omitidos: Vec::new(),
                alerta: None,
            },
            resultados: Vec::new(),
            errores_de_listado: Vec::new(),
            alerta: None,
            origen: vec![RepoOrigen {
                id: id("jparga", "estable"),
                url_clon: "https://github.com/jparga/estable.git".to_string(),
                privado: false,
                es_fork: false,
                archivado: false,
                rama_por_defecto: Some("main".to_string()),
                descripcion: None,
                tamano_kb: 9,
            }],
        };
        let local = [
            repo_local("jparga", "estable", None),
            repo_local("jparga", "sin-listado", None),
        ];

        let origen = completar_origen(&informe, &local);

        let estable = origen
            .iter()
            .find(|r| r.id == id("jparga", "estable"))
            .expect("estable está en el origen");
        assert_eq!(
            estable.rama_por_defecto.as_deref(),
            Some("main"),
            "un mirror sin acciones en la pasada conserva su rama: se le compara el SHA"
        );
        let relleno = origen
            .iter()
            .find(|r| r.id == id("jparga", "sin-listado"))
            .expect("el repo sin listado recibe una entrada mínima");
        assert_eq!(
            relleno.rama_por_defecto, None,
            "sin datos de GitHub no se compara SHA"
        );
    }

    fn repo_local_vacio(dueno: &str, nombre_repo: &str) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre_repo),
            es_mirror: true,
            vacio: true,
            privado: true,
            tamano_kb: 0,
            ultima_sync: None,
        }
    }

    #[tokio::test]
    async fn con_primera_sincronizacion_hace_dos_horas_un_mirror_nunca_sincronizado_es_fallo() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let hace_dos_horas = OffsetDateTime::now_utc() - time::Duration::hours(2);
        almacen
            .registrar_sincronizacion(&crate::almacen::NuevaSincronizacion {
                cuenta: nombre("jparga"),
                inicio: hace_dos_horas,
                fin: hace_dos_horas,
                resultado: ResultadoSincronizacion::Ok,
                creados: 0,
                huerfanos: 0,
                fallos: 0,
                resumen: "sincronización previa de prueba".to_string(),
                detalle_json: None,
            })
            .expect("registrar la primera sincronización de prueba");

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "vacio", false, false)]);
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_vacio("jparga", "vacio")]);

        let resultado = sincronizar_y_verificar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
            &OpcionesVerificacion::default(),
        )
        .await
        .expect("no falla");

        let verificacion = resultado.verificacion.expect("hay informe de verificación");
        let diagnostico = verificacion
            .diagnosticos
            .iter()
            .find(|d| d.id == id("jparga", "vacio"))
            .expect("hay diagnóstico para «vacio»");
        assert_eq!(diagnostico.estado, EstadoRepo::Fallo);
    }

    #[tokio::test]
    async fn con_la_cuenta_recien_creada_un_mirror_nunca_sincronizado_no_es_fallo() {
        // Sin ninguna sincronización previa registrada: esta misma pasada es la primera,
        // así que la antigüedad conocida es prácticamente cero y no supera la gracia.
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "vacio", false, false)]);
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_vacio("jparga", "vacio")]);

        let resultado = sincronizar_y_verificar_con(
            &almacen,
            &cuenta("jparga"),
            &github,
            &gitea,
            &Secreto::nuevo("ghp_x"),
            &OpcionesSync::default(),
            &OpcionesVerificacion::default(),
        )
        .await
        .expect("no falla");

        let verificacion = resultado.verificacion.expect("hay informe de verificación");
        let diagnostico = verificacion
            .diagnosticos
            .iter()
            .find(|d| d.id == id("jparga", "vacio"))
            .expect("hay diagnóstico para «vacio»");
        assert_eq!(diagnostico.estado, EstadoRepo::Ok);
    }

    fn repo_local_simple(dueno: &str, nombre_repo: &str) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre_repo),
            es_mirror: true,
            vacio: false,
            privado: false,
            tamano_kb: 10,
            ultima_sync: None,
        }
    }

    #[tokio::test]
    async fn sincronizar_repo_con_fuerza_solo_ese_mirror() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_simple("jparga", "uno")]);
        let repo = id("jparga", "uno");

        sincronizar_repo_con(&almacen, &cuenta("jparga"), &gitea, &repo)
            .await
            .expect("sincronizar_repo_con no falla");

        assert_eq!(gitea.llamadas_sincronizar_mirror(), vec![repo]);
        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "repo.sincronizar"));
    }

    #[tokio::test]
    async fn sincronizar_repo_con_no_toca_un_mirror_en_contingencia() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_simple("jparga", "uno")]);
        let repo = id("jparga", "uno");
        let cuenta = cuenta("jparga");
        almacen
            .guardar_punto_de_partida(&cuenta.login, &repo, &Default::default())
            .expect("guardar punto de partida");

        let resultado = sincronizar_repo_con(&almacen, &cuenta, &gitea, &repo).await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::RepoEnContingencia(_))
        ));
        assert!(gitea.llamadas_sincronizar_mirror().is_empty());
    }

    #[tokio::test]
    async fn sincronizar_repo_con_no_toca_un_repo_excluido() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_simple("jparga", "uno")]);
        let repo = id("jparga", "uno");
        let mut cuenta = cuenta("jparga");
        cuenta.alcance.excluidos.push(repo.clone());

        let resultado = sincronizar_repo_con(&almacen, &cuenta, &gitea, &repo).await;

        assert!(matches!(resultado, Err(ErrorCuentas::RepoExcluido(_))));
        assert!(gitea.llamadas_sincronizar_mirror().is_empty());
    }

    #[tokio::test]
    async fn sincronizar_repo_con_falla_si_gitea_esta_parado() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let gitea = GiteaDoble::nueva();
        gitea.con_salud(false);

        let resultado =
            sincronizar_repo_con(&almacen, &cuenta("jparga"), &gitea, &id("jparga", "uno")).await;

        assert!(matches!(resultado, Err(ErrorCuentas::GiteaParado(_))));
    }

    #[tokio::test]
    async fn sincronizar_repo_con_falla_si_el_repo_no_existe_en_gitea() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let gitea = GiteaDoble::nueva();

        let resultado = sincronizar_repo_con(
            &almacen,
            &cuenta("jparga"),
            &gitea,
            &id("jparga", "no-existe"),
        )
        .await;

        assert!(matches!(resultado, Err(ErrorCuentas::RepoNoExiste(_))));
        assert!(gitea.llamadas_sincronizar_mirror().is_empty());
    }

    #[tokio::test]
    async fn un_clonado_inicial_fallido_se_descarta_para_volver_a_clonarlo() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let c = cuenta("jparga");
        let repo = id("jparga", "clocal");
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_vacio("jparga", "clocal")]);
        // La verificación ya lo dio por fallido (pasó el periodo de gracia).
        almacen
            .guardar_estado_repo(&c.login, &repo, EstadoRepo::Fallo, Some("nunca sincronizó"))
            .expect("guardar estado");

        let resultado = sincronizar_repo_con(&almacen, &c, &gitea, &repo)
            .await
            .expect("no falla");

        assert_eq!(resultado, SincronizacionDeRepo::PendienteDeReclonar);
        assert_eq!(gitea.borrados(), vec![repo.clone()]);
        assert!(gitea.llamadas_sincronizar_mirror().is_empty());
        // Sin estado guardado, la siguiente pasada lo trata como un repo nuevo.
        assert!(almacen.estados_de(&c.login).expect("leer").is_empty());
    }

    #[tokio::test]
    async fn un_mirror_vacio_que_aun_esta_clonando_no_se_descarta() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let c = cuenta("jparga");
        let repo = id("jparga", "recien-creado");
        let gitea = GiteaDoble::nueva();
        gitea.con_repos("jparga", vec![repo_local_vacio("jparga", "recien-creado")]);
        almacen
            .guardar_estado_repo(&c.login, &repo, EstadoRepo::Ok, None)
            .expect("guardar estado");

        let resultado = sincronizar_repo_con(&almacen, &c, &gitea, &repo)
            .await
            .expect("no falla");

        assert_eq!(resultado, SincronizacionDeRepo::Sincronizado);
        assert!(gitea.borrados().is_empty());
        assert_eq!(gitea.llamadas_sincronizar_mirror(), vec![repo]);
    }
}
