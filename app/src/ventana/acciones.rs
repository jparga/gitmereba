//! Comandos de acción del contrato con la interfaz: todo lo que cambia algo (a diferencia
//! de `lectura.rs`, que solo lee). Envoltorios finos sobre `gitmereba_core`.

use std::path::PathBuf;
use std::process::Stdio;

use gitmereba_core::config;
use gitmereba_core::contingencia::{self, CredencialGitea, OrigenReconciliacion};
use gitmereba_core::cuentas::{
    self, AprovisionadorReal, EstadoPaso, LanzadorSystemd, PasoAlta, SolicitudAlta,
};
use gitmereba_core::gitea::ClienteGitea;
use gitmereba_core::github::{ApiGithub, ClienteGithub};
use gitmereba_core::instancia::{self, NOMBRE_ADMIN_GITEA};
use gitmereba_core::modelo::{Alcance, IdRepo, Nombre, RepoOrigen};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, Secreto};
use tauri::{Emitter, State};
use tauri_plugin_dialog::DialogExt;

use super::dto_acciones::{
    AjustesCuentaEntrada, CredencialesGiteaDto, InformeSincResumen, PreviaAlta,
    RespuestaActualizarGitea, RespuestaCarpeta, RespuestaOk, ResultadoReconciliacion, indice_paso,
    payload_progreso, payload_progreso_error, payload_progreso_sync,
};
use super::error::ErrorUi;
use super::estado::{EstadoApp, Recursos, en_hilo};

/// Intervalo de sincronización con el que se da de alta una cuenta desde la ventana. El
/// contrato (`validar_alta`/`crear_cuenta`) no expone un campo para elegirlo distinto
/// (a diferencia de `gitmereba cuenta add --intervalo`); se puede cambiar después desde
/// Ajustes (`ajustes_guardar`). Mismo valor que el `--intervalo` por defecto de la CLI.
const INTERVALO_MINUTOS_ALTA_POR_DEFECTO: u32 = 30;

#[tauri::command]
pub async fn excluir_repo(
    estado: State<'_, EstadoApp>,
    login: String,
    id: IdRepo,
    excluido: bool,
) -> Result<RespuestaOk, ErrorUi> {
    tracing::debug!(comando = "excluir_repo");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, rutas_cuenta) = recursos.cuenta(login.as_str())?;
        cuentas::excluir_repo(&recursos.almacen, &rutas_cuenta, &cuenta, &id, excluido)
            .map_err(|error| error_ui_repo(&error, &id))?;
        Ok(RespuestaOk::si())
    })
    .await
}

