//! Formato de salida: tablas legibles y el patrón de error para el usuario.
//!
//! Toda la salida de usuario va por stdout con `writeln!` (el lint `print_stdout` está
//! en `-D warnings`: nunca `println!`).

use std::io::Write;

use gitmereba_core::cuentas::ErrorCuentas;
use gitmereba_core::idioma::{Idioma, Localizable};

use crate::textos_cli::TextoCli;

/// Escribe `linea` en `stdout` sin más adorno.
pub fn linea(stdout: &mut impl Write, contenido: &str) {
    let _ = writeln!(stdout, "{contenido}");
}

/// Imprime un error como lo vería el usuario: `error: <mensaje>` y, si existe, una
/// línea de consejo. Nunca un `Debug` de un error (podría filtrar detalles internos).
pub fn error(stderr: &mut impl Write, error: &ErrorCuentas, idioma: Idioma) {
    let _ = writeln!(stderr, "error: {}", error.localizar(idioma));
    if let Some(consejo) = consejo(error) {
        let _ = writeln!(
            stderr,
            "{}",
            TextoCli::Consejo(consejo.texto(idioma)).texto(idioma)
        );
    }
}

/// El consejo breve para los errores más comunes de usuario.
fn consejo(error: &ErrorCuentas) -> Option<TextoCli> {
    match error.codigo() {
        "token-invalido" => Some(TextoCli::ConsejoTokenInvalido),
        "gitea-parado" => Some(TextoCli::ConsejoGiteaParado),
        "cuenta-no-existe" => Some(TextoCli::ConsejoCuentaNoExiste),
        "carpeta-no-vacia" => Some(TextoCli::ConsejoCarpetaNoVacia),
        "login-duplicado" => Some(TextoCli::ConsejoLoginDuplicado),
        "carpeta-duplicada" | "puerto-duplicado" => Some(TextoCli::ConsejoRecursoDuplicado),
        "carpeta-no-valida-para-borrar" | "ruta-protegida" => Some(TextoCli::ConsejoNadaBorrado),
        _ => None,
    }
}

/// Tabla simple alineada por columnas, sin dependencias externas.
pub fn tabla(stdout: &mut impl Write, encabezados: &[&str], filas: &[Vec<String>]) {
    let mut anchos: Vec<usize> = encabezados.iter().map(|h| h.chars().count()).collect();
    for fila in filas {
        for (i, celda) in fila.iter().enumerate() {
            if let Some(ancho) = anchos.get_mut(i) {
                *ancho = (*ancho).max(celda.chars().count());
            }
        }
    }

    linea(stdout, &formatear_fila(encabezados, &anchos));
    let separador: Vec<String> = anchos.iter().map(|a| "-".repeat(*a)).collect();
    linea(
        stdout,
        &separador
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("  "),
    );
    for fila in filas {
        let celdas: Vec<&str> = fila.iter().map(String::as_str).collect();
        linea(stdout, &formatear_fila(&celdas, &anchos));
    }
}

fn formatear_fila(celdas: &[&str], anchos: &[usize]) -> String {
    celdas
        .iter()
        .enumerate()
        .map(|(i, celda)| {
            let ancho = anchos.get(i).copied().unwrap_or(celda.len());
            format!("{celda:<ancho$}")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabla_alinea_columnas() {
        let mut salida = Vec::new();
        tabla(
            &mut salida,
            &["login", "puerto"],
            &[vec!["jparga".to_string(), "33000".to_string()]],
        );
        let texto = String::from_utf8(salida).expect("utf8");
        assert!(texto.contains("login "));
        assert!(texto.contains("jparga"));
    }

    #[test]
    fn error_nunca_imprime_un_debug() {
        let mut salida = Vec::new();
        let fallo = ErrorCuentas::GiteaParado(
            gitmereba_core::modelo::Nombre::nuevo("jparga").expect("nombre"),
        );
        error(&mut salida, &fallo, Idioma::Es);
        let texto = String::from_utf8(salida).expect("utf8");
        assert!(texto.starts_with("error: "));
        assert!(!texto.contains("GiteaParado"));
    }

    #[test]
    fn el_consejo_sale_en_el_idioma_pedido() {
        let fallo = ErrorCuentas::GiteaParado(
            gitmereba_core::modelo::Nombre::nuevo("jparga").expect("nombre"),
        );
        let mut es = Vec::new();
        error(&mut es, &fallo, Idioma::Es);
        let mut en = Vec::new();
        error(&mut en, &fallo, Idioma::En);
        let es = String::from_utf8(es).expect("utf8");
        let en = String::from_utf8(en).expect("utf8");
        assert!(es.contains("\nconsejo: arráncalo con «systemctl --user start»"));
        assert!(en.contains("\nhint: start it with"));
        assert!(!en.contains("consejo"));
    }

    #[test]
    fn error_sale_en_el_idioma_pedido() {
        let fallo = ErrorCuentas::GiteaParado(
            gitmereba_core::modelo::Nombre::nuevo("jparga").expect("nombre"),
        );
        let mut es = Vec::new();
        error(&mut es, &fallo, Idioma::Es);
        let mut en = Vec::new();
        error(&mut en, &fallo, Idioma::En);
        let es = String::from_utf8(es).expect("utf8");
        let en = String::from_utf8(en).expect("utf8");
        assert!(es.contains(&fallo.localizar(Idioma::Es)));
        assert!(en.contains(&fallo.localizar(Idioma::En)));
        assert_ne!(es, en);
    }
}
