//! Ejecuta el plan de sincronización contra GitHub y Gitea.

use std::collections::BTreeSet;

use time::OffsetDateTime;

use crate::gitea::{ApiGitea, ErrorGitea, PeticionMirror};
use crate::github::ApiGithub;
use crate::modelo::{Cuenta, IdRepo, Nombre, RepoLocal};
use crate::secretos::Secreto;

use super::error::ErrorSync;
use super::informe::{ErrorListado, InformeSync, ResultadoAccion, ResultadoRepo, TipoAccion};
use super::plan::{Accion, EstadoPrevio, planificar};
use super::progreso::{FaseSync, ProgresoSync};

/// Cuántos repos se procesarían a la vez como máximo, por defecto.
pub const CONCURRENCIA_POR_DEFECTO: usize = 3;

/// Longitud máxima de un mensaje de error saneado en el informe.
const LONGITUD_MAXIMA_MENSAJE: usize = 500;

/// Opciones de una pasada de sincronización.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcionesSync {
    /// Cuántos repos se procesarían a la vez como máximo. Ver la nota en [`ejecutar`]
    /// sobre por qué, de momento, el procesamiento es secuencial.
    pub concurrencia: usize,
    /// Si es `true`, no se aplica nada: el informe solo lleva el plan.
    pub simulacro: bool,
    /// Si es `true`, tras crear, reanudar o ajustar, se fuerza una sincronización
    /// inmediata de esos mirrors.
    pub forzar_sync: bool,
}

impl Default for OpcionesSync {
    fn default() -> Self {
        Self {
            concurrencia: CONCURRENCIA_POR_DEFECTO,
            simulacro: false,
            forzar_sync: false,
        }
    }
}

/// Sincroniza el Gitea local de `cuenta` con lo que hay en GitHub.
///
/// `estado_previo` no estaba previsto en la firma de esta función, pero sí en
/// la de [`planificar`], que lo necesita: `sync` no depende de `almacen` (no existe
/// todavía) y no persiste nada, así que quien llame a `ejecutar` tiene que reconstruir
/// este valor a partir del [`InformeSync`] anterior y pasarlo aquí.
///
/// # Concurrencia
///
/// `opciones.concurrencia` se acepta para uso futuro, pero el procesamiento es
/// secuencial. `ApiGithub`/`ApiGitea` se consumen por trait con genéricos (nunca `dyn`,
/// ver sus módulos) para no forzar `Box<dyn Trait>` en el código real, así que sus
/// futuros no tienen por qué ser `Send`. Eso descarta `tokio::task::JoinSet` y
/// `tokio::spawn`, que lo exigen; `spawn_local` exigiría además que las referencias
/// `&G`/`&T` fueran `'static`, cosa que esta firma genérica no puede garantizar sin
/// imponérselo también al llamador. `tokio::join!` solo sirve para una aridad fija en
/// tiempo de compilación, no para un lote de tamaño `opciones.concurrencia` decidido en
/// tiempo de ejecución. Acotar la concurrencia real de forma limpia con esos genéricos
/// requeriría una dependencia nueva (p. ej. `futures::stream::buffer_unordered`), que la
/// tarea prohíbe añadir. Por eso se procesa un repo detrás de otro: un fallo en uno no
/// detiene a los demás (se recoge en el informe), pero no hay paralelismo real todavía.
pub async fn ejecutar<G: ApiGithub, T: ApiGitea>(
    github: &G,
    gitea: &T,
    cuenta: &Cuenta,
    token_github: &Secreto,
    opciones: &OpcionesSync,
    estado_previo: &EstadoPrevio,
) -> Result<InformeSync, ErrorSync> {
    ejecutar_con_progreso(
        github,
        gitea,
        cuenta,
        token_github,
        opciones,
        estado_previo,
        &|_| {},
    )
    .await
}

