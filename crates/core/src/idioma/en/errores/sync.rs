//! Errores de la sincronización, en inglés.

use crate::idioma::{Idioma, Localizable};
use crate::sync::ErrorSync;

pub(crate) fn texto(e: &ErrorSync) -> String {
    match e {
        ErrorSync::TokenDeOtraCuenta { esperado, obtenido } => {
            format!("the token belongs to “{obtenido}”, not to the expected account “{esperado}”")
        }
        ErrorSync::Github(interno) => format!("GitHub: {}", interno.localizar(Idioma::En)),
        ErrorSync::Gitea(interno) => format!("Gitea: {}", interno.localizar(Idioma::En)),
    }
}
