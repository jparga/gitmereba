//! Envío de notificaciones de escritorio.
//!
//! # `notify-rust` con el backend `zbus` dentro de un runtime `tokio`
//!
//! `crates/core/Cargo.toml` activa `notify-rust` con `default-features = false, features
//! = ["z"]`. La feature `z` = `zbus` + `serde` + `async`, y `async` = `zbus/async-io`: es
//! decir, el backend `zbus` de esta app usa el executor `async-io`/`async-executor`, **no
//! el de `tokio`**.
//!
//! [`Notification::show()`] (comprobado leyendo
//! `~/.cargo/registry/src/*/notify-rust-4.18.0/src/xdg/mod.rs`, función
//! `show_notification`) hace, en este backend, `zbus::block_on(zbus_rs::connect_and_send_notification(..))`.
//! `zbus::block_on` arranca su propio executor (`async-io`), independiente del de
//! `tokio`: por eso llamarlo desde dentro de un runtime de `tokio` **no entra en
//! pánico** (a diferencia de anidar `tokio::runtime::Handle::block_on` dentro de otro
//! runtime `tokio`, que sí lo hace). Pero sigue siendo una llamada **síncrona y
//! bloqueante**: mientras dura la conversación por D-Bus (normalmente unos pocos
//! milisegundos, pero sin límite si el bus no responde) ocupa el hilo que la invoca sin
//! cederlo al *scheduler* de `tokio`.
//!
//! Por eso [`Notificador::enviar`] es una función **síncrona** a propósito (no
//! `async fn`): quien la llame desde código async (`app/src/comandos/sync.rs`) debe
//! envolverla en `tokio::task::spawn_blocking`, igual que con cualquier otra E/S
//! bloqueante, para no retener un hilo del *runtime* multi-hilo de `tokio` mientras se
//! espera al bus de sesión.
//!
//! [`Notification::show()`]: https://docs.rs/notify-rust/4.18.0/notify_rust/struct.Notification.html#method.show

use std::sync::Mutex;

use notify_rust::{Notification, Timeout, Urgency as UrgenciaNotifyRust};

use super::error::ErrorAvisos;
use super::modelo::{NotificacionLocalizada, Urgencia};

/// Nombre de la app para todas las notificaciones de gitmereba.
const NOMBRE_APP: &str = "gitmereba";

/// Envía una [`NotificacionLocalizada`] (una [`super::Notificacion`] ya decidida por
/// [`super::decidir`] y traducida con [`super::Notificacion::localizar`]).
pub trait Notificador {
    /// Envía `n`. Un error aquí (p. ej. no hay servidor de notificaciones D-Bus en la
    /// sesión) no es fatal para quien llama: gitmereba sigue funcionando sin avisos de
    /// escritorio, solo se registra (`tracing::warn!`) y se continúa.
    fn enviar(&self, n: &NotificacionLocalizada) -> Result<(), ErrorAvisos>;
}

/// Envía notificaciones reales al escritorio vía D-Bus
/// (`org.freedesktop.Notifications`), con appname `gitmereba` e icono genérico según la
/// urgencia (nombres del espectro de iconos freedesktop, no específicos de ningún tema).
pub struct NotificadorEscritorio;

impl Notificador for NotificadorEscritorio {
    fn enviar(&self, n: &NotificacionLocalizada) -> Result<(), ErrorAvisos> {
        let urgencia = match n.urgencia {
            Urgencia::Baja => UrgenciaNotifyRust::Low,
            Urgencia::Normal => UrgenciaNotifyRust::Normal,
            Urgencia::Critica => UrgenciaNotifyRust::Critical,
        };
        // dialog-information/-warning/-error: iconos genéricos estándar de la
        // especificación freedesktop de iconos, presentes en cualquier tema habitual.
        let icono = match n.urgencia {
            Urgencia::Baja => "dialog-information",
            Urgencia::Normal => "dialog-warning",
            Urgencia::Critica => "dialog-error",
        };

        Notification::new()
            .appname(NOMBRE_APP)
            .summary(&n.titulo)
            .body(&n.cuerpo)
            .icon(icono)
            .urgency(urgencia)
            .timeout(Timeout::Default)
            .show()
            .map(|_| ())
            .map_err(|error| ErrorAvisos::Notificador(error.to_string()))
    }
}

/// Doble en memoria de [`Notificador`], para pruebas: no toca D-Bus ni el escritorio real.
/// No está tras `#[cfg(test)]` a propósito: así también lo pueden usar las pruebas de
/// `app` (que no ven el `cfg(test)` de esta crate), igual que
/// [`crate::secretos::LlaveroEnMemoria`].
#[derive(Default)]
pub struct NotificadorMemoria {
    enviadas: Mutex<Vec<NotificacionLocalizada>>,
}

impl NotificadorMemoria {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Copia de todo lo enviado hasta ahora, en el orden en que se envió.
    pub fn enviadas(&self) -> Result<Vec<NotificacionLocalizada>, ErrorAvisos> {
        self.enviadas
            .lock()
            .map(|guardia| guardia.clone())
            .map_err(|_| {
                ErrorAvisos::Notificador("mutex de NotificadorMemoria envenenado".to_string())
            })
    }
}

impl Notificador for NotificadorMemoria {
    fn enviar(&self, n: &NotificacionLocalizada) -> Result<(), ErrorAvisos> {
        let mut guardia = self.enviadas.lock().map_err(|_| {
            ErrorAvisos::Notificador("mutex de NotificadorMemoria envenenado".to_string())
        })?;
        guardia.push(n.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notificacion() -> NotificacionLocalizada {
        NotificacionLocalizada {
            titulo: "título".to_string(),
            cuerpo: "cuerpo".to_string(),
            urgencia: Urgencia::Normal,
            clave_dedupe: "clave".to_string(),
        }
    }

    #[test]
    fn el_doble_en_memoria_registra_lo_enviado_en_orden() {
        let notificador = NotificadorMemoria::nuevo();
        let mut primera = notificacion();
        primera.titulo = "primera".to_string();
        let mut segunda = notificacion();
        segunda.titulo = "segunda".to_string();

        notificador.enviar(&primera).expect("enviar no falla");
        notificador.enviar(&segunda).expect("enviar no falla");

        let enviadas = notificador.enviadas().expect("leer enviadas");
        assert_eq!(enviadas, vec![primera, segunda]);
    }

    #[test]
    fn el_doble_en_memoria_empieza_vacio() {
        let notificador = NotificadorMemoria::nuevo();
        assert!(notificador.enviadas().expect("leer enviadas").is_empty());
    }

    /// Envía una notificación real al escritorio: hace falta un servidor de
    /// notificaciones D-Bus en la sesión (`org.freedesktop.Notifications`). Se ejecuta a
    /// mano con `cargo nextest run -p gitmereba-core --run-ignored ignored-only avisos`
    /// (o `cargo test ... -- --ignored`), nunca en CI ni en la batería normal.
    #[test]
    #[ignore = "envía una notificación real"]
    fn envia_una_notificacion_real_al_escritorio() {
        let notificacion = NotificacionLocalizada {
            titulo: "gitmereba: prueba de notificación".to_string(),
            cuerpo: "Si ves esto, NotificadorEscritorio funciona.".to_string(),
            urgencia: Urgencia::Normal,
            clave_dedupe: "prueba-notificacion".to_string(),
        };
        NotificadorEscritorio
            .enviar(&notificacion)
            .expect("el envío real no falla");
    }
}
