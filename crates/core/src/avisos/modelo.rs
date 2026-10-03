//! Tipos públicos del módulo `avisos`: qué se decide notificar y con qué se decide.

use crate::cuentas::InformeProteccion;
use crate::idioma::{Idioma, Localizable, TextoExterno};
use crate::modelo::Nombre;
use crate::sync::InformeSync;
use crate::verificacion::InformeVerificacion;

/// Urgencia de una notificación de escritorio (mapea a la de `notify-rust`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgencia {
    Baja,
    Normal,
    Critica,
}

/// Una notificación ya decidida por [`super::decidir`], lista para enviar. El título y
/// el cuerpo ya están saneados (sin caracteres de control ni marcado tipo HTML) y nunca
/// llevan secretos ni URLs con credenciales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notificacion {
    pub titulo: TextoAviso,
    pub cuerpo: TextoAviso,
    pub urgencia: Urgencia,
    /// Identificador estable de qué representa esta notificación (p. ej. para que un
    /// [`super::Notificador`] pueda sustituir una notificación anterior con la misma
    /// clave en vez de apilarlas). No es un mecanismo de deduplicación: la decisión de
    /// si avisar o no ya la ha tomado [`super::decidir`].
    pub clave_dedupe: String,
}

/// Una [`Notificacion`] con sus textos ya traducidos a un idioma: es lo que recibe un
/// [`super::Notificador`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificacionLocalizada {
    pub titulo: String,
    pub cuerpo: String,
    pub urgencia: Urgencia,
    pub clave_dedupe: String,
}

impl Notificacion {
    /// Traduce título y cuerpo a `idioma`.
    pub fn localizar(&self, idioma: Idioma) -> NotificacionLocalizada {
        NotificacionLocalizada {
            titulo: self.titulo.localizar(idioma),
            cuerpo: self.cuerpo.localizar(idioma),
            urgencia: self.urgencia,
            clave_dedupe: self.clave_dedupe.clone(),
        }
    }
}

/// Un cambio destructivo dentro del cuerpo de [`TextoAviso::CambioDestructivo`]. Los
/// nombres de rama y de tag ya vienen saneados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CambioAviso {
    HistoriaReescrita { rama: String },
    RamaBorrada { rama: String },
    TagBorrado { tag: String },
    TagMovido { tag: String },
}

/// Texto de una notificación, sin traducir: una variante por frase. Los parámetros
/// externos (login, repo, detalle, ramas) ya llegan saneados por [`super::decidir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextoAviso {
    // --- Títulos ---
    TituloFalloSincronizacion,
    TituloVariosFallos,
    TituloRepositorioRecuperado,
    TituloVariosRecuperados,
    TituloHuerfanos,
    TituloHistoriaReescrita,
    TituloGiteaParado,
    TituloTokenCaducado,
    TituloTokenCaducaPronto,
    // --- Cuerpos ---
    FalloRepositorio {
        login: String,
        repo: String,
        detalle: Option<TextoExterno>,
    },
    FallosAgregados {
        login: String,
        n: usize,
    },
    RepositorioRecuperado {
        login: String,
        repo: String,
    },
    RecuperadosAgregados {
        login: String,
        n: usize,
    },
    HuerfanosListaVacia {
        login: String,
    },
    HuerfanosCandidatos {
        login: String,
        candidatos: usize,
        total_mirrors: usize,
    },
    CambioDestructivo {
        login: String,
        repo: String,
        cambios: Vec<CambioAviso>,
    },
    GiteaParado {
        login: String,
    },
    TokenCaducado {
        login: String,
    },
    TokenCaducaPronto {
        login: String,
        dias: u32,
    },
}

/// De dónde viene el fallo de un repo, para llevar el histórico de cada fuente por
/// separado (un repo puede fallar al sincronizar y, aparte, fallar la verificación).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum TipoFallo {
    /// `ResultadoAccion::Error` al aplicar una acción del plan de sincronización.
    Sincronizacion,
    /// `EstadoRepo::Fallo` en un diagnóstico de [`crate::verificacion::verificar_cuenta`].
    Verificacion,
}

/// Todo lo que necesita [`super::decidir`] de una pasada: el resultado de la
/// sincronización, el de la verificación (si se ha ejecutado) o el hecho de que Gitea no
/// respondía y no se ha llegado a hacer ninguna de las dos cosas.
///
/// Invariante esperado por quien construye este valor (no lo comprueba `decidir`, que es
/// puro y no falla): si `gitea_parado` es `true`, `sync` y `verificacion` deben ser
/// `None` (no se ha llegado a tocar Gitea); si es `false`, normalmente `sync` es `Some`.
#[derive(Debug, Clone)]
pub struct EntradaAvisos {
    pub login: Nombre,
    /// `None` si esta pasada no ha llegado a sincronizar.
    pub sync: Option<InformeSync>,
    /// `None` si no se ha ejecutado una verificación en esta pasada (p. ej. porque
    /// `sync` ha fallado antes, o porque el llamador no la pide siempre).
    pub verificacion: Option<InformeVerificacion>,
    /// El Gitea de la cuenta no respondía: no ha habido ni `sync` ni `verificacion`.
    pub gitea_parado: bool,
    /// Informe de protección de snapshots de esta pasada, si se ha llegado a ejecutar
    /// `None` si no se ha ejecutado (p. ej. Gitea parado, o un
    /// fallo al listar Gitea tras sincronizar).
    pub proteccion: Option<InformeProteccion>,
}
