//! Motivo por el que [`super::evaluar`] ha llegado a un [`super::Diagnostico`] concreto.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Por qué se ha llegado a un estado concreto. Cada variante tiene un texto en español
/// pensado para mostrarse al usuario (vía [`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Motivo {
    /// `git fsck` ha encontrado un objeto corrupto o un enlace roto en el bare local.
    Corrupcion,
    /// No se ha podido leer el estado local del repositorio (refs, fsck...).
    ErrorLocal,
    /// El mirror no está vacío en GitHub pero nunca ha sincronizado, y ya ha pasado el
    /// periodo de gracia inicial.
    NuncaSincronizado,
    /// El SHA de la rama por defecto no coincide entre GitHub y el mirror local, y la
    /// última sincronización ya es antigua.
    ShaDistinto,
    /// El SHA difiere pero la última sincronización es reciente: probablemente GitHub
    /// acaba de recibir un push y el mirror todavía no lo ha tirado.
    PendienteDeSincronizar,
}

impl fmt::Display for Motivo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let texto = match self {
            Motivo::Corrupcion => "el repositorio local está corrupto (git fsck ha fallado)",
            Motivo::ErrorLocal => "no se ha podido leer el estado local del repositorio",
            Motivo::NuncaSincronizado => {
                "el mirror no ha sincronizado nunca y ya ha pasado el periodo de gracia inicial"
            }
            Motivo::ShaDistinto => "el SHA de la rama por defecto no coincide con el de GitHub",
            Motivo::PendienteDeSincronizar => {
                "el SHA difiere pero la sincronización es reciente; probablemente está pendiente de tirar"
            }
        };
        f.write_str(texto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todas_las_variantes_tienen_texto_en_espanol_no_vacio() {
        for motivo in [
            Motivo::Corrupcion,
            Motivo::ErrorLocal,
            Motivo::NuncaSincronizado,
            Motivo::ShaDistinto,
            Motivo::PendienteDeSincronizar,
        ] {
            assert!(!motivo.to_string().is_empty());
        }
    }

    #[test]
    fn se_serializa_en_kebab_case() {
        assert_eq!(
            serde_json::to_string(&Motivo::PendienteDeSincronizar).unwrap(),
            "\"pendiente-de-sincronizar\""
        );
    }
}