#[tauri::command]
pub async fn sincronizar(
    app: tauri::AppHandle,
    estado: State<'_, EstadoApp>,
    login: String,
    id: Option<IdRepo>,
) -> Result<InformeSincResumen, ErrorUi> {
    tracing::debug!(comando = "sincronizar");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _rutas_cuenta) = recursos.cuenta(login.as_str())?;

        match id {
            None => {
                // Construidos aquí (en vez de con el envoltorio `cuentas::sincronizar_y_verificar`,
                // que no acepta progreso) porque necesitamos pasar el cierre de avance hasta
                // `sync::ejecutar_con_progreso`; mismo patrón que `activar_contingencia` un poco
                // más abajo en este fichero.
                let token_github = token_github_de(&recursos, &cuenta.login)?;
                let token_gitea = token_gitea_de(&recursos, &cuenta.login)?;
                let github = ClienteGithub::nuevo(token_github.clone())
                    .map_err(|error| ErrorUi::de(&error))?;
                let gitea = cuentas::cliente_gitea_de_cuenta(&cuenta, token_gitea)
                    .map_err(|error| ErrorUi::de(&error))?;

                // Cuidado: este cierre se ejecuta dentro de `en_hilo` (otro hilo): el
                // `AppHandle` se clona antes de moverlo, igual que hace `crear_cuenta` con
                // `alta://progreso`.
                let login_evento = cuenta.login.to_string();
                let app_progreso = app.clone();
                let al_progresar = move |progreso: gitmereba_core::sync::ProgresoSync| {
                    let payload = payload_progreso_sync(&login_evento, progreso);
                    if let Err(error) = app_progreso.emit("sync://progreso", payload) {
                        tracing::warn!(
                            error = %error,
                            "no se pudo emitir el progreso de la sincronización"
                        );
                    }
                };

                let resultado = cuentas::sincronizar_y_verificar_con_progreso(
                    &recursos.almacen,
                    &cuenta,
                    &github,
                    &gitea,
                    &token_github,
                    &gitmereba_core::sync::OpcionesSync::default(),
                    &gitmereba_core::verificacion::OpcionesVerificacion::default(),
                    &al_progresar,
                )
                .await
                .map_err(|error| ErrorUi::de(&error))?;
                let informe = resultado.sync;
                let resultado_texto = if informe.hay_fallos() {
                    "con-fallos"
                } else {
                    "ok"
                };
                Ok(InformeSincResumen {
                    cuenta: informe.cuenta.clone(),
                    inicio: informe.inicio,
                    fin: informe.fin,
                    resultado: resultado_texto.to_string(),
                    resumen: informe.resumen(),
                })
            }
            Some(id) => {
                let inicio = time::OffsetDateTime::now_utc();
                let hecho = cuentas::sincronizar_repo(&recursos.contexto(), &cuenta, &id)
                    .await
                    .map_err(|error| error_ui_repo(&error, &id))?;
                let resumen = match hecho {
                    cuentas::SincronizacionDeRepo::Sincronizado => {
                        format!("Sincronización forzada de «{id}» en «{}».", cuenta.login)
                    }
                    cuentas::SincronizacionDeRepo::PendienteDeReclonar => {
                        // El mirror vacío ya no existe: la pasada lo crea de nuevo.
                        cuentas::sincronizar(
                            &recursos.contexto(),
                            &cuenta.login,
                            &gitmereba_core::sync::OpcionesSync::default(),
                        )
                        .await
                        .map_err(|error| ErrorUi::de(&error))?;
                        format!("El clonado inicial de «{id}» había fallado: se vuelve a clonar.")
                    }
                };
                let fin = time::OffsetDateTime::now_utc();
                Ok(InformeSincResumen {
                    cuenta: cuenta.clone(),
                    inicio,
                    fin,
                    resultado: "ok".to_string(),
                    resumen,
                })
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn elegir_carpeta(app: tauri::AppHandle) -> Result<RespuestaCarpeta, ErrorUi> {
    tracing::debug!(comando = "elegir_carpeta");
    en_hilo(move || async move {
        let seleccion = app.dialog().file().blocking_pick_folder();
        let carpeta = seleccion
            .and_then(|ruta| ruta.into_path().ok())
            .map(|ruta| ruta.display().to_string());
        Ok(RespuestaCarpeta { carpeta })
    })
    .await
}

#[tauri::command]
pub async fn validar_alta(
    estado: State<'_, EstadoApp>,
    usuario: String,
    token: String,
    carpeta: String,
) -> Result<PreviaAlta, ErrorUi> {
    let token = Secreto::nuevo(token);
    tracing::debug!(comando = "validar_alta");
    if token.esta_vacio() {
        return Err(ErrorUi::token_vacio());
    }
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let login = nombre_de(&usuario)?;
        let carpeta = PathBuf::from(carpeta);

        let indice = config::leer_indice_cuentas(&rutas).map_err(|error| ErrorUi::de(&error))?;
        let puertos: Vec<u16> = indice
            .cuentas
            .values()
            .map(|entrada| entrada.puerto)
            .collect();
        let puerto = instancia::puerto_libre(&puertos).map_err(|error| ErrorUi::de(&error))?;
        config::validar_alta(
            &rutas,
            &indice,
            login.as_str(),
            &carpeta,
            puerto,
            INTERVALO_MINUTOS_ALTA_POR_DEFECTO,
        )
        .map_err(|error| ErrorUi::de(&error))?;

        let github = ClienteGithub::nuevo(token.clone()).map_err(|error| ErrorUi::de(&error))?;
        let descubrimiento = cuentas::descubrir_repos(&github, &login)
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(PreviaAlta::from(descubrimiento))
    })
    .await
}

