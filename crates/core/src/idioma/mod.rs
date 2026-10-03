//! Idioma de la interfaz: resolución a partir de la preferencia y del entorno.

mod en;
mod es;
mod preferencias;

use crate::avisos::TextoAviso;
use crate::config::Rutas;

pub use preferencias::{LecturaPreferencia, Preferencia, guardar_preferencia, leer_preferencia};

/// Idioma en el que se muestran los textos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idioma {
    Es,
    En,
}

/// De dónde sale el idioma resuelto (lo muestra `doctor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrigenIdioma {
    /// La preferencia guardada es explícita (`es` o `en`).
    Preferencia,
    /// La preferencia es `auto` y el idioma sale de una variable de entorno.
    Variable { nombre: &'static str, valor: String },
    /// Sin preferencia ni variables útiles: inglés.
    PorDefecto,
}

/// Variables de entorno de localización, de mayor a menor precedencia.
const VARIABLES_LOCALE: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

/// Primera variable de localización con valor no vacío.
fn variable_de_locale(entorno: &dyn Fn(&str) -> Option<String>) -> Option<(&'static str, String)> {
    VARIABLES_LOCALE.iter().find_map(|&nombre| {
        entorno(nombre)
            .filter(|v| !v.is_empty())
            .map(|v| (nombre, v))
    })
}

/// Idioma efectivo: la preferencia explícita manda; con `auto` se mira el entorno
/// y todo lo que no sea español (incluidos `C` y `POSIX`) es inglés.
pub fn resolver(pref: Preferencia, entorno: &dyn Fn(&str) -> Option<String>) -> Idioma {
    match pref {
        Preferencia::Es => Idioma::Es,
        Preferencia::En => Idioma::En,
        Preferencia::Auto => match variable_de_locale(entorno) {
            Some((_, valor)) if valor.split(['_', '.', '@']).next() == Some("es") => Idioma::Es,
            _ => Idioma::En,
        },
    }
}

/// Origen del idioma que devolvería `resolver` con los mismos argumentos.
pub fn origen(pref: Preferencia, entorno: &dyn Fn(&str) -> Option<String>) -> OrigenIdioma {
    match pref {
        Preferencia::Es | Preferencia::En => OrigenIdioma::Preferencia,
        Preferencia::Auto => match variable_de_locale(entorno) {
            Some((nombre, valor)) => OrigenIdioma::Variable { nombre, valor },
            None => OrigenIdioma::PorDefecto,
        },
    }
}

/// Un texto que se puede mostrar en cualquiera de los idiomas soportados.
pub trait Localizable {
    /// El texto en `idioma`.
    fn localizar(&self, idioma: Idioma) -> String;
}

impl Localizable for TextoAviso {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::avisos::texto(self),
            Idioma::En => en::avisos::texto(self),
        }
    }
}

