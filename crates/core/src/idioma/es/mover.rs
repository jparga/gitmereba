//! Textos de `mover_carpeta`, en español.

use crate::cuentas::{AvisoMover, MotivoDestino, ProblemaCopia, ProgresoMover};

pub(crate) fn motivo_destino(motivo: &MotivoDestino) -> String {
    match motivo {
        MotivoDestino::NoAbsoluta => "la carpeta debe ser una ruta absoluta".to_string(),
        MotivoDestino::ConPuntoPunto => "la ruta no puede contener «..»".to_string(),
        MotivoDestino::NoUtf8 => "la ruta no es texto UTF-8 válido".to_string(),
        MotivoDestino::CaracterNoPermitido { caracter } => format!(
            "la ruta contiene un carácter no permitido ({})",
            caracter.escape_debug()
        ),
        MotivoDestino::Protegida => {
            "no puede ser la raíz, el HOME ni un ancestro del HOME".to_string()
        }
        MotivoDestino::DentroDeDatosDeLaApp => {
            "no puede estar dentro del directorio de datos de la app".to_string()
        }
        MotivoDestino::SinPadre => {
            "la carpeta que contendría el destino no existe o no es un directorio".to_string()
        }
        MotivoDestino::PadreAjeno => {
            "la carpeta que contendría el destino no pertenece a tu usuario".to_string()
        }
        MotivoDestino::EsEnlaceSimbolico => "el destino es un enlace simbólico".to_string(),
        MotivoDestino::NoEsDirectorio => "el destino existe y no es un directorio".to_string(),
        MotivoDestino::NoVacio => "el destino debe estar vacío o no existir".to_string(),
        MotivoDestino::DentroDeOtraCuenta { login, carpeta } => format!(
            "el destino está dentro de la carpeta de la cuenta «{login}» ({})",
            carpeta.display()
        ),
        MotivoDestino::ContieneOtraCuenta { login, carpeta } => format!(
            "el destino contiene la carpeta de la cuenta «{login}» ({})",
            carpeta.display()
        ),
        MotivoDestino::DentroDeLaPropia => {
            "el destino es la carpeta actual de la cuenta o está dentro de ella".to_string()
        }
        MotivoDestino::ContieneLaPropia => {
            "el destino contiene la carpeta actual de la cuenta".to_string()
        }
        MotivoDestino::ParcialExistente { ruta } => format!(
            "queda una copia parcial de un movimiento interrumpido en «{}»: bórrala a mano y vuelve a intentarlo",
            ruta.display()
        ),
        MotivoDestino::OrigenNoValido { ruta } => format!(
            "la carpeta actual «{}» no es un directorio real (¿un enlace simbólico?)",
            ruta.display()
        ),
        MotivoDestino::Io { detalle } => format!("no se pudo comprobar el destino: {}", detalle.es),
    }
}

pub(crate) fn problema_copia(problema: &ProblemaCopia) -> String {
    match problema {
        ProblemaCopia::TipoEspecial { ruta } => format!(
            "«{}» es un fichero especial (FIFO, dispositivo o socket) y no se puede copiar",
            ruta.display()
        ),
        ProblemaCopia::Falta { ruta } => format!("falta «{}» en la copia", ruta.display()),
        ProblemaCopia::Sobra { ruta } => format!("«{}» sobra en la copia", ruta.display()),
        ProblemaCopia::Distinta { ruta } => format!(
            "«{}» difiere del original (tipo, tamaño, modo o enlace)",
            ruta.display()
        ),
        ProblemaCopia::HashDistinto { ruta } => format!(
            "el SHA-256 de «{}» no coincide con el del original",
            ruta.display()
        ),
        ProblemaCopia::Fsck { repo, detalle } => {
            format!("«git fsck» falla en «{}»: {}", repo.display(), detalle.es)
        }
    }
}

pub(crate) fn progreso(progreso: &ProgresoMover) -> String {
    match progreso {
        ProgresoMover::Validando => "Comprobando la carpeta de destino".to_string(),
        ProgresoMover::ParandoGitea => "Parando Gitea".to_string(),
        ProgresoMover::Renombrando => "Moviendo la carpeta".to_string(),
        ProgresoMover::Copiando {
            ficheros,
            ficheros_total,
            ..
        } => format!("Copiando: {ficheros} de {ficheros_total} ficheros"),
        ProgresoMover::Verificando { hechos, total } => {
            format!("Verificando la copia: {hechos} de {total} repositorios")
        }
        ProgresoMover::Repuntando => "Actualizando la configuración".to_string(),
        ProgresoMover::ArrancandoGitea => "Arrancando Gitea en la carpeta nueva".to_string(),
        ProgresoMover::Confirmando => "Confirmando el cambio".to_string(),
        ProgresoMover::BorrandoOrigen => "Borrando la carpeta antigua".to_string(),
        ProgresoMover::Terminado => "Movimiento terminado".to_string(),
    }
}

pub(crate) fn aviso(aviso: &AvisoMover) -> String {
    match aviso {
        AvisoMover::OrigenSinBorrar { ruta, motivo } => format!(
            "La cuenta ya está en la carpeta nueva, pero no se pudo borrar la antigua «{}»: {}. Bórrala a mano.",
            ruta.display(),
            motivo.es
        ),
        AvisoMover::AuditoriaSinRegistrar { motivo } => format!(
            "La cuenta se ha movido, pero no se pudo anotar en la auditoría: {}",
            motivo.es
        ),
    }
}
