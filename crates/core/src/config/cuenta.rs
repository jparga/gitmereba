//! Lectura y escritura de `gitmereba.toml` (por cuenta) y `cuentas.toml` (índice).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::error::ErrorConfig;
use crate::config::fichero::escribir_privado;
use crate::config::rutas::{Rutas, RutasCuenta};
use crate::modelo::Cuenta;

/// Una entrada del índice `cuentas.toml`: dónde vive la cuenta y en qué puerto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaCuenta {
    pub carpeta: PathBuf,
    pub puerto: u16,
}

/// Índice login → carpeta y puerto, en `~/.local/share/gitmereba/cuentas.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndiceCuentas {
    #[serde(default)]
    pub cuentas: HashMap<String, EntradaCuenta>,
}

/// Lee `cuentas.toml`; si no existe todavía, devuelve un índice vacío.
pub fn leer_indice_cuentas(rutas: &Rutas) -> Result<IndiceCuentas, ErrorConfig> {
    let ruta = rutas.fichero_cuentas();
    if !ruta.exists() {
        return Ok(IndiceCuentas::default());
    }
    let contenido =
        fs::read_to_string(&ruta).map_err(|error| ErrorConfig::Io(error.to_string()))?;
    toml::from_str(&contenido).map_err(|error| ErrorConfig::Toml(error.to_string()))
}

/// Escribe `cuentas.toml` de forma atómica, con permisos 0600.
pub fn escribir_indice_cuentas(rutas: &Rutas, indice: &IndiceCuentas) -> Result<(), ErrorConfig> {
    let contenido =
        toml::to_string_pretty(indice).map_err(|error| ErrorConfig::Toml(error.to_string()))?;
    escribir_privado(&rutas.fichero_cuentas(), contenido.as_bytes())
}

/// Lee `gitmereba.toml` de una cuenta.
pub fn leer_cuenta(rutas_cuenta: &RutasCuenta) -> Result<Cuenta, ErrorConfig> {
    let contenido = fs::read_to_string(rutas_cuenta.fichero_config())
        .map_err(|error| ErrorConfig::Io(error.to_string()))?;
    toml::from_str(&contenido).map_err(|error| ErrorConfig::Toml(error.to_string()))
}

/// Escribe `gitmereba.toml` de una cuenta de forma atómica, con permisos 0600.
///
/// `cuenta` no lleva secretos: esos van al llavero, nunca a este fichero.
pub fn escribir_cuenta(rutas_cuenta: &RutasCuenta, cuenta: &Cuenta) -> Result<(), ErrorConfig> {
    let contenido =
        toml::to_string_pretty(cuenta).map_err(|error| ErrorConfig::Toml(error.to_string()))?;
    escribir_privado(&rutas_cuenta.fichero_config(), contenido.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::modelo::{Alcance, Nombre};

    fn cuenta_de_prueba() -> Cuenta {
        Cuenta {
            login: Nombre::nuevo("jparga").expect("nombre válido"),
            carpeta: PathBuf::from("/cuentas/jparga"),
            puerto: 3999,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![Nombre::nuevo("mereba").expect("nombre válido")],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    #[test]
    fn ida_y_vuelta_de_gitmereba_toml() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let rutas_cuenta = RutasCuenta::nueva(directorio.path());
        let cuenta = cuenta_de_prueba();

        escribir_cuenta(&rutas_cuenta, &cuenta).expect("escribir_cuenta no falla");
        let leida = leer_cuenta(&rutas_cuenta).expect("leer_cuenta no falla");

        assert_eq!(leida, cuenta);
    }

    #[test]
    fn ida_y_vuelta_de_cuentas_toml() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(directorio.path());
        let mut indice = IndiceCuentas::default();
        indice.cuentas.insert(
            "jparga".to_string(),
            EntradaCuenta {
                carpeta: PathBuf::from("/cuentas/jparga"),
                puerto: 3999,
            },
        );

        escribir_indice_cuentas(&rutas, &indice).expect("escribir_indice_cuentas no falla");
        let leido = leer_indice_cuentas(&rutas).expect("leer_indice_cuentas no falla");

        assert_eq!(leido, indice);
    }

    #[test]
    fn leer_indice_sin_fichero_devuelve_vacio() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(directorio.path());

        let indice = leer_indice_cuentas(&rutas).expect("leer_indice_cuentas no falla");

        assert!(indice.cuentas.is_empty());
    }
}
