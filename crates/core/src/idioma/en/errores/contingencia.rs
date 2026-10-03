//! Errores de la contingencia, en inglés.

use crate::contingencia::{ErrorContingencia, PasoActivar};
use crate::idioma::{Idioma, Localizable};

fn paso(p: &PasoActivar) -> &'static str {
    match p {
        PasoActivar::PausarMirror => "pausing the mirror",
        PasoActivar::AsegurarOrganizacion => "ensuring the contingency organization",
        PasoActivar::CrearRepo => "creating the sibling repo",
        PasoActivar::Empujar => "pushing the content to the sibling repo",
    }
}

pub(crate) fn texto(e: &ErrorContingencia) -> String {
    match e {
        ErrorContingencia::NombreInvalido(interno) => format!(
            "invalid contingency organization name: {}",
            interno.localizar(Idioma::En)
        ),
        ErrorContingencia::RepoNoExiste(repo) => format!("“{repo}” does not exist on Gitea"),
        ErrorContingencia::NoEsMirror(repo) => format!("“{repo}” is not a mirror"),
        ErrorContingencia::RutaBareInvalida(repo) => {
            format!("could not locate the bare repository of “{repo}” inside repositories/")
        }
        ErrorContingencia::FalloAlActivar {
            repo,
            paso: p,
            origen,
        } => format!(
            "activating the contingency of “{repo}” failed while {}: {origen}",
            paso(p)
        ),
        ErrorContingencia::OrigenNoGithub => {
            "the reconciliation source must be an https URL on github.com".to_string()
        }
        ErrorContingencia::NoActivada(repo) => format!(
            "the contingency of “{repo}” is not activated: there is no sibling repo on disk"
        ),
        ErrorContingencia::ReconciliacionIncompleta(repo) => {
            format!("“{repo}” cannot be closed: the reconciliation is not complete")
        }
        ErrorContingencia::HayTrabajoSinReconciliar(repo) => {
            format!("the contingency of “{repo}” cannot be deleted: there are unreconciled commits")
        }
        ErrorContingencia::ConfirmacionIncorrecta { esperado } => {
            format!("incorrect confirmation: “{esperado}” was expected")
        }
        ErrorContingencia::Git(interno) => format!("git: {}", interno.localizar(Idioma::En)),
        ErrorContingencia::Gitea(interno) => format!("Gitea: {}", interno.localizar(Idioma::En)),
        ErrorContingencia::Io(detalle) => format!("I/O error: {detalle}"),
    }
}
