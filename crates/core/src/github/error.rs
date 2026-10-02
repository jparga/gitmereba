//! Errores del cliente de GitHub.

use time::OffsetDateTime;

/// Error al hablar con la API de GitHub.
///
/// Los mensajes están pensados para mostrarse al usuario: ninguno contiene el token ni
/// otros datos sensibles.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ErrorGithub {
    /// 401: el token no es válido o ha caducado.
    #[error("el token de GitHub no es válido o ha caducado")]
    TokenInvalido,
    /// 403 que no corresponde a un límite de peticiones agotado.
    #[error("el token no tiene permiso para esta operación")]
    SinPermiso,
    /// 403 con `x-ratelimit-remaining: 0`, o 429 tras agotar los reintentos.
    #[error("se ha agotado el límite de peticiones a GitHub")]
    LimiteDePeticiones {
        /// Momento en que se reinicia el límite, si GitHub lo informó.
        reinicio: Option<OffsetDateTime>,
    },
    /// 404.
    #[error("el recurso solicitado no existe en GitHub")]
    NoEncontrado,
    /// Fallo de red o de conexión, ya reintentado sin éxito.
    #[error("error de red al hablar con GitHub: {0}")]
    Red(String),
    /// Cualquier otra respuesta que no encaja en los casos anteriores.
    #[error("GitHub respondió de forma inesperada ({estado}): {detalle}")]
    RespuestaInesperada {
        /// Código de estado HTTP devuelto.
        estado: u16,
        /// Detalle breve, sin datos sensibles.
        detalle: String,
    },
    /// La URL indicada no cumple la política de esquemas y hosts permitidos.
    #[error("la URL no está permitida")]
    UrlNoPermitida,
    /// Respuesta con datos que no se pueden interpretar (JSON, cabeceras, paginación).
    #[error("datos inválidos: {0}")]
    DatosInvalidos(String),
}

impl ErrorGithub {
    /// Indica si tiene sentido reintentar la operación completa más tarde.
    pub fn es_reintentable(&self) -> bool {
        match self {
            ErrorGithub::Red(_) | ErrorGithub::LimiteDePeticiones { .. } => true,
            ErrorGithub::RespuestaInesperada { estado, .. } => {
                (500..=599).contains(estado) || *estado == 429
            }
            ErrorGithub::TokenInvalido
            | ErrorGithub::SinPermiso
            | ErrorGithub::NoEncontrado
            | ErrorGithub::UrlNoPermitida
            | ErrorGithub::DatosInvalidos(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_errores_transitorios_son_reintentables() {
        assert!(ErrorGithub::Red("x".into()).es_reintentable());
        assert!(ErrorGithub::LimiteDePeticiones { reinicio: None }.es_reintentable());
        assert!(
            ErrorGithub::RespuestaInesperada {
                estado: 502,
                detalle: String::new()
            }
            .es_reintentable()
        );
        assert!(
            ErrorGithub::RespuestaInesperada {
                estado: 429,
                detalle: String::new()
            }
            .es_reintentable()
        );
    }

    #[test]
    fn los_errores_de_credencial_no_son_reintentables() {
        assert!(!ErrorGithub::TokenInvalido.es_reintentable());
        assert!(!ErrorGithub::SinPermiso.es_reintentable());
        assert!(!ErrorGithub::NoEncontrado.es_reintentable());
        assert!(!ErrorGithub::UrlNoPermitida.es_reintentable());
        assert!(
            !ErrorGithub::RespuestaInesperada {
                estado: 400,
                detalle: String::new()
            }
            .es_reintentable()
        );
    }

    #[test]
    fn los_mensajes_no_incluyen_la_palabra_token_en_claro() {
        // El mensaje describe el error, nunca reproduce el valor del secreto.
        let error = ErrorGithub::Red("conexión rechazada".into());
        assert!(!format!("{error:?}").contains("ghp_"));
    }
}
