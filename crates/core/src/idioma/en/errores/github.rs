//! Errores del cliente de GitHub, en inglés.

use crate::github::ErrorGithub;

pub(crate) fn texto(e: &ErrorGithub) -> String {
    match e {
        ErrorGithub::TokenInvalido => "the GitHub token is invalid or has expired".to_string(),
        ErrorGithub::SinPermiso => {
            "the token does not have permission for this operation".to_string()
        }
        ErrorGithub::LimiteDePeticiones { .. } => {
            "the GitHub request limit has been reached".to_string()
        }
        ErrorGithub::NoEncontrado => "the requested resource does not exist on GitHub".to_string(),
        ErrorGithub::Red(detalle) => format!("network error talking to GitHub: {detalle}"),
        ErrorGithub::RespuestaInesperada { estado, detalle } => {
            format!("GitHub responded unexpectedly ({estado}): {detalle}")
        }
        ErrorGithub::UrlNoPermitida => "the URL is not allowed".to_string(),
        ErrorGithub::DatosInvalidos(detalle) => format!("invalid data: {detalle}"),
    }
}
