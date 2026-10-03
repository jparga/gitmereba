//! Textos de las notificaciones, en inglés.

use crate::avisos::{CambioAviso, TextoAviso};

pub(crate) fn texto(t: &TextoAviso) -> String {
    match t {
        TextoAviso::TituloFalloSincronizacion => "Sync failure".to_string(),
        TextoAviso::TituloVariosFallos => "Several repositories with failures".to_string(),
        TextoAviso::TituloRepositorioRecuperado => "Repository recovered".to_string(),
        TextoAviso::TituloVariosRecuperados => "Several repositories recovered".to_string(),
        TextoAviso::TituloHuerfanos => "Too many orphans".to_string(),
        TextoAviso::TituloHistoriaReescrita => "History rewritten".to_string(),
        TextoAviso::TituloGiteaParado => "Gitea is not responding".to_string(),
        TextoAviso::TituloTokenCaducado => "GitHub token expired".to_string(),
        TextoAviso::TituloTokenCaducaPronto => "GitHub token about to expire".to_string(),
        TextoAviso::FalloRepositorio {
            login,
            repo,
            detalle,
        } => {
            let mut cuerpo = format!("Repository “{repo}” has a failure in “{login}”.");
            if let Some(detalle) = detalle {
                cuerpo.push_str(&format!(" Detail: {}.", detalle.en));
            }
            cuerpo
        }
        TextoAviso::FallosAgregados { login, n } => {
            format!("{n} repositories with failures in “{login}”.")
        }
        TextoAviso::RepositorioRecuperado { login, repo } => {
            format!("Repository “{repo}” is back to normal in “{login}”.")
        }
        TextoAviso::RecuperadosAgregados { login, n } => {
            format!("{n} repositories are back to normal in “{login}”.")
        }
        TextoAviso::HuerfanosListaVacia { login } => format!(
            "GitHub returned an empty list of repositories in “{login}”; no orphan \
             has been marked, as a precaution."
        ),
        TextoAviso::HuerfanosCandidatos {
            login,
            candidatos,
            total_mirrors,
        } => format!(
            "{candidatos} of {total_mirrors} repositories would be marked as orphans in \
             “{login}”; this has not been applied, as a precaution."
        ),
        TextoAviso::CambioDestructivo {
            login,
            repo,
            cambios,
        } => {
            let descripcion = cambios.iter().map(cambio).collect::<Vec<_>>().join("; ");
            format!(
                "{descripcion} in “{repo}” ({login}). The previous copy is protected \
                 in the snapshots."
            )
        }
        TextoAviso::GiteaParado { login } => {
            format!("The Gitea of “{login}” is not responding; check “gitmereba doctor”.")
        }
        TextoAviso::TokenCaducado { login } => {
            format!("The GitHub token of “{login}” has expired; generate a new one.")
        }
        TextoAviso::TokenCaducaPronto { login, dias } => {
            format!("The GitHub token of “{login}” expires in {dias} day(s).")
        }
    }
}

fn cambio(c: &CambioAviso) -> String {
    match c {
        CambioAviso::HistoriaReescrita { rama } => format!("History rewritten (branch {rama})"),
        CambioAviso::RamaBorrada { rama } => format!("Branch {rama} deleted"),
        CambioAviso::TagBorrado { tag } => format!("Tag {tag} deleted"),
        CambioAviso::TagMovido { tag } => format!("Tag {tag} moved"),
    }
}