#[tauri::command]
pub async fn crear_cuenta(
    app: tauri::AppHandle,
    estado: State<'_, EstadoApp>,
    usuario: String,
    carpeta: String,
    token: String,
    repos: Vec<String>,
    organizaciones: Vec<String>,
) -> Result<RespuestaOk, ErrorUi> {
    let token = Secreto::nuevo(token);
    tracing::debug!(comando = "crear_cuenta");
    if token.esta_vacio() {
        return Err(ErrorUi::token_vacio());
    }
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let login = nombre_de(&usuario)?;
        let organizaciones = organizaciones
            .iter()
            .map(|org| nombre_de(org))
            .collect::<Result<Vec<_>, _>>()?;
        let marcados = repos
            .iter()
            .map(|texto| parsear_id_repo(texto))
            .collect::<Result<Vec<_>, _>>()?;

        let github = ClienteGithub::nuevo(token.clone()).map_err(|error| ErrorUi::de(&error))?;
        let mut universo = github
            .repos_de_usuario()
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        for organizacion in &organizaciones {
            let repos_org = github
                .repos_de_organizacion(organizacion)
                .await
                .map_err(|error| ErrorUi::de(&error))?;
            universo.extend(repos_org);
        }
        let excluidos = calcular_excluidos(&universo, &marcados);

        let solicitud = SolicitudAlta {
            login: login.clone(),
            token: token.clone(),
            carpeta: PathBuf::from(carpeta),
            alcance: Alcance {
                // Si el usuario marcó algún fork, los forks entran en el alcance (si no,
                // `sync::planificar` lo descartaría); los que no marcó van en `excluidos`.
                incluir_forks: universo
                    .iter()
                    .any(|repo| repo.es_fork && marcados.contains(&repo.id)),
                organizaciones,
                excluidos,
            },
            intervalo_minutos: INTERVALO_MINUTOS_ALTA_POR_DEFECTO,
            modo_pruebas: false,
        };

        let recursos = Recursos::abrir(&rutas)?;
        let contexto = recursos.contexto();
        let lanzador = LanzadorSystemd;
        let aprovisionador = AprovisionadorReal;

        let ultimo_paso = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1));
        let ultimo_paso_progreso = ultimo_paso.clone();
        let app_progreso = app.clone();
        let mut progreso = move |paso: PasoAlta, transicion: EstadoPaso| {
            ultimo_paso_progreso.store(indice_paso(paso), std::sync::atomic::Ordering::SeqCst);
            let payload = payload_progreso(paso, transicion);
            if let Err(error) = app_progreso.emit("alta://progreso", payload) {
                tracing::warn!(error = %error, "no se pudo emitir el progreso del alta");
            }
        };

        let resultado = cuentas::alta(
            &contexto,
            &solicitud,
            &github,
            ClienteGitea::nuevo,
            &lanzador,
            &aprovisionador,
            &mut progreso,
        )
        .await;

        match resultado {
            Ok(_) => Ok(RespuestaOk::si()),
            Err(error) => {
                let paso = ultimo_paso.load(std::sync::atomic::Ordering::SeqCst);
                let payload = payload_progreso_error(paso, error.to_string());
                let _ = app.emit("alta://progreso", payload);
                Err(ErrorUi::de(&error))
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn activar_contingencia(
    estado: State<'_, EstadoApp>,
    login: String,
    id: IdRepo,
) -> Result<RespuestaOk, ErrorUi> {
    tracing::debug!(comando = "activar_contingencia");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, rutas_cuenta) = recursos.cuenta(login.as_str())?;
        let token_gitea = token_gitea_de(&recursos, &cuenta.login)?;
        let gitea = cuentas::cliente_gitea_de_cuenta(&cuenta, token_gitea.clone())
            .map_err(|error| ErrorUi::de(&error))?;
        let credencial = CredencialGitea {
            usuario: NOMBRE_ADMIN_GITEA.to_string(),
            token: token_gitea,
        };

        let resultado = contingencia::activar(&gitea, &cuenta, &rutas_cuenta, &id, &credencial)
            .await
            .map_err(|error| error_ui_repo(&error, &id))?;

        recursos
            .almacen
            .guardar_punto_de_partida(&cuenta.login, &id, &resultado.punto_de_partida)
            .map_err(|error| ErrorUi::de(&error))?;
        recursos
            .almacen
            .auditar(
                Some(&cuenta.login),
                "contingencia.activar",
                &format!("id={id}"),
            )
            .map_err(|error| ErrorUi::de(&error))?;

        Ok(RespuestaOk::si())
    })
    .await
}