/// Igual que [`ejecutar`], pero además informa del avance real de la pasada llamando a
/// `al_progresar` con un [`ProgresoSync`]: `Listando` al empezar, `Aplicando` antes de la
/// primera acción del plan y tras cada una (con éxito o con fallo, `total` es
/// `plan.acciones.len()`), y nada más (la fase `Verificando` no es cosa de `sync`: la
/// añade `cuentas::sincronizar_y_verificar_con_progreso`). `ejecutar` es esta misma
/// función con un cierre vacío, para no obligar a sus llamadores actuales a pasar uno.
pub async fn ejecutar_con_progreso<G: ApiGithub, T: ApiGitea>(
    github: &G,
    gitea: &T,
    cuenta: &Cuenta,
    token_github: &Secreto,
    opciones: &OpcionesSync,
    estado_previo: &EstadoPrevio,
    al_progresar: &dyn Fn(ProgresoSync),
) -> Result<InformeSync, ErrorSync> {
    let inicio = OffsetDateTime::now_utc();
    al_progresar(ProgresoSync {
        fase: FaseSync::Listando,
        hechos: 0,
        total: 0,
    });

    let identidad = github.identidad().await?;
    if identidad.login.as_str().to_lowercase() != cuenta.login.as_str().to_lowercase() {
        return Err(ErrorSync::TokenDeOtraCuenta {
            esperado: cuenta.login.clone(),
            obtenido: identidad.login,
        });
    }

    let mut origen = Vec::new();
    let mut errores_de_listado = Vec::new();
    // Dueños (en minúsculas) para los que no se puede confiar en el listado de este
    // pase: ni el de GitHub ni el de Gitea son fiables, así que no se marcan huérfanos.
    let mut duenos_con_error: BTreeSet<String> = BTreeSet::new();

    match github.repos_de_usuario().await {
        Ok(repos) => origen.extend(repos),
        Err(e) => {
            errores_de_listado.push(ErrorListado {
                dueno: cuenta.login.clone(),
                mensaje: sanear(&e.to_string(), token_github),
            });
            duenos_con_error.insert(cuenta.login.as_str().to_lowercase());
        }
    }

    for org in &cuenta.alcance.organizaciones {
        match github.repos_de_organizacion(org).await {
            Ok(repos) => origen.extend(repos),
            Err(e) => {
                errores_de_listado.push(ErrorListado {
                    dueno: org.clone(),
                    mensaje: sanear(&e.to_string(), token_github),
                });
                duenos_con_error.insert(org.as_str().to_lowercase());
            }
        }
    }

    let mut duenos_necesarios: Vec<Nombre> = vec![cuenta.login.clone()];
    duenos_necesarios.extend(cuenta.alcance.organizaciones.iter().cloned());

    let mut local: Vec<RepoLocal> = Vec::new();
    for dueno in &duenos_necesarios {
        if let Err(e) = gitea.asegurar_organizacion(dueno).await {
            errores_de_listado.push(ErrorListado {
                dueno: dueno.clone(),
                mensaje: sanear(&e.to_string(), token_github),
            });
            duenos_con_error.insert(dueno.as_str().to_lowercase());
            continue;
        }
        match gitea.repos_de(dueno).await {
            Ok(repos) => local.extend(repos),
            Err(e) => {
                errores_de_listado.push(ErrorListado {
                    dueno: dueno.clone(),
                    mensaje: sanear(&e.to_string(), token_github),
                });
                duenos_con_error.insert(dueno.as_str().to_lowercase());
            }
        }
    }

    // Nunca se marcan huérfanos de un dueño cuyo listado (en GitHub o en Gitea) ha
    // fallado en esta pasada: sencillamente no entra en la planificación.
    let local_para_plan: Vec<RepoLocal> = local
        .into_iter()
        .filter(|repo| !duenos_con_error.contains(&repo.id.dueno.as_str().to_lowercase()))
        .collect();

    let plan = planificar(cuenta, &origen, &local_para_plan, estado_previo);
    let alerta = plan.alerta.clone();

    if opciones.simulacro {
        al_progresar(ProgresoSync {
            fase: FaseSync::Aplicando,
            hechos: 0,
            total: 0,
        });
        return Ok(InformeSync {
            cuenta: cuenta.clone(),
            inicio,
            fin: OffsetDateTime::now_utc(),
            caduca_token: identidad.caduca,
            plan,
            resultados: Vec::new(),
            errores_de_listado,
            alerta,
            origen,
        });
    }

    let mut resultados = Vec::new();
    let mut objetivos_forzar: Vec<IdRepo> = Vec::new();
    let total_acciones = plan.acciones.len();
    al_progresar(ProgresoSync {
        fase: FaseSync::Aplicando,
        hechos: 0,
        total: total_acciones,
    });

    for (indice, accion) in plan.acciones.iter().enumerate() {
        let (id, tipo, resultado) = aplicar_accion(gitea, cuenta, token_github, accion).await;
        if opciones.forzar_sync
            && matches!(resultado, ResultadoAccion::Ok)
            && matches!(
                tipo,
                TipoAccion::CrearMirror | TipoAccion::Reanudar | TipoAccion::AjustarIntervalo
            )
        {
            objetivos_forzar.push(id.clone());
        }
        resultados.push(ResultadoRepo {
            id,
            accion: tipo,
            resultado,
        });
        al_progresar(ProgresoSync {
            fase: FaseSync::Aplicando,
            hechos: indice + 1,
            total: total_acciones,
        });
    }

    if opciones.forzar_sync {
        for id in objetivos_forzar {
            let resultado = match gitea.sincronizar_mirror(&id).await {
                Ok(()) => ResultadoAccion::Ok,
                Err(e) => ResultadoAccion::Error(sanear(&e.to_string(), token_github)),
            };
            resultados.push(ResultadoRepo {
                id,
                accion: TipoAccion::ForzarSincronizacion,
                resultado,
            });
        }
    }

    Ok(InformeSync {
        cuenta: cuenta.clone(),
        inicio,
        fin: OffsetDateTime::now_utc(),
        caduca_token: identidad.caduca,
        plan,
        resultados,
        errores_de_listado,
        alerta,
        origen,
    })
}

