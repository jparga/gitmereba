//! Errores de los snapshots, en inglés.

use crate::idioma::{Idioma, Localizable};
use crate::snapshots::ErrorSnapshots;

pub(crate) fn texto(e: &ErrorSnapshots) -> String {
    match e {
        ErrorSnapshots::Git(interno) => format!("git: {}", interno.localizar(Idioma::En)),
        ErrorSnapshots::Io(detalle) => format!("I/O error: {detalle}"),
        ErrorSnapshots::Json(detalle) => {
            format!("error reading or writing the manifest: {detalle}")
        }
        ErrorSnapshots::OrigenNoExiste => {
            "the source repository does not exist in “gitea/repositories/”".to_string()
        }
        ErrorSnapshots::OrigenFueraDeRepositorios => "the source repository ends up outside \
            “gitea/repositories/” after resolving links"
            .to_string(),
        ErrorSnapshots::SinCapturas => "there are no captures for this repository".to_string(),
        ErrorSnapshots::DestinoFueraDeSnapshots => {
            "the snapshot ends up outside “snapshots/” after resolving links".to_string()
        }
        ErrorSnapshots::DestinoExiste => "the restore destination already exists".to_string(),
        ErrorSnapshots::DestinoInvalido => {
            "the restore destination is not a valid path".to_string()
        }
        ErrorSnapshots::DestinoDentroDeGiteaOSnapshots => {
            "the destination cannot be inside “gitea/” or “snapshots/”".to_string()
        }
        ErrorSnapshots::MarcaInvalida(marca) => {
            format!("the timestamp “{marca}” does not have the format YYYYMMDDTHHMMSSZ")
        }
        ErrorSnapshots::CapturaNoEncontrada(marca) => format!("capture “{marca}” was not found"),
        ErrorSnapshots::ManifiestoDemasiadoGrande(bytes) => {
            format!("the manifest takes {bytes} bytes, more than the allowed maximum")
        }
        ErrorSnapshots::ShaInvalido => {
            "the manifest contains a SHA with an invalid format".to_string()
        }
        ErrorSnapshots::RutaNoUtf8 => "a path contains bytes that are not valid UTF-8".to_string(),
        ErrorSnapshots::RestauracionInconsistente => {
            "the references after restoring do not match the manifest".to_string()
        }
    }
}