#[tauri::command]
pub async fn reconciliar(
    estado: State<'_, EstadoApp>,
    login: String,
    id: IdRepo,
    token: String,
) -> Result<ResultadoReconciliacion, ErrorUi> {
    let token = Secreto::nuevo(token);
    tracing::debug!(comando = "reconciliar");
    if token.esta_vacio() {
        return Err(ErrorUi::token_vacio());
    }
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, rutas_cuenta) = recursos.cuenta(login.as_str())?;

        let punto_de_partida = recursos
            .almacen
            .punto_de_partida(&cuenta.login, &id)
            .map_err(|error| ErrorUi::de(&error))?
            .ok_or_else(|| {
                ErrorUi::nuevo(
                    "no_activada",
                    format!("la contingencia de «{id}» no está activada"),
                )
            })?;
        let origen = OrigenReconciliacion::Github(format!(
            "https://github.com/{}/{}.git",
            id.dueno, id.nombre
        ));

        // El token de escritura viaja solo a `contingencia::reconciliar` (que lo pasa a
        // `git push`/`git fetch` como cabecera HTTP, nunca en la URL ni en argumentos) y
        // nunca se guarda en el llavero ni en ningún otro sitio.
        let informe =
            contingencia::reconciliar(&rutas_cuenta, &id, &punto_de_partida, &origen, &token)
                .await
                .map_err(|error| error_ui_repo(&error, &id))?;

        recursos
            .almacen
            .auditar(
                Some(&cuenta.login),
                "contingencia.reconciliar",
                &informe.resumen(),
            )
            .map_err(|error| ErrorUi::de(&error))?;

        // Reconciliación completa: el mirror original vuelve a sincronizarse y deja de
        // constar como contingencia. El repo hermano no se borra (eso exige confirmación
        // aparte); con divergencia no se cierra nada.
        if informe.completa {
            let token_gitea = token_gitea_de(&recursos, &cuenta.login)?;
            let gitea = cuentas::cliente_gitea_de_cuenta(&cuenta, token_gitea)
                .map_err(|error| ErrorUi::de(&error))?;
            contingencia::cerrar(
                &gitea,
                &rutas_cuenta,
                &id,
                cuenta.intervalo_minutos,
                &punto_de_partida,
                Some(&informe),
            )
            .await
            .map_err(|error| error_ui_repo(&error, &id))?;
            recursos
                .almacen
                .borrar_punto_de_partida(&cuenta.login, &id)
                .map_err(|error| ErrorUi::de(&error))?;
            recursos
                .almacen
                .auditar(
                    Some(&cuenta.login),
                    "contingencia.cerrar",
                    &format!("id={id}"),
                )
                .map_err(|error| ErrorUi::de(&error))?;
        }

        Ok(ResultadoReconciliacion {
            resultado: if informe.completa {
                "ok"
            } else {
                "divergencia"
            }
            .to_string(),
            mensaje: informe.resumen(),
        })
    })
    .await
}

