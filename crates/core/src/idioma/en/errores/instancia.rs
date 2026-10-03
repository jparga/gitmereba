//! Errores de la instancia de Gitea, en inglés.

use crate::idioma::{Idioma, Localizable};
use crate::instancia::ErrorInstancia;

pub(crate) fn texto(e: &ErrorInstancia) -> String {
    match e {
        ErrorInstancia::Io(detalle) => format!("I/O error: {detalle}"),
        ErrorInstancia::Red(detalle) => format!("network error: {detalle}"),
        ErrorInstancia::HostNoPermitido(host) => format!("download host not allowed: {host}"),
        ErrorInstancia::TamanoExcedido(mb) => {
            format!("the download exceeds the maximum allowed size ({mb} MB)")
        }
        ErrorInstancia::Sha256NoCoincide => {
            "the SHA-256 of the downloaded binary does not match the expected one".to_string()
        }
        ErrorInstancia::GpgvNoDisponible => "gpgv is not installed on the system".to_string(),
        ErrorInstancia::FirmaInvalida => {
            "the GPG signature of the Gitea binary is not valid".to_string()
        }
        ErrorInstancia::GiteaNoEncontrado => {
            "gitea is not installed or not found at the given path".to_string()
        }
        ErrorInstancia::GiteaFallo { codigo, stderr } => {
            format!("gitea exited with code {codigo}: {stderr}")
        }
        ErrorInstancia::SystemctlFallo { codigo, stderr } => {
            format!("systemctl exited with code {codigo}: {stderr}")
        }
        ErrorInstancia::Timeout => "the time limit was exceeded waiting for a process".to_string(),
        ErrorInstancia::ArranqueTimeout => {
            "gitea did not respond in time after starting".to_string()
        }
        ErrorInstancia::SinPuertoLibre => "there is no free port in the assigned range".to_string(),
        ErrorInstancia::EntradaInvalida(detalle) => format!("invalid input: {detalle}"),
        ErrorInstancia::UsuarioGiteaYaExiste(usuario) => {
            format!("the user “{usuario}” already exists on Gitea")
        }
        ErrorInstancia::Llavero(interno) => {
            format!(
                "error accessing the keyring: {}",
                interno.localizar(Idioma::En)
            )
        }
    }
}
