//! `gitmereba sync`: sincroniza una cuenta o todas. Lo ejecuta el timer.

use time::OffsetDateTime;

use super::abrir_almacen;
use gitmereba_core::avisos::{self, EntradaAvisos, Notificador, NotificadorEscritorio};
use gitmereba_core::config::{self, Rutas};
use gitmereba_core::cuentas::{self, Contexto, ErrorCuentas};
use gitmereba_core::idioma::idioma_actual;
use gitmereba_core::modelo::{IdRepo, Nombre};
use gitmereba_core::secretos::LlaveroDelSistema;
use gitmereba_core::sync::OpcionesSync;
use gitmereba_core::verificacion::OpcionesVerificacion;

use crate::cli::SyncArgs;
use crate::salida;

use super::CODIGO_ERROR_DE_USO;
use super::CODIGO_FALLOS_PARCIALES;

pub async fn ejecutar(args: SyncArgs, rutas: &Rutas) -> u8 {
    let logins = match logins_a_sincronizar(&args, rutas) {
        Ok(logins) => logins,
        Err(codigo) => return codigo,
    };

    let llavero = match LlaveroDelSistema::nuevo() {
        Ok(llavero) => llavero,
        Err(error) => return fallo(&format!("no se pudo acceder al llavero: {error}")),
    };
    let almacen = match abrir_almacen(rutas) {
        Ok(almacen) => almacen,
        Err(error) => return fallo(&format!("no se pudo abrir el almacén: {error}")),
    };
    let contexto = Contexto::nuevo(rutas, &llavero, &almacen);

    let opciones = OpcionesSync {
        simulacro: args.simulacro,
        forzar_sync: args.forzar,
        ..OpcionesSync::default()
    };
    let opciones_verificacion = OpcionesVerificacion::default();

    if let Some(texto) = &args.repo
        && let Err(codigo) = forzar_repo(&contexto, &logins, texto).await
    {
        return codigo;
    }

    let mut hubo_fallos = false;
    let mut hubo_error = false;
    for login in &logins {
        let entrada_avisos = match cuentas::sincronizar_y_verificar(
            &contexto,
            login,
            &opciones,
            &opciones_verificacion,
        )
        .await
        {
            Ok(resultado) => {
                let mut stdout = std::io::stdout().lock();
                salida::linea(&mut stdout, &resultado.sync.resumen());
                if resultado.sync.hay_fallos() {
                    hubo_fallos = true;
                }
                Some(EntradaAvisos {
                    login: login.clone(),
                    sync: Some(resultado.sync),
                    verificacion: resultado.verificacion,
                    gitea_parado: false,
                    proteccion: resultado.proteccion,
                })
            }
            // Gitea parado también pasa por `avisos::decidir` (así lo exige el aviso de sincronización):
            // no ha habido ni sincronización ni verificación, pero conviene avisar.
            Err(ErrorCuentas::GiteaParado(login_parado)) => {
                emitir_error(&ErrorCuentas::GiteaParado(login_parado.clone()));
                hubo_error = true;
                Some(EntradaAvisos {
                    login: login_parado,
                    sync: None,
                    verificacion: None,
                    gitea_parado: true,
                    proteccion: None,
                })
            }
            Err(error) => {
                emitir_error(&error);
                hubo_error = true;
                None
            }
        };

        if let Some(entrada_avisos) = entrada_avisos {
            procesar_avisos(rutas, &entrada_avisos, args.sin_avisos).await;
        }
    }

    if hubo_error {
        1
    } else if hubo_fallos {
        CODIGO_FALLOS_PARCIALES
    } else {
        0
    }
}