#[tauri::command]
pub async fn ajustes_guardar(
    estado: State<'_, EstadoApp>,
    login: String,
    ajustes: AjustesCuentaEntrada,
) -> Result<RespuestaOk, ErrorUi> {
    tracing::debug!(comando = "ajustes_guardar");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, rutas_cuenta) = recursos.cuenta(login.as_str())?;

        if std::path::Path::new(&ajustes.carpeta) != cuenta.carpeta {
            return Err(ErrorUi::nuevo(
                "no_implementado",
                "mover la carpeta de una cuenta aún no está disponible",
            ));
        }

        cuentas::actualizar_ajustes(
            &recursos.almacen,
            &rutas_cuenta,
            &cuenta,
            ajustes.intervalo_minutos,
            ajustes.alcance,
        )
        .map_err(|error| ErrorUi::de(&error))?;

        // El intervalo vive también en el timer de systemd: sin esto no cambiaría hasta
        // volver a dar de alta la cuenta.
        let (actualizada, _rutas_cuenta) = recursos.cuenta(login.as_str())?;
        let ejecutable =
            std::env::current_exe().map_err(|error| ErrorUi::interno(error.to_string()))?;
        cuentas::reparar_temporizador(&recursos.rutas, &actualizada, &ejecutable)
            .await
            .map_err(|error| ErrorUi::de(&error))?;

        Ok(RespuestaOk::si())
    })
    .await
}

#[tauri::command]
pub async fn rotar_token(
    estado: State<'_, EstadoApp>,
    login: String,
    token: String,
) -> Result<RespuestaOk, ErrorUi> {
    let token = Secreto::nuevo(token);
    tracing::debug!(comando = "rotar_token");
    if token.esta_vacio() {
        return Err(ErrorUi::token_vacio());
    }
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _rutas_cuenta) = recursos.cuenta(login.as_str())?;
        let github = ClienteGithub::nuevo(token.clone()).map_err(|error| ErrorUi::de(&error))?;

        cuentas::rotar_token(
            &recursos.llavero,
            &recursos.almacen,
            &cuenta,
            &github,
            token,
        )
        .await
        .map_err(|error| ErrorUi::de(&error))?;

        Ok(RespuestaOk::si())
    })
    .await
}

#[tauri::command]
pub async fn baja_cuenta(
    estado: State<'_, EstadoApp>,
    login: String,
) -> Result<RespuestaOk, ErrorUi> {
    tracing::debug!(comando = "baja_cuenta");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _rutas_cuenta) = recursos.cuenta(login.as_str())?;
        let lanzador = LanzadorSystemd;
        // `borrar_datos: false` siempre: la carpeta y los repos de la cuenta no se
        // borran desde la ventana (comando `baja_cuenta` del contrato con la interfaz).
        cuentas::baja(&recursos.contexto(), &lanzador, &cuenta.login, false)
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(RespuestaOk::si())
    })
    .await
}

#[tauri::command]
pub async fn abrir_gitea(
    estado: State<'_, EstadoApp>,
    login: String,
) -> Result<RespuestaOk, ErrorUi> {
    tracing::debug!(comando = "abrir_gitea");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _rutas_cuenta) = recursos.cuenta(login.as_str())?;
        let url = cuenta.url_publica();

        // Sin shell, argumentos como lista, sin esperar a que termine y sin heredar
        // stdin: `xdg-open` decide el navegador por defecto del sistema.
        std::process::Command::new("xdg-open")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| ErrorUi::interno(format!("no se pudo abrir el navegador: {error}")))?;

        Ok(RespuestaOk::si())
    })
    .await
}

#[tauri::command]
pub async fn actualizar_gitea(
    estado: State<'_, EstadoApp>,
) -> Result<RespuestaActualizarGitea, ErrorUi> {
    tracing::debug!(comando = "actualizar_gitea");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        instancia::asegurar_binario(&rutas)
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(RespuestaActualizarGitea {
            ok: true,
            version: instancia::VERSION_GITEA.to_string(),
        })
    })
    .await
}

