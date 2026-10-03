//! Descripciones de los pasos del alta, en inglés.

use crate::cuentas::PasoAlta;

pub(crate) fn descripcion(paso: PasoAlta) -> &'static str {
    match paso {
        PasoAlta::Validar => "Validating the account data",
        PasoAlta::GuardarToken => "Saving the token in the keyring",
        PasoAlta::CrearCarpeta => "Creating the account folder",
        PasoAlta::EscribirConfiguracion => "Writing the configuration",
        PasoAlta::AsegurarBinario => "Checking the Gitea binary",
        PasoAlta::Provisionar => "Provisioning Gitea",
        PasoAlta::ArrancarGitea => "Starting Gitea",
        PasoAlta::PrimeraSincronizacion => "Running the first sync",
        PasoAlta::RegistrarEnAlmacen => "Recording the audit entry",
    }
}
