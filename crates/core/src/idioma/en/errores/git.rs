//! Errores de la invocación de `git`, en inglés.

use crate::git::ErrorGit;

pub(crate) fn texto(e: &ErrorGit) -> String {
    match e {
        ErrorGit::NoEncontrado => "git is not installed or not found in the PATH".to_string(),
        ErrorGit::Timeout => "the git command exceeded the time limit".to_string(),
        ErrorGit::Fallo { codigo, stderr } => format!("git exited with code {codigo}: {stderr}"),
        ErrorGit::EntradaInvalida(detalle) => format!("invalid input: {detalle}"),
        ErrorGit::Sistema(detalle) => format!("system error invoking git: {detalle}"),
    }
}
