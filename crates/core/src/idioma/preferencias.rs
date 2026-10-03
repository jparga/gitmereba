//! Preferencia de idioma guardada en `preferencias.toml`.

use std::fs::File;
use std::io::{ErrorKind, Read};

use serde::{Deserialize, Serialize};

use crate::config::fichero::escribir_privado;
use crate::config::{ErrorConfig, Rutas};

/// Tamaño máximo que se acepta para `preferencias.toml`: es un fichero minúsculo y
/// no se lee nada que lo supere.
const TAMANO_MAXIMO: u64 = 4096;

/// Idioma elegido por el usuario; `Auto` sigue el entorno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preferencia {
    #[default]
    Auto,
    Es,
    En,
}

/// Resultado de leer `preferencias.toml`; `doctor` avisa si es `NoValida`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LecturaPreferencia {
    Ausente,
    Valida(Preferencia),
    NoValida,
}

impl LecturaPreferencia {
    /// Preferencia efectiva: sin fichero o con fichero no válido, `Auto`.
    pub fn preferencia(&self) -> Preferencia {
        match self {
            Self::Valida(p) => *p,
            Self::Ausente | Self::NoValida => Preferencia::Auto,
        }
    }
}

/// Contenido de `preferencias.toml`; las claves desconocidas se ignoran.
#[derive(Debug, Default, Serialize, Deserialize)]
struct FicheroPreferencias {
    #[serde(default)]
    idioma: Preferencia,
}

/// Lee la preferencia guardada. Nunca falla: cualquier problema es `NoValida`.
pub fn leer_preferencia(rutas: &Rutas) -> LecturaPreferencia {
    let fichero = match File::open(rutas.fichero_preferencias()) {
        Ok(fichero) => fichero,
        Err(error) if error.kind() == ErrorKind::NotFound => return LecturaPreferencia::Ausente,
        Err(_) => return LecturaPreferencia::NoValida,
    };
    let mut texto = String::new();
    // Un byte más que el máximo basta para detectar que se pasa, sin leerlo entero.
    if fichero
        .take(TAMANO_MAXIMO + 1)
        .read_to_string(&mut texto)
        .is_err()
        || texto.len() as u64 > TAMANO_MAXIMO
    {
        return LecturaPreferencia::NoValida;
    }
    match toml::from_str::<FicheroPreferencias>(&texto) {
        Ok(contenido) => LecturaPreferencia::Valida(contenido.idioma),
        Err(_) => LecturaPreferencia::NoValida,
    }
}

/// Guarda la preferencia (fichero 0600, directorio 0700 si hay que crearlo).
pub fn guardar_preferencia(rutas: &Rutas, p: Preferencia) -> Result<(), ErrorConfig> {
    let texto = toml::to_string(&FicheroPreferencias { idioma: p })
        .map_err(|error| ErrorConfig::Toml(error.to_string()))?;
    escribir_privado(&rutas.fichero_preferencias(), texto.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn rutas() -> (tempfile::TempDir, Rutas) {
        let dir = tempfile::tempdir().expect("tempdir");
        let rutas = Rutas::con_raiz(dir.path());
        (dir, rutas)
    }

    fn escribir(rutas: &Rutas, contenido: &str) {
        fs::create_dir_all(rutas.directorio_config()).expect("crear directorio");
        fs::write(rutas.fichero_preferencias(), contenido).expect("escribir");
    }

    #[test]
    fn sin_fichero_esta_ausente() {
        let (_dir, rutas) = rutas();
        let lectura = leer_preferencia(&rutas);
        assert_eq!(lectura, LecturaPreferencia::Ausente);
        assert_eq!(lectura.preferencia(), Preferencia::Auto);
    }

    #[test]
    fn guardar_y_releer() {
        let (_dir, rutas) = rutas();
        guardar_preferencia(&rutas, Preferencia::En).expect("guardar");
        assert_eq!(
            leer_preferencia(&rutas),
            LecturaPreferencia::Valida(Preferencia::En)
        );
        let texto = fs::read_to_string(rutas.fichero_preferencias()).expect("leer");
        assert_eq!(texto.trim(), r#"idioma = "en""#);
    }

    #[test]
    fn valores_o_contenido_no_validos() {
        let largo = format!("# {}\n", "x".repeat(5000));
        for contenido in [
            r#"idioma = "fr""#,
            "esto no es toml [[[",
            r#"idioma = 3"#,
            largo.as_str(),
        ] {
            let (_dir, rutas) = rutas();
            escribir(&rutas, contenido);
            let lectura = leer_preferencia(&rutas);
            assert_eq!(lectura, LecturaPreferencia::NoValida, "{contenido:.30}");
            assert_eq!(lectura.preferencia(), Preferencia::Auto);
        }
    }

    #[test]
    fn claves_desconocidas_se_ignoran() {
        let (_dir, rutas) = rutas();
        escribir(&rutas, "idioma = \"es\"\ntema = \"oscuro\"\n");
        assert_eq!(
            leer_preferencia(&rutas),
            LecturaPreferencia::Valida(Preferencia::Es)
        );
    }

    #[test]
    fn fichero_sin_clave_idioma_es_auto() {
        let (_dir, rutas) = rutas();
        escribir(&rutas, "tema = \"oscuro\"\n");
        assert_eq!(
            leer_preferencia(&rutas),
            LecturaPreferencia::Valida(Preferencia::Auto)
        );
    }

    #[test]
    fn guardar_crea_con_permisos_privados() {
        let (_dir, rutas) = rutas();
        guardar_preferencia(&rutas, Preferencia::Es).expect("guardar");
        let modo =
            |p: &std::path::Path| fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(modo(&rutas.fichero_preferencias()), 0o600);
        assert_eq!(modo(rutas.directorio_config()), 0o700);
    }
}
