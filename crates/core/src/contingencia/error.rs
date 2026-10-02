//! Error del módulo `contingencia`.
//!
//! Ningún mensaje ni campo contiene secretos: los errores de `git` y `gitea` que se
//! envuelven aquí ya vienen saneados (ver `git::proceso` y `gitea::cliente`), y este
//! módulo no añade ningún dato nuevo que pudiera llevarlos. `reconciliar` además sanea a
//! mano cualquier aparición literal del token de escritura en los mensajes que construye
//! él mismo (ver `super::reconciliar`).

use crate::git::ErrorGit;
use crate::gitea::ErrorGitea;
use crate::modelo::{ErrorNombre, IdRepo};

/// Paso en el que falló [`super::activar`], para poder decir dónde quedó a medias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasoActivar {
    PausarMirror,
    AsegurarOrganizacion,
    CrearRepo,
    Empujar,
}

impl std::fmt::Display for PasoActivar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let texto = match self {
            PasoActivar::PausarMirror => "pausar el mirror",
            PasoActivar::AsegurarOrganizacion => "asegurar la organización de contingencia",
            PasoActivar::CrearRepo => "crear el repo hermano",
            PasoActivar::Empujar => "empujar el contenido al repo hermano",
        };
        f.write_str(texto)
    }
}

/// Fallos de `contingencia`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorContingencia {
    #[error("nombre de organización de contingencia inválido: {0}")]
    NombreInvalido(#[from] ErrorNombre),
    #[error("«{0}» no existe en Gitea")]
    RepoNoExiste(IdRepo),
    #[error("«{0}» no es un mirror")]
    NoEsMirror(IdRepo),
    #[error("no se pudo localizar el bare de «{0}» dentro de repositories/")]
    RutaBareInvalida(IdRepo),
    /// Un paso de `activar` falló tras crear (o antes de crear) el repo hermano: no se
    /// borra nada; repetir `activar` con los mismos datos debe completarlo.
    #[error("activar la contingencia de «{repo}» falló al {paso}: {origen}")]
    FalloAlActivar {
        repo: IdRepo,
        paso: PasoActivar,
        origen: String,
    },
    #[error("el origen de reconciliación debe ser una URL https de github.com")]
    OrigenNoGithub,
    #[error("la contingencia de «{0}» no está activada: no hay repo hermano en disco")]
    NoActivada(IdRepo),
    #[error("no se puede cerrar «{0}»: la reconciliación no está completa")]
    ReconciliacionIncompleta(IdRepo),
    #[error("no se puede borrar la contingencia de «{0}»: hay commits sin reconciliar")]
    HayTrabajoSinReconciliar(IdRepo),
    #[error("confirmación incorrecta: se esperaba «{esperado}»")]
    ConfirmacionIncorrecta { esperado: String },
    #[error("git: {0}")]
    Git(#[from] ErrorGit),
    #[error("Gitea: {0}")]
    Gitea(#[from] ErrorGitea),
    #[error("error de E/S: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paso_activar_se_muestra_en_espanol() {
        assert_eq!(PasoActivar::PausarMirror.to_string(), "pausar el mirror");
        assert_eq!(
            PasoActivar::Empujar.to_string(),
            "empujar el contenido al repo hermano"
        );
    }
}
