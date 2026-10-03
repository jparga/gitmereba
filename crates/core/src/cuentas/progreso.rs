//! Pasos del alta, para pintar el progreso desde la CLI o desde la interfaz.

/// Cada paso de [`super::alta`] (pasos 3-8 del alta, más la validación
/// inicial y el guardado del token).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PasoAlta {
    Validar,
    GuardarToken,
    CrearCarpeta,
    EscribirConfiguracion,
    AsegurarBinario,
    Provisionar,
    ArrancarGitea,
    PrimeraSincronizacion,
    RegistrarEnAlmacen,
}

impl PasoAlta {
    /// Descripción breve en español, para mostrar junto al estado. Para mostrarla al
    /// usuario en su idioma, [`Localizable::localizar`].
    pub fn descripcion(&self) -> &'static str {
        crate::idioma::descripcion_paso_es(*self)
    }
}

/// Estado de un [`PasoAlta`] en el momento de notificar el progreso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EstadoPaso {
    Iniciando,
    Hecho,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_paso_tiene_una_descripcion_no_vacia() {
        for paso in [
            PasoAlta::Validar,
            PasoAlta::GuardarToken,
            PasoAlta::CrearCarpeta,
            PasoAlta::EscribirConfiguracion,
            PasoAlta::AsegurarBinario,
            PasoAlta::Provisionar,
            PasoAlta::ArrancarGitea,
            PasoAlta::PrimeraSincronizacion,
            PasoAlta::RegistrarEnAlmacen,
        ] {
            assert!(!paso.descripcion().is_empty());
        }
    }

    #[test]
    fn la_descripcion_sale_en_el_idioma_pedido() {
        use crate::idioma::{Idioma, Localizable};
        assert_eq!(
            PasoAlta::GuardarToken.localizar(Idioma::Es),
            "Guardando el token en el llavero"
        );
        assert_eq!(
            PasoAlta::GuardarToken.localizar(Idioma::En),
            "Saving the token in the keyring"
        );
        for paso in [
            PasoAlta::Validar,
            PasoAlta::GuardarToken,
            PasoAlta::CrearCarpeta,
            PasoAlta::EscribirConfiguracion,
            PasoAlta::AsegurarBinario,
            PasoAlta::Provisionar,
            PasoAlta::ArrancarGitea,
            PasoAlta::PrimeraSincronizacion,
            PasoAlta::RegistrarEnAlmacen,
        ] {
            assert_eq!(paso.localizar(Idioma::Es), paso.descripcion());
            assert_ne!(paso.localizar(Idioma::En), paso.localizar(Idioma::Es));
        }
    }
}
