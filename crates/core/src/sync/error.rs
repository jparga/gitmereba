//! Error de la sincronización.

use crate::gitea::ErrorGitea;
use crate::github::ErrorGithub;
use crate::modelo::Nombre;

/// Fallos de [`super::ejecutar`]. Ningún mensaje contiene secretos.
#[derive(Debug, thiserror::Error)]
pub enum ErrorSync {
    /// El token identifica a una cuenta distinta de la esperada: no se hace nada más.
    #[error("el token pertenece a «{obtenido}», no a la cuenta esperada «{esperado}»")]
    TokenDeOtraCuenta { esperado: Nombre, obtenido: Nombre },
    /// Fallo irrecuperable hablando con GitHub (p. ej. al comprobar la identidad).
    #[error("GitHub: {0}")]
    Github(#[from] ErrorGithub),
    /// Fallo irrecuperable hablando con Gitea (p. ej. al asegurar una organización).
    #[error("Gitea: {0}")]
    Gitea(#[from] ErrorGitea),
}
