//! Validación de nombres de referencia que llegan de fuera del proceso (usuario, red).

use crate::git::proceso::ErrorGit;

/// Caracteres que git interpreta especialmente en un nombre de referencia.
const CARACTERES_PROHIBIDOS: [char; 7] = ['~', '^', ':', '?', '*', '[', '\\'];

/// Valida un nombre de referencia (rama, tag o SHA) antes de pasarlo a `git`.
///
/// Rechaza cadenas vacías, las que empiezan por `-` (podrían leerse como opción),
/// las que contienen `..`, espacios, caracteres de control, `~^:?*[\` o `@{`.
pub fn validar_ref(referencia: &str) -> Result<(), ErrorGit> {
    if referencia.is_empty() {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia está vacía".to_string(),
        ));
    }
    if referencia.starts_with('-') {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia no puede empezar por «-»".to_string(),
        ));
    }
    if referencia.contains("..") {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia no puede contener «..»".to_string(),
        ));
    }
    if referencia.contains("@{") {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia no puede contener «@{»".to_string(),
        ));
    }
    if referencia.chars().any(char::is_control) {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia no puede contener caracteres de control".to_string(),
        ));
    }
    if referencia.chars().any(char::is_whitespace) {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia no puede contener espacios".to_string(),
        ));
    }
    if referencia
        .chars()
        .any(|c| CARACTERES_PROHIBIDOS.contains(&c))
    {
        return Err(ErrorGit::EntradaInvalida(
            "la referencia contiene caracteres no permitidos".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acepta_referencias_normales() {
        for referencia in [
            "main",
            "refs/heads/main",
            "origin/master",
            "a1b2c3d",
            "feat/x-1",
        ] {
            assert!(validar_ref(referencia).is_ok(), "{referencia}");
        }
    }

    #[test]
    fn rechaza_vacia() {
        assert!(validar_ref("").is_err());
    }

    #[test]
    fn rechaza_opcion_disfrazada_de_referencia() {
        assert!(validar_ref("--upload-pack=x").is_err());
        assert!(validar_ref("-rf").is_err());
    }

    #[test]
    fn rechaza_rangos_y_dobles_puntos() {
        assert!(validar_ref("a..b").is_err());
        assert!(validar_ref("..").is_err());
    }

    #[test]
    fn rechaza_espacios() {
        assert!(validar_ref("x y").is_err());
    }

    #[test]
    fn rechaza_caracteres_de_control() {
        assert!(validar_ref("a\nb").is_err());
        assert!(validar_ref("a\tb").is_err());
    }

    #[test]
    fn rechaza_caracteres_especiales_de_git() {
        for referencia in ["a~1", "a^2", "a:b", "a?b", "a*b", "a[b", "a\\b", "a@{1}"] {
            assert!(validar_ref(referencia).is_err(), "{referencia}");
        }
    }
}
