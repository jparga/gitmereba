//! Errores del módulo de avisos, en inglés.

use crate::avisos::ErrorAvisos;

pub(crate) fn texto(e: &ErrorAvisos) -> String {
    match e {
        ErrorAvisos::Io(detalle) => format!("I/O error: {detalle}"),
        ErrorAvisos::Notificador(detalle) => {
            format!("could not send the desktop notification: {detalle}")
        }
    }
}
