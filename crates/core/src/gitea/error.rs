//! Errores del cliente de Gitea.

use thiserror::Error;

/// Fallos al hablar con la API de Gitea.
///
/// Ningún mensaje incluye tokens ni credenciales: ver [`super::cliente`] para el saneado
/// del campo `detalle`.
#[derive(Debug, Error)]
pub enum ErrorGitea {
    /// La URL de Gitea no es `http(s)://127.0.0.1`, `http(s)://localhost` o
    /// `http(s)://[::1]` con el esquema esperado (`http` para [`super::ClienteGitea::nuevo`],
    /// `https` para [`super::ClienteGitea::nuevo_con_certificado`]), o lleva
    /// credenciales o ruta. La app siempre le habla a Gitea por loopback, aunque la
    /// cuenta esté expuesta a la LAN.
    #[error(
        "la URL de Gitea debe ser local (127.0.0.1, localhost o [::1]), sin usuario/clave ni ruta"
    )]
    UrlNoLocal,
    /// Conexión rechazada o tiempo agotado: Gitea no está levantado o no responde.
    #[error("Gitea no está disponible (conexión rechazada o tiempo agotado)")]
    NoDisponible,
    /// 401: el token no es válido o ha caducado.
    #[error("el token de Gitea no es válido")]
    TokenInvalido,
    /// 403: el token es válido pero no tiene permiso para la operación.
    #[error("sin permiso para esta operación en Gitea")]
    SinPermiso,
    /// 404 en una operación que no lo trata como caso normal.
    #[error("recurso no encontrado en Gitea")]
    NoEncontrado,
    /// 409 (o 422 de «ya existe» tras una carrera): el recurso ya existe.
    #[error("el recurso ya existe en Gitea")]
    YaExiste,
    /// El intervalo de mirror no tiene la forma `0` o `<n>m`/`<n>h` (mínimo 10 minutos).
    #[error("intervalo de mirror no válido: «{0}» (usa «0» o «<n>m»/«<n>h», mínimo 10 minutos)")]
    IntervaloInvalido(String),
    /// Cualquier otra respuesta no esperada. `detalle` está recortado a 500 caracteres y
    /// saneado: sin credenciales de URL ni tokens de esta llamada.
    #[error("respuesta inesperada de Gitea ({estado}): {detalle}")]
    RespuestaInesperada { estado: u16, detalle: String },
    /// La respuesta de Gitea no se pudo interpretar (JSON inesperado, nombre inválido, etc.).
    #[error("datos inválidos recibidos de Gitea: {0}")]
    DatosInvalidos(String),
}
