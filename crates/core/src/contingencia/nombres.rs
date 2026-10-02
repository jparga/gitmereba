//! Nombre de la organización de contingencia de un dueño.

use crate::modelo::{ErrorNombre, Nombre};

const PREFIJO: &str = "contingencia-";

/// Organización de contingencia del dueño `original`: `contingencia-<original>`. Es una
/// función pura de validación: falla si el resultado supera la longitud máxima de
/// [`Nombre`] o, en teoría, contuviera algún carácter no permitido (no puede pasar
/// partiendo de un `Nombre` ya válido, salvo por la longitud).
pub fn org_contingencia(original: &Nombre) -> Result<Nombre, ErrorNombre> {
    Nombre::nuevo(format!("{PREFIJO}{original}"))
}

/// `true` si `dueno` es una organización de contingencia (`contingencia-<dueño>`), es
/// decir, si sus repos son copias hermanas con escritura y no mirrors de GitHub.
pub fn es_org_contingencia(dueno: &Nombre) -> bool {
    dueno.as_str().to_ascii_lowercase().starts_with(PREFIJO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconoce_las_organizaciones_de_contingencia() {
        let hermana = Nombre::nuevo("Contingencia-jparga").expect("válido");
        let normal = Nombre::nuevo("jparga").expect("válido");
        assert!(es_org_contingencia(&hermana));
        assert!(!es_org_contingencia(&normal));
    }

    #[test]
    fn antepone_el_prefijo_de_contingencia() {
        let dueno = Nombre::nuevo("jparga").expect("nombre de prueba válido");
        assert_eq!(
            org_contingencia(&dueno).expect("válido").as_str(),
            "contingencia-jparga"
        );
    }

    #[test]
    fn falla_si_el_resultado_supera_la_longitud_maxima() {
        let dueno = Nombre::nuevo("a".repeat(95)).expect("nombre de prueba válido");
        assert!(matches!(
            org_contingencia(&dueno),
            Err(ErrorNombre::DemasiadoLargo(_))
        ));
    }

    #[test]
    fn el_prefijo_no_cambia_el_orden_relativo_del_dueno() {
        // El nombre de contingencia sigue siendo determinista y estable frente al mismo
        // dueño: llamar dos veces da el mismo resultado.
        let dueno = Nombre::nuevo("acme").expect("nombre de prueba válido");
        assert_eq!(
            org_contingencia(&dueno).expect("válido"),
            org_contingencia(&dueno).expect("válido")
        );
    }
}
