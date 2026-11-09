//! `gitmereba apparmor`: imprime por stdout un perfil de AppArmor opcional para Gitea.
//!
//! Solo lo genera. Cargarlo exige root, así que la app no lo instala, no escribe en
//! disco ni llama a `apparmor_parser`: la persona lo guarda y lo instala con `sudo`
//! siguiendo las instrucciones de la cabecera del propio perfil.

use gitmereba_core::config::{self, Rutas};
use gitmereba_core::cuentas::ruta_binario_cacheado;
use gitmereba_core::idioma::{Idioma, Localizable, idioma_actual};
use gitmereba_core::instancia::{self, ParametrosPerfil};

use crate::salida;
use crate::textos_cli::TextoCli;

/// Perfil y número de cuentas que incluye; el error ya viene localizado.
fn generar(rutas: &Rutas, idioma: Idioma) -> Result<(String, usize), String> {
    let indice = config::leer_indice_cuentas(rutas)
        .map_err(|error| TextoCli::NoLeeIndice(error.localizar(idioma)).texto(idioma))?;
    let mut carpetas: Vec<_> = indice.cuentas.values().map(|e| e.carpeta.clone()).collect();
    carpetas.sort();
    let binario = ruta_binario_cacheado(rutas);
    let directorio_datos = rutas.directorio_datos();
    let perfil = instancia::generar_perfil(&ParametrosPerfil {
        binario_gitea: &binario,
        directorio_datos,
        carpetas_cuenta: &carpetas,
    })
    .map_err(|error| TextoCli::ApparmorRutaInvalida(error.localizar(idioma)).texto(idioma))?;
    Ok((perfil, carpetas.len()))
}

pub fn ejecutar(rutas: &Rutas) -> u8 {
    let idioma = idioma_actual(rutas);
    match generar(rutas, idioma) {
        Ok((perfil, cuentas)) => {
            if cuentas == 0 {
                let mut stderr = std::io::stderr().lock();
                salida::linea(&mut stderr, &TextoCli::ApparmorSinCuentas.texto(idioma));
            }
            let mut stdout = std::io::stdout().lock();
            // El perfil ya termina en salto de línea.
            salida::linea(&mut stdout, perfil.trim_end_matches('\n'));
            0
        }
        Err(mensaje) => {
            let mut stderr = std::io::stderr().lock();
            salida::linea(&mut stderr, &format!("error: {mensaje}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cuentas_el_perfil_empieza_por_la_cabecera_y_no_escribe_ficheros() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let (perfil, cuentas) = generar(&rutas, Idioma::Es).expect("perfil");
        assert_eq!(cuentas, 0);
        assert!(perfil.starts_with("# Perfil de AppArmor"));
        assert!(perfil.contains("flags=(complain)"));
        let restos = std::fs::read_dir(temporal.path()).expect("leer").count();
        assert_eq!(restos, 0, "el comando no debe escribir nada");
    }

    #[test]
    fn con_cuentas_incluye_su_carpeta() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let carpeta = temporal.path().join("cuenta ana");
        let mut indice = config::IndiceCuentas::default();
        indice.cuentas.insert(
            "ana".to_string(),
            config::EntradaCuenta {
                carpeta: carpeta.clone(),
                puerto: 3001,
            },
        );
        config::escribir_indice_cuentas(&rutas, &indice).expect("índice");
        let (perfil, cuentas) = generar(&rutas, Idioma::En).expect("perfil");
        assert_eq!(cuentas, 1);
        assert!(perfil.contains(&format!("\"{}/**\" rwk,", carpeta.display())));
    }
}
