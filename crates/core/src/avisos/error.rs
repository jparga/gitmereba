//! Error del módulo `avisos`.

use thiserror::Error;

/// Fallos al persistir el estado de avisos o al enviar una notificación de escritorio.
/// Nunca lleva secretos: solo mensajes de E/S o del sistema de notificaciones.
#[derive(Debug, Error)]
pub enum ErrorAvisos {
    #[error("error de E/S: {0}")]
    Io(String),
    #[error("no se pudo enviar la notificación de escritorio: {0}")]
    Notificador(String),
}