async fn aplicar_accion<T: ApiGitea>(
    gitea: &T,
    cuenta: &Cuenta,
    token_github: &Secreto,
    accion: &Accion,
) -> (IdRepo, TipoAccion, ResultadoAccion) {
    match accion {
        Accion::CrearMirror(repo) => {
            let peticion = PeticionMirror {
                url_clon: repo.url_clon.clone(),
                // Mínimo privilegio: el token solo hace falta para clonar repos
                // privados; los públicos se clonan sin credenciales.
                token: if repo.privado {
                    Some(token_github.clone())
                } else {
                    None
                },
                dueno: repo.id.dueno.clone(),
                nombre: repo.id.nombre.clone(),
                intervalo: format!("{}m", cuenta.intervalo_minutos),
                // Siempre privado en local, con independencia de la visibilidad real en
                // GitHub.
                privado: true,
                descripcion: repo.descripcion.clone(),
            };
            let resultado = match gitea.crear_mirror(&peticion).await {
                // Que ya exista es un éxito: el mirror que queríamos ya está ahí.
                Ok(_) | Err(ErrorGitea::YaExiste) => ResultadoAccion::Ok,
                Err(e) => ResultadoAccion::Error(sanear(&e.to_string(), token_github)),
            };
            (repo.id.clone(), TipoAccion::CrearMirror, resultado)
        }
        Accion::MarcarHuerfano(id) => {
            let resultado = fijar_intervalo(gitea, id, "0", token_github).await;
            (id.clone(), TipoAccion::MarcarHuerfano, resultado)
        }
        Accion::Reanudar(id) => {
            let intervalo = format!("{}m", cuenta.intervalo_minutos);
            let resultado = fijar_intervalo(gitea, id, &intervalo, token_github).await;
            (id.clone(), TipoAccion::Reanudar, resultado)
        }
        Accion::AjustarIntervalo { id, minutos } => {
            let intervalo = format!("{minutos}m");
            let resultado = fijar_intervalo(gitea, id, &intervalo, token_github).await;
            (id.clone(), TipoAccion::AjustarIntervalo, resultado)
        }
    }
}

