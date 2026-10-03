//! Descripciones de los pasos del alta, en español.

use crate::cuentas::PasoAlta;

pub(crate) fn descripcion(paso: PasoAlta) -> &'static str {
    match paso {
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