/// Token de administración de Gitea de `login`, guardado en el llavero durante la
/// provisión. Un `None` aquí sería un almacén inconsistente (una cuenta dada
/// de alta siempre tiene uno): se trata como error interno, no como un código del
/// contrato.
/// Usuario y contraseña del administrador del Gitea local de la cuenta, para que su dueño
/// pueda entrar en la interfaz web. Es la única respuesta del contrato que lleva un
/// secreto: sale del llavero del propio usuario, no se registra y cada consulta se audita.
#[tauri::command]
pub async fn credenciales_gitea(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<CredencialesGiteaDto, ErrorUi> {
    tracing::debug!(comando = "credenciales_gitea");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _) = recursos.cuenta(login.as_str())?;
        let password = recursos
            .llavero
            .leer(&cuenta.login, ClaveSecreto::PasswordAdminGitea)
            .map_err(|error| ErrorUi::de(&error))?
            .ok_or_else(|| {
                ErrorUi::interno("no hay contraseña de administración de Gitea para esta cuenta")
            })?;
        recursos
            .almacen
            .auditar(
                Some(&cuenta.login),
                "gitea.credenciales",
                "consulta de las credenciales de administración desde la ventana",
            )
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(CredencialesGiteaDto {
            usuario: NOMBRE_ADMIN_GITEA.to_string(),
            password: password.exponer().to_string(),
        })
    })
    .await
}

fn token_gitea_de(recursos: &Recursos, login: &Nombre) -> Result<Secreto, ErrorUi> {
    recursos
        .llavero
        .leer(login, ClaveSecreto::TokenGitea)
        .map_err(|error| ErrorUi::de(&error))?
        .ok_or_else(|| ErrorUi::interno("no hay token de administración de Gitea para esta cuenta"))
}

fn token_github_de(recursos: &Recursos, login: &Nombre) -> Result<Secreto, ErrorUi> {
    recursos
        .llavero
        .leer(login, ClaveSecreto::TokenGithub)
        .map_err(|error| ErrorUi::de(&error))?
        .ok_or_else(|| ErrorUi::interno("no hay token de GitHub para esta cuenta"))
}

/// `String` de la interfaz → `Nombre` validado, con el código de error del contrato
/// para datos de entrada inválidos (no hay un `codigo` propio en la tabla del contrato
/// para un nombre con caracteres no permitidos: se usa el genérico `datos_invalidos`,
/// que ya existe para el resto de errores de forma).
fn nombre_de(valor: &str) -> Result<Nombre, ErrorUi> {
    Nombre::nuevo(valor).map_err(|error| ErrorUi::nuevo("datos_invalidos", error.to_string()))
}

/// `"dueño/nombre"` → [`IdRepo`]. Pura y testeada aparte: la usan `crear_cuenta` para
/// los repos marcados por el usuario.
fn parsear_id_repo(valor: &str) -> Result<IdRepo, ErrorUi> {
    let (dueno, nombre) = valor.split_once('/').ok_or_else(|| {
        ErrorUi::nuevo(
            "datos_invalidos",
            format!("«{valor}» no tiene la forma «dueño/nombre»"),
        )
    })?;
    Ok(IdRepo {
        dueno: nombre_de(dueno)?,
        nombre: nombre_de(nombre)?,
    })
}

/// Repos de `universo` que no están entre los `marcados`: lo que `crear_cuenta` registra
/// como `Alcance::excluidos` (contrato con la interfaz, comando `crear_cuenta`: «el resto queda
/// registrado como excluido»). Pura y testeada aparte.
fn calcular_excluidos(universo: &[RepoOrigen], marcados: &[IdRepo]) -> Vec<IdRepo> {
    universo
        .iter()
        .map(|repo| repo.id.clone())
        .filter(|id| !marcados.contains(id))
        .collect()
}