async fn fijar_intervalo<T: ApiGitea>(
    gitea: &T,
    id: &IdRepo,
    intervalo: &str,
    token_github: &Secreto,
) -> ResultadoAccion {
    match gitea.fijar_intervalo(id, intervalo).await {
        Ok(()) => ResultadoAccion::Ok,
        Err(e) => ResultadoAccion::Error(sanear(&e.to_string(), token_github)),
    }
}

/// Quita cualquier aparición literal del token de GitHub de un mensaje de error y lo
/// recorta. Defensa en profundidad: los mensajes de [`crate::github::ErrorGithub`] y
/// [`crate::gitea::ErrorGitea`] ya están pensados para no llevar secretos, pero un
/// mensaje de Gitea sobre un fallo de `migrate` puede citar la URL o el token que se le
/// mandó.
fn sanear(mensaje: &str, token_github: &Secreto) -> String {
    let valor = token_github.exponer();
    let oculto = if valor.is_empty() {
        mensaje.to_string()
    } else {
        mensaje.replace(valor, "***")
    };
    match oculto.char_indices().nth(LONGITUD_MAXIMA_MENSAJE) {
        Some((limite, _)) => oculto[..limite].to_string(),
        None => oculto,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::sync::dobles::pruebas::{cuenta, id, repo_local, repo_origen};
    use crate::sync::dobles::{FalloProgramado, GiteaDoble, GithubDoble};

    fn secreto() -> Secreto {
        Secreto::nuevo("ghp_token_de_prueba")
    }

    #[tokio::test]
    async fn token_de_otra_cuenta_no_hace_nada_mas() {
        let github = GithubDoble::con_identidad("otra-persona", None);
        let gitea = GiteaDoble::default();
        let c = cuenta("jparga", false, &[], &[], 30);
        let resultado = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await;
        assert!(matches!(
            resultado,
            Err(ErrorSync::TokenDeOtraCuenta { .. })
        ));
        assert!(gitea.organizaciones_aseguradas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn comparacion_de_login_insensible_a_mayusculas() {
        let github = GithubDoble::con_identidad("JParga", None);
        let gitea = GiteaDoble::default();
        let c = cuenta("jparga", false, &[], &[], 30);
        let resultado = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await;
        assert!(resultado.is_ok());
    }

    #[tokio::test]
    async fn alta_de_repo_nuevo_publico_sin_token_y_privado_con_token() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![
            repo_origen("jparga", "publico", false, false),
            repo_origen("jparga", "privado", true, false),
        ]);
        let gitea = GiteaDoble::default();
        let c = cuenta("jparga", false, &[], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert!(!informe.hay_fallos());
        let llamadas = gitea.llamadas_crear_mirror.lock().unwrap();
        assert_eq!(llamadas.len(), 2);
        let publico = llamadas
            .iter()
            .find(|p| p.nombre.as_str() == "publico")
            .unwrap();
        assert!(publico.token.is_none());
        // El mirror es siempre privado en local, sea cual sea la visibilidad real.
        assert!(publico.privado);
        let privado = llamadas
            .iter()
            .find(|p| p.nombre.as_str() == "privado")
            .unwrap();
        assert!(privado.token.is_some());
        assert!(privado.privado);
    }

    #[tokio::test]
    async fn ya_existe_al_crear_cuenta_como_exito() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![repo_origen("jparga", "repo1", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_fallo_crear_mirror(&id("jparga", "repo1"), FalloProgramado::YaExiste);
        let c = cuenta("jparga", false, &[], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();
        assert!(!informe.hay_fallos());
        assert_eq!(informe.resultados.len(), 1);
        assert_eq!(informe.resultados[0].resultado, ResultadoAccion::Ok);
    }

    #[tokio::test]
    async fn un_fallo_al_crear_no_impide_crear_los_demas() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![
            repo_origen("jparga", "falla", false, false),
            repo_origen("jparga", "bien", false, false),
        ]);
        let gitea = GiteaDoble::default();
        gitea.con_fallo_crear_mirror(
            &id("jparga", "falla"),
            FalloProgramado::Otro("fallo simulado".to_string()),
        );
        let c = cuenta("jparga", false, &[], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();
        assert!(informe.hay_fallos());
        assert_eq!(informe.resultados.len(), 2);
        let de_falla = informe
            .resultados
            .iter()
            .find(|r| r.id == id("jparga", "falla"))
            .unwrap();
        assert!(matches!(de_falla.resultado, ResultadoAccion::Error(_)));
        let de_bien = informe
            .resultados
            .iter()
            .find(|r| r.id == id("jparga", "bien"))
            .unwrap();
        assert_eq!(de_bien.resultado, ResultadoAccion::Ok);
    }

    #[tokio::test]
    async fn fallo_al_listar_una_organizacion_no_deja_huerfanos_pero_si_altas_del_resto() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_fallo_organizacion("rota");
        github.con_repos_organizacion("sana", vec![repo_origen("sana", "nuevo", false, false)]);
        let gitea = GiteaDoble::default();
        // "rota" tenía un mirror local que, de no protegerse, se marcaría huérfano.
        gitea.con_repos("rota", vec![repo_local("rota", "existente", true)]);
        let c = cuenta("jparga", false, &["rota", "sana"], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert_eq!(informe.errores_de_listado.len(), 1);
        assert_eq!(informe.errores_de_listado[0].dueno, id("rota", "x").dueno);
        assert!(
            informe
                .plan
                .acciones
                .iter()
                .all(|a| !matches!(a, Accion::MarcarHuerfano(_)))
        );
        assert!(
            informe
                .plan
                .acciones
                .iter()
                .any(|a| matches!(a, Accion::CrearMirror(repo) if repo.id == id("sana", "nuevo")))
        );
    }

    #[tokio::test]
    async fn simulacro_no_llama_a_ninguna_operacion_de_escritura() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![repo_origen("jparga", "nuevo", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_repos("jparga", vec![repo_local("jparga", "desaparecido", true)]);
        let c = cuenta("jparga", false, &[], &[], 30);
        let opciones = OpcionesSync {
            simulacro: true,
            ..OpcionesSync::default()
        };
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &opciones,
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert!(!informe.plan.acciones.is_empty());
        assert!(informe.resultados.is_empty());
        assert!(gitea.llamadas_crear_mirror.lock().unwrap().is_empty());
        assert!(gitea.llamadas_fijar_intervalo.lock().unwrap().is_empty());
        assert!(gitea.llamadas_sincronizar_mirror.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn huerfano_se_pausa_y_nunca_se_borra() {
        let github = GithubDoble::con_identidad("jparga", None);
        // Un repo que sigue existiendo, para que el listado de GitHub no esté vacío y
        // no salte la salvaguarda de la sección "salvaguarda" (que se prueba aparte).
        github.con_repos_usuario(vec![repo_origen("jparga", "sigue", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_repos(
            "jparga",
            vec![
                repo_local("jparga", "sigue", true),
                repo_local("jparga", "desaparecido", true),
            ],
        );
        let c = cuenta("jparga", false, &[], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert!(!informe.hay_fallos());
        let llamadas = gitea.llamadas_fijar_intervalo.lock().unwrap();
        assert_eq!(llamadas.len(), 1);
        assert_eq!(llamadas[0].1, "0");
        // Si `borrar_repo` se hubiera llamado, el doble habría entrado en pánico y este
        // test habría fallado antes de llegar aquí.
    }

    #[tokio::test]
    async fn forzar_sync_sincroniza_los_mirrors_creados_y_reanudados() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![
            repo_origen("jparga", "nuevo", false, false),
            repo_origen("jparga", "vuelve", false, false),
        ]);
        let gitea = GiteaDoble::default();
        gitea.con_repos("jparga", vec![repo_local("jparga", "vuelve", true)]);
        let mut previo = EstadoPrevio::default();
        previo.huerfanos.insert(id("jparga", "vuelve"));
        let c = cuenta("jparga", false, &[], &[], 30);
        let opciones = OpcionesSync {
            forzar_sync: true,
            ..OpcionesSync::default()
        };
        let informe = ejecutar(&github, &gitea, &c, &secreto(), &opciones, &previo)
            .await
            .unwrap();

        assert!(!informe.hay_fallos());
        let sincronizados = gitea.llamadas_sincronizar_mirror.lock().unwrap();
        assert_eq!(sincronizados.len(), 2);
        assert!(sincronizados.contains(&id("jparga", "nuevo")));
        assert!(sincronizados.contains(&id("jparga", "vuelve")));
    }

    #[tokio::test]
    async fn opciones_por_defecto() {
        let opciones = OpcionesSync::default();
        assert_eq!(opciones.concurrencia, 3);
        assert!(!opciones.simulacro);
        assert!(!opciones.forzar_sync);
    }

    #[tokio::test]
    async fn fallo_al_listar_los_repos_propios_no_deja_huerfanos_del_login() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_fallo_repos_usuario();
        github.con_repos_organizacion("acme", vec![repo_origen("acme", "nuevo", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_repos("jparga", vec![repo_local("jparga", "existente", true)]);
        let c = cuenta("jparga", false, &["acme"], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert_eq!(informe.errores_de_listado.len(), 1);
        assert!(
            informe
                .plan
                .acciones
                .iter()
                .all(|a| !matches!(a, Accion::MarcarHuerfano(_)))
        );
        assert!(
            informe
                .plan
                .acciones
                .iter()
                .any(|a| matches!(a, Accion::CrearMirror(repo) if repo.id == id("acme", "nuevo")))
        );
    }

    #[tokio::test]
    async fn fallo_al_asegurar_una_organizacion_se_anota_y_no_deja_huerfanos_de_ella() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_organizacion("acme", vec![repo_origen("acme", "nuevo", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_fallo_asegurar_organizacion("acme");
        // Si no se protegiera, este mirror se marcaría huérfano al no poder listarse.
        gitea.con_repos("acme", vec![repo_local("acme", "existente", true)]);
        let c = cuenta("jparga", false, &["acme"], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert_eq!(informe.errores_de_listado.len(), 1);
        assert!(
            informe
                .plan
                .acciones
                .iter()
                .all(|a| !matches!(a, Accion::MarcarHuerfano(_)))
        );
    }

    #[tokio::test]
    async fn fallo_al_listar_repos_de_gitea_de_un_dueno_intenta_crear_igualmente() {
        // Si Gitea no puede listar los repos de un dueño, no sabemos qué hay ya: se
        // intenta crear todo lo de GitHub para ese dueño (los que ya existan
        // responderán `YaExiste`, que cuenta como éxito) en vez de arriesgarnos a
        // marcar huérfano algo que sí existe.
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_organizacion("acme", vec![repo_origen("acme", "repo1", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_fallo_repos_de("acme");
        let c = cuenta("jparga", false, &["acme"], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert_eq!(informe.errores_de_listado.len(), 1);
        assert!(
            informe
                .plan
                .acciones
                .iter()
                .any(|a| matches!(a, Accion::CrearMirror(repo) if repo.id == id("acme", "repo1")))
        );
    }

    #[tokio::test]
    async fn un_fallo_al_pausar_un_huerfano_se_registra_y_no_detiene_a_los_demas() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![repo_origen("jparga", "sigue", false, false)]);
        let gitea = GiteaDoble::default();
        gitea.con_repos(
            "jparga",
            vec![
                repo_local("jparga", "sigue", true),
                repo_local("jparga", "rebelde", true),
            ],
        );
        gitea.con_fallo_fijar_intervalo(&id("jparga", "rebelde"));
        let c = cuenta("jparga", false, &[], &[], 30);
        let informe = ejecutar(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
        )
        .await
        .unwrap();

        assert!(informe.hay_fallos());
        let de_rebelde = informe
            .resultados
            .iter()
            .find(|r| r.id == id("jparga", "rebelde"))
            .unwrap();
        assert!(matches!(de_rebelde.resultado, ResultadoAccion::Error(_)));
    }

    #[tokio::test]
    async fn el_progreso_de_aplicando_termina_en_hechos_igual_a_total_y_nunca_decrece() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![
            repo_origen("jparga", "uno", false, false),
            repo_origen("jparga", "dos", false, false),
            repo_origen("jparga", "tres", false, false),
        ]);
        let gitea = GiteaDoble::default();
        // Un fallo en medio del plan no debe romper la monotonía del progreso: se sigue
        // avanzando igual, con éxito o con fallo.
        gitea.con_fallo_crear_mirror(
            &id("jparga", "dos"),
            FalloProgramado::Otro("fallo simulado".to_string()),
        );
        let c = cuenta("jparga", false, &[], &[], 30);
        let eventos: RefCell<Vec<ProgresoSync>> = RefCell::new(Vec::new());
        let al_progresar = |evento: ProgresoSync| eventos.borrow_mut().push(evento);

        let informe = ejecutar_con_progreso(
            &github,
            &gitea,
            &c,
            &secreto(),
            &OpcionesSync::default(),
            &EstadoPrevio::default(),
            &al_progresar,
        )
        .await
        .unwrap();
        assert!(informe.hay_fallos());

        let eventos = eventos.into_inner();
        assert!(!eventos.is_empty());
        let mut anterior = 0;
        for evento in &eventos {
            assert!(
                evento.hechos >= anterior,
                "hechos no debe decrecer nunca: {eventos:?}"
            );
            anterior = evento.hechos;
        }
        let ultimo_aplicando = eventos
            .iter()
            .rev()
            .find(|evento| evento.fase == FaseSync::Aplicando)
            .expect("hay al menos un evento de la fase Aplicando");
        assert_eq!(ultimo_aplicando.total, 3);
        assert_eq!(ultimo_aplicando.hechos, ultimo_aplicando.total);
        assert_eq!(eventos.first().unwrap().fase, FaseSync::Listando);
    }

    #[tokio::test]
    async fn el_simulacro_no_aplica_nada_y_solo_emite_aplicando_en_cero() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![repo_origen("jparga", "nuevo", false, false)]);
        let gitea = GiteaDoble::default();
        let c = cuenta("jparga", false, &[], &[], 30);
        let opciones = OpcionesSync {
            simulacro: true,
            ..OpcionesSync::default()
        };
        let eventos: RefCell<Vec<ProgresoSync>> = RefCell::new(Vec::new());
        let al_progresar = |evento: ProgresoSync| eventos.borrow_mut().push(evento);

        let informe = ejecutar_con_progreso(
            &github,
            &gitea,
            &c,
            &secreto(),
            &opciones,
            &EstadoPrevio::default(),
            &al_progresar,
        )
        .await
        .unwrap();
        assert!(!informe.plan.acciones.is_empty());

        let eventos = eventos.into_inner();
        assert_eq!(
            eventos,
            vec![
                ProgresoSync {
                    fase: FaseSync::Listando,
                    hechos: 0,
                    total: 0
                },
                ProgresoSync {
                    fase: FaseSync::Aplicando,
                    hechos: 0,
                    total: 0
                },
            ]
        );
    }
}
