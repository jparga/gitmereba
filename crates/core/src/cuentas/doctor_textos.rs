//! Textos tipados de `doctor`: cada frase es una variante con sus parámetros, y el
//! catálogo de cada idioma (`idioma::{es,en}::doctor`) la convierte en texto.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::idioma::{Idioma, Localizable};

/// Parte de una cuenta que revisa una comprobación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParteCuenta {
    Carpeta,
    AppIni,
    BinarioGitea,
    Gitea,
    Secretos,
    Snapshots,
    Temporizador,
}

/// Nombre de una comprobación de `doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NombreComprobacion {
    Git,
    Gpgv,
    Llavero,
    DirectorioDatos,
    Auditoria,
    AislamientoSystemd,
    Cortafuegos,
    Cuentas,
    Idioma,
    Cuenta { login: String, parte: ParteCuenta },
}

/// Motivo por el que `app.ini` de una cuenta no es correcto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotivoAppIni {
    Permisos { modo: u32 },
    SinHttpAddrLocal,
    SinHttps,
    SinCertificado,
    ClaveSinPermisos,
}

/// Texto de un error que `doctor` muestra dentro de un mensaje. El informe se construye una
/// sola vez y se muestra después en el idioma elegido, así que el error se guarda ya
/// localizado en ambos idiomas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextoExterno {
    pub es: String,
    pub en: String,
}

impl TextoExterno {
    /// Error de `core`: su texto en cada idioma.
    pub fn de(error: &impl Localizable) -> Self {
        Self {
            es: error.localizar(Idioma::Es),
            en: error.localizar(Idioma::En),
        }
    }

    /// Error de una librería ajena (io, sistema) que no se puede traducir: el mismo texto
    /// en ambos idiomas.
    pub fn literal(texto: impl Into<String>) -> Self {
        let texto = texto.into();
        Self {
            es: texto.clone(),
            en: texto,
        }
    }
}

/// Mensajes y consejos de `doctor`. Los parámetros que vienen del exterior (errores,
/// rutas) se muestran tal cual; ninguno lleva secretos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextoDoctor {
    // --- git y gpgv ---
    GitVersion {
        version: String,
    },
    GitNoDisponible {
        error: TextoExterno,
    },
    ConsejoInstalarGit,
    GpgvDisponible {
        ruta: PathBuf,
    },
    GpgvNoEncontrado {
        ruta: PathBuf,
    },
    ConsejoInstalarGnupg,
    // --- llavero ---
    LlaveroErrorInterno {
        error: TextoExterno,
    },
    ConsejoRepetirComprobacion,
    LlaveroAccesible,
    LlaveroNoAccesible {
        error: TextoExterno,
    },
    ConsejoSecretService,
    // --- directorio de datos y permisos ---
    DirectorioDatosNoExiste,
    ConsejoSeCreaConCuentaAdd,
    PermisosCorrectos0700,
    PermisosIncorrectos0700 {
        modo: u32,
    },
    ConsejoChmod700 {
        ruta: PathBuf,
    },
    PermisosNoLegibles {
        error: TextoExterno,
    },
    ConsejoDirectorioAccesible,
    // --- aislamiento de systemd y cortafuegos ---
    AislamientoAviso,
    ConsejoAislamiento,
    AislamientoOk,
    UfwInstalado,
    ConsejoActivarUfw,
    UfwNoEncontrado,
    ConsejoInstalarUfw,
    // --- auditoría e índice de cuentas ---
    AuditoriaIntegra,
    AuditoriaRota {
        id: i64,
    },
    ConsejoAuditoriaManipulada,
    AuditoriaNoVerificable {
        error: TextoExterno,
    },
    ConsejoAccesoAlmacen,
    CuentasIndiceIlegible {
        error: TextoExterno,
    },
    ConsejoPermisosIndice,
    // --- carpeta y app.ini de la cuenta ---
    CarpetaNoExiste,
    ConsejoRepetirAltaORestaurar,
    CarpetaExisteCon0700,
    ErrorSistema {
        error: TextoExterno,
    },
    ConsejoRevisarPermisosAMano,
    AppIniNoExiste,
    ConsejoRepetirAltaProvision,
    ConsejoRevisarFicheroLegible,
    ConsejoFicheroLegible,
    AppIniSinLanCorrecto,
    AppIniMotivos {
        motivos: Vec<MotivoAppIni>,
    },
    ConsejoAppIniSinLan,
    AppIniExpuestoLan {
        host: String,
    },
    ConsejoCortafuegosLan,
    ConsejoRegenerarLan,
    // --- binario y servicio de Gitea ---
    BinarioNoEncontrado {
        ruta: PathBuf,
    },
    ConsejoReinstalarBinario,
    BinarioHashCorrecto {
        version: String,
    },
    BinarioHashDistinto,
    ConsejoBorrarBinario,
    BinarioHashError {
        error: TextoExterno,
    },
    ConsejoRevisarUrl,
    GiteaResponde,
    GiteaNoResponde,
    ConsejoArrancarGitea,
    ConsejoRevisarServicioGitea,
    // --- secretos y snapshots ---
    ConsejoRevisarLlavero,
    SecretosPresentes,
    SecretosFaltan {
        faltan: Vec<String>,
    },
    ConsejoRegenerarSecretos,
    SnapshotsResumen {
        total: usize,
        protegidas: usize,
    },
    ConsejoCapturasProtegidas,
    SnapshotsError {
        error: TextoExterno,
    },
    ConsejoPermisosSnapshots,
    // --- temporizador ---
    TemporizadorNoInstalado,
    ConsejoAbrirVentanaInstala,
    TemporizadorSinExecStart {
        ruta: PathBuf,
    },
    ConsejoAbrirVentanaReescribe,
    TemporizadorSincroniza {
        ejecutable: String,
    },
    TemporizadorEjecutablePerdido {
        ejecutable: String,
    },
    ConsejoActualizarTemporizador,
    // --- idioma ---
    IdiomaPreferencia {
        idioma: Idioma,
        ruta: PathBuf,
    },
    IdiomaVariable {
        idioma: Idioma,
        variable: String,
        valor: String,
    },
    IdiomaPorDefecto {
        idioma: Idioma,
    },
    ConsejoFijarIdioma,
    IdiomaPreferenciaNoValida {
        idioma: Idioma,
        ruta: PathBuf,
    },
    ConsejoCorregirPreferencias,
}
