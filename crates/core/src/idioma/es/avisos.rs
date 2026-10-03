//! Textos de las notificaciones, en español.

use crate::avisos::{CambioAviso, TextoAviso};

pub(crate) fn texto(t: &TextoAviso) -> String {
    match t {
        TextoAviso::TituloFalloSincronizacion => "Fallo de sincronización".to_string(),
        TextoAviso::TituloVariosFallos => "Varios repositorios con fallos".to_string(),
        TextoAviso::TituloRepositorioRecuperado => "Repositorio recuperado".to_string(),
        TextoAviso::TituloVariosRecuperados => "Varios repositorios recuperados".to_string(),
        TextoAviso::TituloHuerfanos => "Demasiados huérfanos".to_string(),
        TextoAviso::TituloHistoriaReescrita => "Historia reescrita".to_string(),
        TextoAviso::TituloGiteaParado => "Gitea no responde".to_string(),
        TextoAviso::TituloTokenCaducado => "Token de GitHub caducado".to_string(),
        TextoAviso::TituloTokenCaducaPronto => "Token de GitHub a punto de caducar".to_string(),
        TextoAviso::FalloRepositorio {
            login,
            repo,
            detalle,
        } => {
            let mut cuerpo = format!("El repositorio «{repo}» tiene un fallo en «{login}».");
            if let Some(detalle) = detalle {
                cuerpo.push_str(&format!(" Detalle: {}.", detalle.es));
            }
            cuerpo
        }
        TextoAviso::FallosAgregados { login, n } => {
            format!("{n} repositorios con fallos en «{login}».")
        }
        TextoAviso::RepositorioRecuperado { login, repo } => {
            format!("El repositorio «{repo}» ha vuelto a estar bien en «{login}».")
        }
        TextoAviso::RecuperadosAgregados { login, n } => {
            format!("{n} repositorios han vuelto a estar bien en «{login}».")
        }
        TextoAviso::HuerfanosListaVacia { login } => format!(
            "GitHub devolvió una lista vacía de repositorios en «{login}»; no se ha \
             marcado ningún huérfano por precaución."
        ),
        TextoAviso::HuerfanosCandidatos {
            login,
            candidatos,
            total_mirrors,
        } => format!(
            "{candidatos} de {total_mirrors} repositorios se marcarían huérfanos en \
             «{login}»; no se ha aplicado por precaución."
        ),
        TextoAviso::CambioDestructivo {
            login,
            repo,
            cambios,
        } => {
            let descripcion = cambios.iter().map(cambio).collect::<Vec<_>>().join("; ");
            format!(
                "{descripcion} en «{repo}» ({login}). La copia anterior está protegida \
                 en los snapshots."
            )
        }
        TextoAviso::GiteaParado { login } => {
            format!("El Gitea de «{login}» no responde; revisa «gitmereba doctor».")
        }
        TextoAviso::TokenCaducado { login } => {
            format!("El token de GitHub de «{login}» ha caducado; genera uno nuevo.")
        }
        TextoAviso::TokenCaducaPronto { login, dias } => {
            format!("El token de GitHub de «{login}» caduca en {dias} día(s).")
        }
    }
}

fn cambio(c: &CambioAviso) -> String {
    match c {
        CambioAviso::HistoriaReescrita { rama } => format!("Historia reescrita (rama {rama})"),
        CambioAviso::RamaBorrada { rama } => format!("Rama {rama} borrada"),
        CambioAviso::TagBorrado { tag } => format!("Tag {tag} borrado"),
        CambioAviso::TagMovido { tag } => format!("Tag {tag} movido"),
    }
}
