//! Error del cálculo de espacio en disco, en inglés.

use crate::verificacion::ErrorEspacio;

pub(crate) fn texto(e: &ErrorEspacio) -> String {
    match e {
        ErrorEspacio::Io(detalle) => format!("I/O error computing disk space: {detalle}"),
    }
}