/// Decide los avisos de `entrada` (`avisos::decidir`) y los envía, cargando y guardando
/// el estado persistido de la cuenta entre pasadas.
///
/// Un fallo aquí (leer/guardar el estado, o enviar una notificación) se registra con
/// `tracing::warn!` y nunca cambia el código de salida de `sync`: los avisos son un
/// extra, no el resultado principal del comando. El estado se guarda siempre, incluso
/// con `--sin-avisos`: así, si se quita el flag más adelante, el dedupe/reaviso sigue
/// siendo coherente con lo que ya se sabía.
async fn procesar_avisos(rutas: &Rutas, entrada: &EntradaAvisos, sin_avisos: bool) {
    let ruta_estado = avisos::ruta_estado(rutas.directorio_datos(), &entrada.login);
    let previo = avisos::cargar_estado(&ruta_estado);
    let (notificaciones, nuevo_estado) =
        avisos::decidir(entrada, &previo, OffsetDateTime::now_utc());

    if let Err(error) = avisos::guardar_estado(&ruta_estado, &nuevo_estado) {
        tracing::warn!(
            cuenta = %entrada.login,
            error = %error,
            "no se pudo guardar el estado de avisos"
        );
    }

    if sin_avisos {
        return;
    }

    let idioma = idioma_actual(rutas);
    for notificacion in notificaciones {
        let notificacion = notificacion.localizar(idioma);
        // `Notificador::enviar` es síncrona y bloqueante (D-Bus vía `notify-rust`/`zbus`,
        // ver `gitmereba_core::avisos`): se manda a un hilo aparte para no bloquear el
        // runtime de `tokio` mientras dura la conversación con el bus de sesión.
        let resultado =
            tokio::task::spawn_blocking(move || NotificadorEscritorio.enviar(&notificacion)).await;

        match resultado {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(
                    cuenta = %entrada.login,
                    error = %error,
                    "no se pudo enviar una notificación de escritorio"
                );
            }
            Err(error) => {
                tracing::warn!(
                    cuenta = %entrada.login,
                    error = %error,
                    "la tarea de enviar la notificación de escritorio no ha terminado bien"
                );
            }
        }
    }
}

/// `--repo dueño/nombre`: sincroniza ese mirror o, si nunca llegó a clonarse, lo descarta
/// para que la pasada que viene a continuación lo cree de nuevo.
async fn forzar_repo(
    contexto: &Contexto<'_, LlaveroDelSistema>,
    logins: &[Nombre],
    texto: &str,
) -> Result<(), u8> {
    let id = texto
        .split_once('/')
        .and_then(|(dueno, nombre)| {
            Some(IdRepo {
                dueno: Nombre::nuevo(dueno).ok()?,
                nombre: Nombre::nuevo(nombre).ok()?,
            })
        })
        .ok_or_else(|| {
            fallo("--repo debe tener la forma dueño/nombre");
            CODIGO_ERROR_DE_USO
        })?;
    let Some(login) = logins.first() else {
        return Ok(());
    };
    let rutas_cuenta = config::leer_indice_cuentas(contexto.rutas)
        .ok()
        .and_then(|indice| indice.cuentas.get(login.as_str()).cloned())
        .map(|entrada| config::RutasCuenta::nueva(entrada.carpeta))
        .ok_or_else(|| fallo(&format!("no existe la cuenta «{login}»")))?;
    let cuenta = config::leer_cuenta(&rutas_cuenta)
        .map_err(|error| fallo(&format!("no se pudo leer la cuenta: {error}")))?;

    match cuentas::sincronizar_repo(contexto, &cuenta, &id).await {
        Ok(cuentas::SincronizacionDeRepo::Sincronizado) => {
            salida::linea(
                &mut std::io::stdout().lock(),
                &format!("«{id}»: sincronización pedida."),
            );
            Ok(())
        }
        Ok(cuentas::SincronizacionDeRepo::PendienteDeReclonar) => {
            salida::linea(
                &mut std::io::stdout().lock(),
                &format!("«{id}»: el clonado inicial había fallado; se vuelve a clonar."),
            );
            Ok(())
        }
        Err(error) => {
            emitir_error(&error);
            Err(1)
        }
    }
}

fn logins_a_sincronizar(args: &SyncArgs, rutas: &Rutas) -> Result<Vec<Nombre>, u8> {
    match (&args.login, args.todas) {
        (Some(_), true) => {
            fallo("indica un login o --todas, no las dos cosas");
            Err(CODIGO_ERROR_DE_USO)
        }
        (None, false) => {
            fallo("indica un login o usa --todas");
            Err(CODIGO_ERROR_DE_USO)
        }
        (Some(login), false) => match Nombre::nuevo(login.as_str()) {
            Ok(nombre) => Ok(vec![nombre]),
            Err(error) => {
                fallo(&format!("login inválido: {error}"));
                Err(CODIGO_ERROR_DE_USO)
            }
        },
        (None, true) => match config::leer_indice_cuentas(rutas) {
            Ok(indice) => Ok(indice
                .cuentas
                .keys()
                .filter_map(|login| Nombre::nuevo(login.as_str()).ok())
                .collect()),
            Err(error) => {
                fallo(&format!("no se pudo leer el índice de cuentas: {error}"));
                Err(1)
            }
        },
    }
}

fn fallo(mensaje: &str) -> u8 {
    let mut stderr = std::io::stderr().lock();
    salida::linea(&mut stderr, &format!("error: {mensaje}"));
    1
}

fn emitir_error(error: &ErrorCuentas) {
    let mut stderr = std::io::stderr().lock();
    salida::error(&mut stderr, error);
}