/// Idioma actual de la app: preferencia guardada y entorno real del proceso.
pub fn idioma_actual(rutas: &Rutas) -> Idioma {
    resolver(leer_preferencia(rutas).preferencia(), &|nombre| {
        std::env::var(nombre).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avisos::CambioAviso;

    fn env<'a>(pares: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pares
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn preferencia_explicita_manda() {
        assert_eq!(
            resolver(Preferencia::En, &env(&[("LANG", "es_ES.UTF-8")])),
            Idioma::En
        );
    }

    #[test]
    fn auto_con_lang_es() {
        for v in ["es_ES.UTF-8", "es", "es@euro", "es_MX"] {
            assert_eq!(
                resolver(Preferencia::Auto, &env(&[("LANG", v)])),
                Idioma::Es,
                "{v}"
            );
        }
    }

    #[test]
    fn auto_otro_idioma_o_c_es_ingles() {
        for v in ["en_US.UTF-8", "fr_FR", "C", "POSIX", "C.UTF-8", ""] {
            assert_eq!(
                resolver(Preferencia::Auto, &env(&[("LANG", v)])),
                Idioma::En,
                "{v}"
            );
        }
        assert_eq!(resolver(Preferencia::Auto, &env(&[])), Idioma::En);
    }

    #[test]
    fn precedencia_lc_all_lc_messages_lang() {
        let e = env(&[("LANG", "en_US"), ("LC_MESSAGES", "es_ES"), ("LC_ALL", "")]);
        assert_eq!(resolver(Preferencia::Auto, &e), Idioma::Es); // LC_ALL vacío se salta
    }

    #[test]
    fn origen_indica_de_donde_sale_el_idioma() {
        assert_eq!(
            origen(Preferencia::Es, &env(&[("LANG", "en_US")])),
            OrigenIdioma::Preferencia
        );
        assert_eq!(
            origen(
                Preferencia::Auto,
                &env(&[("LC_ALL", ""), ("LANG", "es_ES")])
            ),
            OrigenIdioma::Variable {
                nombre: "LANG",
                valor: "es_ES".to_string()
            }
        );
        assert_eq!(
            origen(Preferencia::Auto, &env(&[("LANG", "")])),
            OrigenIdioma::PorDefecto
        );
    }

    fn s(t: &str) -> String {
        t.to_string()
    }

    fn casos() -> Vec<(TextoAviso, &'static str, &'static str)> {
        vec![
            (
                TextoAviso::TituloFalloSincronizacion,
                "Fallo de sincronización",
                "Sync failure",
            ),
            (
                TextoAviso::TituloVariosFallos,
                "Varios repositorios con fallos",
                "Several repositories with failures",
            ),
            (
                TextoAviso::TituloRepositorioRecuperado,
                "Repositorio recuperado",
                "Repository recovered",
            ),
            (
                TextoAviso::TituloVariosRecuperados,
                "Varios repositorios recuperados",
                "Several repositories recovered",
            ),
            (
                TextoAviso::TituloHuerfanos,
                "Demasiados huérfanos",
                "Too many orphans",
            ),
            (
                TextoAviso::TituloHistoriaReescrita,
                "Historia reescrita",
                "History rewritten",
            ),
            (
                TextoAviso::TituloGiteaParado,
                "Gitea no responde",
                "Gitea is not responding",
            ),
            (
                TextoAviso::TituloTokenCaducado,
                "Token de GitHub caducado",
                "GitHub token expired",
            ),
            (
                TextoAviso::TituloTokenCaducaPronto,
                "Token de GitHub a punto de caducar",
                "GitHub token about to expire",
            ),
            (
                TextoAviso::FalloRepositorio {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                    detalle: None,
                },
                "El repositorio «jparga/r1» tiene un fallo en «jparga».",
                "Repository “jparga/r1” has a failure in “jparga”.",
            ),
            (
                TextoAviso::FalloRepositorio {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                    detalle: Some(s("sin red")),
                },
                "El repositorio «jparga/r1» tiene un fallo en «jparga». Detalle: sin red.",
                "Repository “jparga/r1” has a failure in “jparga”. Detail: sin red.",
            ),
            (
                TextoAviso::FallosAgregados {
                    login: s("jparga"),
                    n: 4,
                },
                "4 repositorios con fallos en «jparga».",
                "4 repositories with failures in “jparga”.",
            ),
            (
                TextoAviso::RepositorioRecuperado {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                },
                "El repositorio «jparga/r1» ha vuelto a estar bien en «jparga».",
                "Repository “jparga/r1” is back to normal in “jparga”.",
            ),
            (
                TextoAviso::RecuperadosAgregados {
                    login: s("jparga"),
                    n: 5,
                },
                "5 repositorios han vuelto a estar bien en «jparga».",
                "5 repositories are back to normal in “jparga”.",
            ),
            (
                TextoAviso::HuerfanosListaVacia { login: s("jparga") },
                "GitHub devolvió una lista vacía de repositorios en «jparga»; no se ha \
                 marcado ningún huérfano por precaución.",
                "GitHub returned an empty list of repositories in “jparga”; no orphan \
                 has been marked, as a precaution.",
            ),
            (
                TextoAviso::HuerfanosCandidatos {
                    login: s("jparga"),
                    candidatos: 5,
                    total_mirrors: 8,
                },
                "5 de 8 repositorios se marcarían huérfanos en «jparga»; no se ha \
                 aplicado por precaución.",
                "5 of 8 repositories would be marked as orphans in “jparga”; this has \
                 not been applied, as a precaution.",
            ),
            (
                TextoAviso::CambioDestructivo {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                    cambios: vec![
                        CambioAviso::HistoriaReescrita { rama: s("main") },
                        CambioAviso::RamaBorrada { rama: s("dev") },
                        CambioAviso::TagBorrado { tag: s("v1") },
                        CambioAviso::TagMovido { tag: s("v2") },
                    ],
                },
                "Historia reescrita (rama main); Rama dev borrada; Tag v1 borrado; Tag v2 \
                 movido en «jparga/r1» (jparga). La copia anterior está protegida en los \
                 snapshots.",
                "History rewritten (branch main); Branch dev deleted; Tag v1 deleted; Tag v2 \
                 moved in “jparga/r1” (jparga). The previous copy is protected in the \
                 snapshots.",
            ),
            (
                TextoAviso::GiteaParado { login: s("jparga") },
                "El Gitea de «jparga» no responde; revisa «gitmereba doctor».",
                "The Gitea of “jparga” is not responding; check “gitmereba doctor”.",
            ),
            (
                TextoAviso::TokenCaducado { login: s("jparga") },
                "El token de GitHub de «jparga» ha caducado; genera uno nuevo.",
                "The GitHub token of “jparga” has expired; generate a new one.",
            ),
            (
                TextoAviso::TokenCaducaPronto {
                    login: s("jparga"),
                    dias: 3,
                },
                "El token de GitHub de «jparga» caduca en 3 día(s).",
                "The GitHub token of “jparga” expires in 3 day(s).",
            ),
        ]
    }

    #[test]
    fn cada_frase_de_avisos_se_renderiza_en_espanol_y_en_ingles() {
        for (texto, es, en) in casos() {
            assert_eq!(texto.localizar(Idioma::Es), es, "{texto:?}");
            assert_eq!(texto.localizar(Idioma::En), en, "{texto:?}");
        }
    }
}
