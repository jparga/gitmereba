//! Módulo `avisos`: decide cuándo avisar por escritorio de fallos, recuperaciones,
//! caducidad del token de GitHub o de que Gitea está parado, y los envía.
//!
//! [`decidir`] es la función pura (sin E/S): a partir del resultado de una pasada y de lo
//! que ya se sabía ([`EstadoAvisos`]) decide qué [`Notificacion`] mandar y cómo queda el
//! estado para la próxima vez. El resto del módulo es la parte con E/S: persistir
//! [`EstadoAvisos`] entre pasadas ([`cargar_estado`]/[`guardar_estado`]) y enviar las
//! notificaciones resultantes por un [`Notificador`].

mod decidir;
mod error;
mod estado;
mod modelo;
mod notificador;
mod saneado;

pub use decidir::decidir;
pub use error::ErrorAvisos;
pub use estado::{
    EntradaFallo, EstadoAvisos, cargar_estado, guardar_estado, nombre_fichero_estado, ruta_estado,
};
pub use modelo::{
    CambioAviso, EntradaAvisos, Notificacion, NotificacionLocalizada, TextoAviso, TipoFallo,
    Urgencia,
};
pub use notificador::{Notificador, NotificadorEscritorio, NotificadorMemoria};
