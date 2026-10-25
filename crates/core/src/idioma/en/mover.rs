//! Texts of `mover_carpeta`, in English.

use crate::cuentas::{AvisoMover, MotivoDestino, ProblemaCopia, ProgresoMover};

pub(crate) fn motivo_destino(motivo: &MotivoDestino) -> String {
    match motivo {
        MotivoDestino::NoAbsoluta => "the folder must be an absolute path".to_string(),
        MotivoDestino::ConPuntoPunto => "the path cannot contain “..”".to_string(),
        MotivoDestino::NoUtf8 => "the path is not valid UTF-8 text".to_string(),
        MotivoDestino::CaracterNoPermitido { caracter } => format!(
            "the path contains a character that is not allowed ({})",
            caracter.escape_debug()
        ),
        MotivoDestino::Protegida => {
            "it cannot be the root, the HOME or an ancestor of the HOME".to_string()
        }
        MotivoDestino::DentroDeDatosDeLaApp => {
            "it cannot be inside the app’s data directory".to_string()
        }
        MotivoDestino::SinPadre => {
            "the folder that would contain the destination does not exist or is not a directory"
                .to_string()
        }
        MotivoDestino::PadreAjeno => {
            "the folder that would contain the destination does not belong to your user".to_string()
        }
        MotivoDestino::EsEnlaceSimbolico => "the destination is a symbolic link".to_string(),
        MotivoDestino::NoEsDirectorio => {
            "the destination exists and is not a directory".to_string()
        }
        MotivoDestino::NoVacio => "the destination must be empty or not exist".to_string(),
        MotivoDestino::DentroDeOtraCuenta { login, carpeta } => format!(
            "the destination is inside the folder of the account “{login}” ({})",
            carpeta.display()
        ),
        MotivoDestino::ContieneOtraCuenta { login, carpeta } => format!(
            "the destination contains the folder of the account “{login}” ({})",
            carpeta.display()
        ),
        MotivoDestino::DentroDeLaPropia => {
            "the destination is the account’s current folder or is inside it".to_string()
        }
        MotivoDestino::ContieneLaPropia => {
            "the destination contains the account’s current folder".to_string()
        }
        MotivoDestino::ParcialExistente { ruta } => format!(
            "a partial copy from an interrupted move is left in “{}”: delete it by hand and try again",
            ruta.display()
        ),
        MotivoDestino::OrigenNoValido { ruta } => format!(
            "the current folder “{}” is not a real directory (a symbolic link?)",
            ruta.display()
        ),
        MotivoDestino::Io { detalle } => {
            format!("the destination could not be checked: {}", detalle.en)
        }
    }
}

pub(crate) fn problema_copia(problema: &ProblemaCopia) -> String {
    match problema {
        ProblemaCopia::TipoEspecial { ruta } => format!(
            "“{}” is a special file (FIFO, device or socket) and cannot be copied",
            ruta.display()
        ),
        ProblemaCopia::Falta { ruta } => format!("“{}” is missing from the copy", ruta.display()),
        ProblemaCopia::Sobra { ruta } => {
            format!("“{}” is extra in the copy", ruta.display())
        }
        ProblemaCopia::Distinta { ruta } => format!(
            "“{}” differs from the original (type, size, mode or link)",
            ruta.display()
        ),
        ProblemaCopia::HashDistinto { ruta } => format!(
            "the SHA-256 of “{}” does not match the original’s",
            ruta.display()
        ),
        ProblemaCopia::Fsck { repo, detalle } => {
            format!("“git fsck” fails in “{}”: {}", repo.display(), detalle.en)
        }
    }
}

pub(crate) fn progreso(progreso: &ProgresoMover) -> String {
    match progreso {
        ProgresoMover::Validando => "Checking the destination folder".to_string(),
        ProgresoMover::ParandoGitea => "Stopping Gitea".to_string(),
        ProgresoMover::Renombrando => "Moving the folder".to_string(),
        ProgresoMover::Copiando {
            ficheros,
            ficheros_total,
            ..
        } => format!("Copying: {ficheros} of {ficheros_total} files"),
        ProgresoMover::Verificando { hechos, total } => {
            format!("Verifying the copy: {hechos} of {total} repositories")
        }
        ProgresoMover::Repuntando => "Updating the configuration".to_string(),
        ProgresoMover::ArrancandoGitea => "Starting Gitea in the new folder".to_string(),
        ProgresoMover::Confirmando => "Confirming the change".to_string(),
        ProgresoMover::BorrandoOrigen => "Deleting the old folder".to_string(),
        ProgresoMover::Terminado => "Move finished".to_string(),
    }
}

pub(crate) fn aviso(aviso: &AvisoMover) -> String {
    match aviso {
        AvisoMover::OrigenSinBorrar { ruta, motivo } => format!(
            "The account is already in the new folder, but the old one “{}” could not be deleted: {}. Delete it by hand.",
            ruta.display(),
            motivo.en
        ),
        AvisoMover::AuditoriaSinRegistrar { motivo } => format!(
            "The account was moved, but it could not be recorded in the audit log: {}",
            motivo.en
        ),
    }
}