/// `ErrorUi::de`, salvo que la variante más interna sea «RepoNoExiste»: varios módulos
/// de `core` la nombran así (`cuentas::ErrorCuentas::RepoNoExiste`,
/// `contingencia::ErrorContingencia::RepoNoExiste`) y el contrato fija en su lugar el
/// código estable `repo_no_encontrado` (ver `ErrorUi::repo_no_encontrado`).
fn error_ui_repo<E: std::fmt::Debug + std::fmt::Display>(error: &E, id: &IdRepo) -> ErrorUi {
    let generico = ErrorUi::de(error);
    if generico.codigo == "repo_no_existe" {
        ErrorUi::repo_no_encontrado(id.dueno.as_str(), id.nombre.as_str())
    } else {
        generico
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    fn repo_origen(dueno: &str, nombre_repo: &str) -> RepoOrigen {
        RepoOrigen {
            id: id(dueno, nombre_repo),
            url_clon: format!("https://github.com/{dueno}/{nombre_repo}.git"),
            privado: false,
            es_fork: false,
            archivado: false,
            rama_por_defecto: Some("main".to_string()),
            descripcion: None,
            tamano_kb: 1,
        }
    }

    #[test]
    fn parsear_id_repo_acepta_dueno_barra_nombre() {
        let parseado = parsear_id_repo("jparga/gitmereba").expect("parsea");
        assert_eq!(parseado, id("jparga", "gitmereba"));
    }

    #[test]
    fn parsear_id_repo_rechaza_sin_barra() {
        let resultado = parsear_id_repo("gitmereba");
        assert!(matches!(resultado, Err(error) if error.codigo == "datos_invalidos"));
    }

    #[test]
    fn parsear_id_repo_rechaza_un_nombre_invalido() {
        let resultado = parsear_id_repo("jparga/../etc");
        assert!(matches!(resultado, Err(error) if error.codigo == "datos_invalidos"));
    }

    #[test]
    fn calcular_excluidos_deja_fuera_solo_lo_no_marcado() {
        let universo = vec![
            repo_origen("jparga", "uno"),
            repo_origen("jparga", "dos"),
            repo_origen("jparga", "tres"),
        ];
        let marcados = vec![id("jparga", "dos")];

        let excluidos = calcular_excluidos(&universo, &marcados);

        assert_eq!(excluidos, vec![id("jparga", "uno"), id("jparga", "tres")]);
    }

    #[test]
    fn calcular_excluidos_vacio_si_todo_esta_marcado() {
        let universo = vec![repo_origen("jparga", "uno")];
        let marcados = vec![id("jparga", "uno")];

        assert!(calcular_excluidos(&universo, &marcados).is_empty());
    }

    #[test]
    fn error_ui_repo_traduce_repo_no_existe_al_codigo_del_contrato() {
        // El código lo deriva `ErrorUi::de` del `Debug` de la variante, no del
        // `Display`: cualquier error cuya variante más interna se llame «RepoNoExiste»
        // (`cuentas::ErrorCuentas`, `contingencia::ErrorContingencia`, ...) cae aquí.
        #[derive(Debug)]
        #[allow(dead_code)]
        enum ErrorFalso {
            RepoNoExiste(String),
        }
        impl std::fmt::Display for ErrorFalso {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "no existe")
            }
        }

        let error = ErrorFalso::RepoNoExiste("jparga/x".to_string());
        let repo = id("jparga", "x");

        let traducido = error_ui_repo(&error, &repo);

        assert_eq!(traducido.codigo, "repo_no_encontrado");
    }

    #[test]
    fn error_ui_repo_no_toca_otros_codigos() {
        #[derive(Debug)]
        struct ErrorFalso;
        impl std::fmt::Display for ErrorFalso {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "token inválido")
            }
        }

        let traducido = error_ui_repo(&ErrorFalso, &id("jparga", "x"));

        assert_eq!(traducido.codigo, "error_falso");
    }
}
