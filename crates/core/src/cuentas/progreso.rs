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
    /// Descripción breve en español, para mostrar junto al estado.
    pub fn descripcion(&self) -> &'static str {
        match self {
            PasoAlta::Validar => "Validando los datos de la cuenta",
            PasoAlta::GuardarToken => "Guardando el token en el llavero",
            PasoAlta::CrearCarpeta => "Creando la carpeta de la cuenta",
            PasoAlta::EscribirConfiguracion => "Escribiendo la configuración",
            PasoAlta::AsegurarBinario => "Comprobando el binario de Gitea",
            PasoAlta::Provisionar => "Provisionando Gitea",
            PasoAlta::ArrancarGitea => "Arrancando Gitea",
            PasoAlta::PrimeraSincronizacion => "Sincronizando por primera vez",
            PasoAlta::RegistrarEnAlmacen => "Registrando la auditoría",
        }
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
}
