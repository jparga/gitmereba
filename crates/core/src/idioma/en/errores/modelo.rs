//! Errores de validación de nombres y hosts, en inglés.

use crate::modelo::{ErrorHostInterno, ErrorNombre};

pub(crate) fn nombre(e: &ErrorNombre) -> String {
    match e {
        ErrorNombre::Vacio => "the name is empty".to_string(),
        ErrorNombre::DemasiadoLargo(maximo) => {
            format!("the name is longer than {maximo} characters")
        }
        ErrorNombre::CaracteresNoPermitidos => {
            "the name contains disallowed characters".to_string()
        }
        ErrorNombre::FormaNoPermitida => "the name cannot start with a hyphen, contain “..” \
                                          or be “.” or “.git”"
            .to_string(),
    }
}

pub(crate) fn host(e: &ErrorHostInterno) -> String {
    match e {
        ErrorHostInterno::Vacio => "the host name is empty".to_string(),
        ErrorHostInterno::DemasiadoLargo => {
            "the host name is longer than 253 characters".to_string()
        }
        ErrorHostInterno::NoTerminaEnInternal => {
            "the host name must end in “.internal”".to_string()
        }
        ErrorHostInterno::EtiquetaInvalida(etiqueta) => {
            format!("the host name label “{etiqueta}” is not valid")
        }
    }
}
