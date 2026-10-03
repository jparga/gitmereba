//! Errores del llavero, en inglés.

use crate::secretos::ErrorLlavero;

pub(crate) fn texto(e: &ErrorLlavero) -> String {
    match e {
        ErrorLlavero::Backend(detalle) => {
            format!("could not access the system keyring: {detalle}")
        }
        ErrorLlavero::NoDisponible(detalle) => {
            format!("no system keyring is available: {detalle}")
        }
        ErrorLlavero::Bloqueado(detalle) => format!("the system keyring is locked: {detalle}"),
    }
}
