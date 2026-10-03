//! Errores del cliente de Gitea, en inglés.

use crate::gitea::ErrorGitea;

pub(crate) fn texto(e: &ErrorGitea) -> String {
    match e {
        ErrorGitea::UrlNoLocal => "the Gitea URL must be local (127.0.0.1, localhost or [::1]), \
                                   with no user/password or path"
            .to_string(),
        ErrorGitea::NoDisponible => {
            "Gitea is not available (connection refused or timed out)".to_string()
        }
        ErrorGitea::TokenInvalido => "the Gitea token is invalid".to_string(),
        ErrorGitea::SinPermiso => "no permission for this operation on Gitea".to_string(),
        ErrorGitea::NoEncontrado => "resource not found on Gitea".to_string(),
        ErrorGitea::YaExiste => "the resource already exists on Gitea".to_string(),
        ErrorGitea::IntervaloInvalido(valor) => format!(
            "invalid mirror interval: “{valor}” (use “0” or “<n>m”/“<n>h”, minimum 10 minutes)"
        ),
        ErrorGitea::RespuestaInesperada { estado, detalle } => {
            format!("unexpected response from Gitea ({estado}): {detalle}")
        }
        ErrorGitea::DatosInvalidos(detalle) => {
            format!("invalid data received from Gitea: {detalle}")
        }
    }
}
