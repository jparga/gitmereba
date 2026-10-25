//! Tipos de `mover_carpeta` que acaban en un texto para el usuario: cada frase es una
//! variante con sus parámetros y el catálogo de cada idioma (`idioma::{es,en}::mover`) la
//! convierte en texto. Ninguno lleva secretos.

use std::path::PathBuf;

use serde::Serialize;

use crate::idioma::TextoExterno;
use crate::modelo::Nombre;

/// Por qué una carpeta de destino (o la de origen) no sirve para mover la cuenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoDestino {
    /// La ruta no es absoluta.
    NoAbsoluta,
    /// La ruta contiene `..`.
    ConPuntoPunto,
    /// La ruta no es texto UTF-8 (acabaría en `app.ini` y en unidades systemd).
    NoUtf8,
    /// La ruta contiene un carácter de control o uno que rompería `app.ini` o una unidad
    /// de systemd (`%`, `$`, `"`, `\`, `#`, `;`).
    CaracterNoPermitido { caracter: char },
    /// Es la raíz, el HOME o un ancestro del HOME.
    Protegida,
    /// Está dentro del directorio de datos de la app.
    DentroDeDatosDeLaApp,
    /// La carpeta que contendría el destino no existe o no es un directorio.
    SinPadre,
    /// La carpeta que contendría el destino no es del usuario que ejecuta la app.
    PadreAjeno,
    /// El destino existe y es un enlace simbólico.
    EsEnlaceSimbolico,
    /// El destino existe y no es un directorio.
    NoEsDirectorio,
    /// El destino existe y no está vacío.
    NoVacio,
    /// El destino está dentro de la carpeta de otra cuenta (o es esa carpeta).
    DentroDeOtraCuenta { login: Nombre, carpeta: PathBuf },
    /// El destino contiene la carpeta de otra cuenta.
    ContieneOtraCuenta { login: Nombre, carpeta: PathBuf },
    /// El destino es la carpeta actual de la cuenta o está dentro de ella.
    DentroDeLaPropia,
    /// El destino contiene la carpeta actual de la cuenta.
    ContieneLaPropia,
    /// Quedó una copia parcial de un movimiento anterior interrumpido.
    ParcialExistente { ruta: PathBuf },
    /// La carpeta actual de la cuenta no se puede mover (no es un directorio real).
    OrigenNoValido { ruta: PathBuf },
    /// No se pudo comprobar el destino.
    Io { detalle: TextoExterno },
}

/// Por qué una copia no se da por buena (o no se puede hacer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemaCopia {
    /// El origen contiene un fichero especial (FIFO, dispositivo, socket).
    TipoEspecial { ruta: PathBuf },
    /// Falta en la copia algo que está en el origen.
    Falta { ruta: PathBuf },
    /// Hay en la copia algo que no está en el origen.
    Sobra { ruta: PathBuf },
    /// Tipo, tamaño, modo o destino de enlace distintos.
    Distinta { ruta: PathBuf },
    /// El SHA-256 de un fichero crítico no coincide.
    HashDistinto { ruta: PathBuf },
    /// `git fsck` falla en un repositorio de la copia.
    Fsck {
        repo: PathBuf,
        detalle: TextoExterno,
    },
}

/// Cómo se movió la carpeta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoMover {
    /// Mismo sistema de ficheros: un `rename`.
    Renombrado,
    /// Otro sistema de ficheros: copia verificada y borrado del origen.
    Copia,
}

impl ModoMover {
    /// Identificador estable para la auditoría (no es un texto para el usuario).
    pub fn identificador(self) -> &'static str {
        match self {
            ModoMover::Renombrado => "renombrado",
            ModoMover::Copia => "copia",
        }
    }
}

/// Avance de [`super::mover_carpeta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "fase", rename_all = "snake_case")]
pub enum ProgresoMover {
    Validando,
    ParandoGitea,
    Renombrando,
    Copiando {
        ficheros: u64,
        ficheros_total: u64,
        bytes: u64,
        bytes_total: u64,
    },
    Verificando {
        hechos: u64,
        total: u64,
    },
    Repuntando,
    ArrancandoGitea,
    Confirmando,
    BorrandoOrigen,
    Terminado,
}

/// Cosas que no impiden dar el movimiento por hecho pero el usuario debe saber.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvisoMover {
    /// La cuenta ya está en la carpeta nueva, pero no se pudo borrar la antigua.
    OrigenSinBorrar { ruta: PathBuf, motivo: TextoExterno },
    /// El movimiento se hizo pero no se pudo anotar en la auditoría.
    AuditoriaSinRegistrar { motivo: TextoExterno },
}

/// Resultado de un movimiento confirmado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoMover {
    pub origen: PathBuf,
    pub destino: PathBuf,
    pub modo: ModoMover,
    pub avisos: Vec<AvisoMover>,
}
