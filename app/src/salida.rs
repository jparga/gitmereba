//! Formato de salida: tablas legibles y el patrón de error para el usuario.
//!
//! Toda la salida de usuario va por stdout con `writeln!` (el lint `print_stdout` está
//! en `-D warnings`: nunca `println!`).

use std::io::Write;

use gitmereba_core::cuentas::ErrorCuentas;

/// Escribe `linea` en `stdout` sin más adorno.
pub fn linea(stdout: &mut impl Write, contenido: &str) {
    let _ = writeln!(stdout, "{contenido}");
}

/// Imprime un error como lo vería el usuario: `error: <mensaje>` y, si existe, una
/// línea de consejo. Nunca un `Debug` de un error (podría filtrar detalles internos).
pub fn error(stderr: &mut impl Write, error: &ErrorCuentas) {
    let _ = writeln!(stderr, "error: {error}");
    if let Some(consejo) = consejo(error) {
        let _ = writeln!(stderr, "consejo: {consejo}");
    }
}

/// Un consejo breve en español para los errores más comunes de usuario.
fn consejo(error: &ErrorCuentas) -> Option<&'static str> {
    match error.codigo() {
        "token-invalido" => {
            Some("comprueba que el token no ha caducado y tiene permiso de lectura")
        }
        "gitea-parado" => {
            Some("arráncalo con «systemctl --user start» o revisa «gitmereba doctor»")
        }
        "cuenta-no-existe" => Some("consulta los logins dados de alta con «gitmereba cuenta list»"),
        "carpeta-no-vacia" => Some("elige una carpeta vacía o inexistente"),
        "login-duplicado" => Some("ya hay una cuenta con ese login; usa «gitmereba cuenta list»"),
        "carpeta-duplicada" | "puerto-duplicado" => {
            Some("esa carpeta o puerto ya los usa otra cuenta")
        }
        "carpeta-no-valida-para-borrar" | "ruta-protegida" => {
            Some("no se ha borrado nada; revisa la ruta a mano")
        }
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
        error(&mut salida, &fallo);
        let texto = String::from_utf8(salida).expect("utf8");
        assert!(texto.starts_with("error: "));
        assert!(!texto.contains("GiteaParado"));
    }
}
