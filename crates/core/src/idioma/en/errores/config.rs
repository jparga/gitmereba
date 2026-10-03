//! Errores de configuración, en inglés.

use crate::config::ErrorConfig;

pub(crate) fn texto(e: &ErrorConfig) -> String {
    match e {
        ErrorConfig::SinDirectorioHome => "the user's home directory was not found".to_string(),
        ErrorConfig::Io(detalle) => format!("I/O error: {detalle}"),
        ErrorConfig::Toml(detalle) => format!("error reading or writing TOML: {detalle}"),
        ErrorConfig::CarpetaNoAbsoluta => {
            "the destination folder must be an absolute path".to_string()
        }
        ErrorConfig::CarpetaRaiz => {
            "the destination folder cannot be the filesystem root".to_string()
        }
        ErrorConfig::CarpetaHome => {
            "the destination folder cannot be the user's home directory".to_string()
        }
        ErrorConfig::CarpetaDentroDeDatos => {
            "the destination folder cannot be inside the app's data directory".to_string()
        }
        ErrorConfig::CarpetaNoVacia => {
            "the destination folder must be empty or not exist".to_string()
        }
        ErrorConfig::LoginDuplicado(login) => {
            format!("the login “{login}” is already in use by another account")
        }
        ErrorConfig::CarpetaDuplicada(ruta) => format!(
            "the folder “{}” is already in use by another account",
            ruta.display()
        ),
        ErrorConfig::PuertoFueraDeRango => "the port must be between 1024 and 65535".to_string(),
        ErrorConfig::PuertoDuplicado(puerto) => {
            format!("port {puerto} is already in use by another account")
        }
        ErrorConfig::IntervaloDemasiadoCorto => {
            "the minimum interval between synchronizations is 10 minutes".to_string()
        }
    }
}
