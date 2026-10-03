//! Idioma de la interfaz: resolución a partir de la preferencia y del entorno.

mod preferencias;

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

/// Idioma actual de la app: preferencia guardada y entorno real del proceso.
pub fn idioma_actual(rutas: &Rutas) -> Idioma {
    resolver(leer_preferencia(rutas).preferencia(), &|nombre| {
        std::env::var(nombre).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
