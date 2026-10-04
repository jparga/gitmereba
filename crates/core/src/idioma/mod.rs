//! Idioma de la interfaz: resolución a partir de la preferencia y del entorno.

mod en;
mod errores;
mod es;
mod preferencias;

use serde::{Deserialize, Serialize};

use crate::avisos::TextoAviso;
use crate::config::Rutas;
use crate::contingencia::InformeReconciliacion;
use crate::cuentas::{NombreComprobacion, PasoAlta, TextoDoctor};
use crate::sync::InformeSync;
use crate::verificacion::Aviso;

pub use preferencias::{LecturaPreferencia, Preferencia, guardar_preferencia, leer_preferencia};

/// Idioma en el que se muestran los textos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Idioma {
    Es,
    En,
}

impl Idioma {
    /// Código de dos letras (`es`, `en`).
    pub fn codigo(self) -> &'static str {
        match self {
            Idioma::Es => "es",
            Idioma::En => "en",
        }
    }
}

/// Texto de un error que se muestra dentro de otro mensaje (`doctor`, avisos). El informe se construye una
/// sola vez y se muestra después en el idioma elegido, así que el error se guarda ya
/// localizado en ambos idiomas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for TextoExterno {
    /// Acepta también una cadena suelta (formato anterior a los idiomas, que se guardó
    /// en español): se lee como el mismo texto en ambos idiomas.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Formato {
            Par { es: String, en: String },
            Cadena(String),
        }
        Ok(match Formato::deserialize(deserializer)? {
            Formato::Par { es, en } => Self { es, en },
            Formato::Cadena(texto) => Self::literal(texto),
        })
    }
}

/// De dónde sale el idioma resuelto (lo muestra `doctor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrigenIdioma {
    /// La preferencia guardada es explícita (`es` o `en`).
    Preferencia,
    /// La preferencia es `auto` y el idioma sale de una variable de entorno.
    Variable { nombre: &'static str, valor: String },
    /// Sin preferencia ni variables útiles: inglés.
    PorDefecto,
}

/// Variables de entorno de localización, de mayor a menor precedencia.
const VARIABLES_LOCALE: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

/// Primera variable de localización con valor no vacío.
fn variable_de_locale(entorno: &dyn Fn(&str) -> Option<String>) -> Option<(&'static str, String)> {
    VARIABLES_LOCALE.iter().find_map(|&nombre| {
        entorno(nombre)
            .filter(|v| !v.is_empty())
            .map(|v| (nombre, v))
    })
}

/// Idioma efectivo: la preferencia explícita manda; con `auto` se mira el entorno
/// y todo lo que no sea español (incluidos `C` y `POSIX`) es inglés.
pub fn resolver(pref: Preferencia, entorno: &dyn Fn(&str) -> Option<String>) -> Idioma {
    match pref {
        Preferencia::Es => Idioma::Es,
        Preferencia::En => Idioma::En,
        Preferencia::Auto => match variable_de_locale(entorno) {
            Some((_, valor)) if valor.split(['_', '.', '@']).next() == Some("es") => Idioma::Es,
            _ => Idioma::En,
        },
    }
}

/// Origen del idioma que devolvería `resolver` con los mismos argumentos.
pub fn origen(pref: Preferencia, entorno: &dyn Fn(&str) -> Option<String>) -> OrigenIdioma {
    match pref {
        Preferencia::Es | Preferencia::En => OrigenIdioma::Preferencia,
        Preferencia::Auto => match variable_de_locale(entorno) {
            Some((nombre, valor)) => OrigenIdioma::Variable { nombre, valor },
            None => OrigenIdioma::PorDefecto,
        },
    }
}

/// Un texto que se puede mostrar en cualquiera de los idiomas soportados.
pub trait Localizable {
    /// El texto en `idioma`.
    fn localizar(&self, idioma: Idioma) -> String;
}

impl Localizable for TextoAviso {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::avisos::texto(self),
            Idioma::En => en::avisos::texto(self),
        }
    }
}

impl Localizable for Aviso {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::verificacion::aviso(self),
            Idioma::En => en::verificacion::aviso(self),
        }
    }
}

impl Localizable for TextoDoctor {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::doctor::texto(self),
            Idioma::En => en::doctor::texto(self),
        }
    }
}

impl Localizable for NombreComprobacion {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::doctor::nombre(self),
            Idioma::En => en::doctor::nombre(self),
        }
    }
}

impl Localizable for InformeSync {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::sync::resumen(self),
            Idioma::En => en::sync::resumen(self),
        }
    }
}

impl Localizable for InformeReconciliacion {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::reconciliacion::resumen(self),
            Idioma::En => en::reconciliacion::resumen(self),
        }
    }
}

impl Localizable for PasoAlta {
    fn localizar(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es::progreso::descripcion(*self).to_string(),
            Idioma::En => en::progreso::descripcion(*self).to_string(),
        }
    }
}

/// Descripción en español de un paso del alta, para `PasoAlta::descripcion`.
pub(crate) fn descripcion_paso_es(paso: PasoAlta) -> &'static str {
    es::progreso::descripcion(paso)
}

/// Idioma actual de la app: preferencia guardada y entorno real del proceso.
pub fn idioma_actual(rutas: &Rutas) -> Idioma {
    resolver(leer_preferencia(rutas).preferencia(), &|nombre| {
        std::env::var(nombre).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avisos::CambioAviso;
    use crate::cuentas::{
        MotivoAppIni, NombreComprobacion, ParteCuenta, TextoDoctor, TextoExterno,
    };
    use std::path::PathBuf;

    fn env<'a>(pares: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pares
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn preferencia_explicita_manda() {
        assert_eq!(
            resolver(Preferencia::En, &env(&[("LANG", "es_ES.UTF-8")])),
            Idioma::En
        );
    }

    #[test]
    fn auto_con_lang_es() {
        for v in ["es_ES.UTF-8", "es", "es@euro", "es_MX"] {
            assert_eq!(
                resolver(Preferencia::Auto, &env(&[("LANG", v)])),
                Idioma::Es,
                "{v}"
            );
        }
    }

    #[test]
    fn auto_otro_idioma_o_c_es_ingles() {
        for v in ["en_US.UTF-8", "fr_FR", "C", "POSIX", "C.UTF-8", ""] {
            assert_eq!(
                resolver(Preferencia::Auto, &env(&[("LANG", v)])),
                Idioma::En,
                "{v}"
            );
        }
        assert_eq!(resolver(Preferencia::Auto, &env(&[])), Idioma::En);
    }

    #[test]
    fn precedencia_lc_all_lc_messages_lang() {
        let e = env(&[("LANG", "en_US"), ("LC_MESSAGES", "es_ES"), ("LC_ALL", "")]);
        assert_eq!(resolver(Preferencia::Auto, &e), Idioma::Es); // LC_ALL vacío se salta
    }

    #[test]
    fn origen_indica_de_donde_sale_el_idioma() {
        assert_eq!(
            origen(Preferencia::Es, &env(&[("LANG", "en_US")])),
            OrigenIdioma::Preferencia
        );
        assert_eq!(
            origen(
                Preferencia::Auto,
                &env(&[("LC_ALL", ""), ("LANG", "es_ES")])
            ),
            OrigenIdioma::Variable {
                nombre: "LANG",
                valor: "es_ES".to_string()
            }
        );
        assert_eq!(
            origen(Preferencia::Auto, &env(&[("LANG", "")])),
            OrigenIdioma::PorDefecto
        );
    }

    fn s(t: &str) -> String {
        t.to_string()
    }

    fn casos() -> Vec<(TextoAviso, &'static str, &'static str)> {
        vec![
            (
                TextoAviso::TituloFalloSincronizacion,
                "Fallo de sincronización",
                "Sync failure",
            ),
            (
                TextoAviso::TituloVariosFallos,
                "Varios repositorios con fallos",
                "Several repositories with failures",
            ),
            (
                TextoAviso::TituloRepositorioRecuperado,
                "Repositorio recuperado",
                "Repository recovered",
            ),
            (
                TextoAviso::TituloVariosRecuperados,
                "Varios repositorios recuperados",
                "Several repositories recovered",
            ),
            (
                TextoAviso::TituloHuerfanos,
                "Demasiados huérfanos",
                "Too many orphans",
            ),
            (
                TextoAviso::TituloHistoriaReescrita,
                "Historia reescrita",
                "History rewritten",
            ),
            (
                TextoAviso::TituloGiteaParado,
                "Gitea no responde",
                "Gitea is not responding",
            ),
            (
                TextoAviso::TituloTokenCaducado,
                "Token de GitHub caducado",
                "GitHub token expired",
            ),
            (
                TextoAviso::TituloTokenCaducaPronto,
                "Token de GitHub a punto de caducar",
                "GitHub token about to expire",
            ),
            (
                TextoAviso::FalloRepositorio {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                    detalle: None,
                },
                "El repositorio «jparga/r1» tiene un fallo en «jparga».",
                "Repository “jparga/r1” has a failure in “jparga”.",
            ),
            (
                TextoAviso::FalloRepositorio {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                    detalle: Some(TextoExterno {
                        es: s("sin red"),
                        en: s("no network"),
                    }),
                },
                "El repositorio «jparga/r1» tiene un fallo en «jparga». Detalle: sin red.",
                "Repository “jparga/r1” has a failure in “jparga”. Detail: no network.",
            ),
            (
                TextoAviso::FallosAgregados {
                    login: s("jparga"),
                    n: 4,
                },
                "4 repositorios con fallos en «jparga».",
                "4 repositories with failures in “jparga”.",
            ),
            (
                TextoAviso::RepositorioRecuperado {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                },
                "El repositorio «jparga/r1» ha vuelto a estar bien en «jparga».",
                "Repository “jparga/r1” is back to normal in “jparga”.",
            ),
            (
                TextoAviso::RecuperadosAgregados {
                    login: s("jparga"),
                    n: 5,
                },
                "5 repositorios han vuelto a estar bien en «jparga».",
                "5 repositories are back to normal in “jparga”.",
            ),
            (
                TextoAviso::HuerfanosListaVacia { login: s("jparga") },
                "GitHub devolvió una lista vacía de repositorios en «jparga»; no se ha \
                 marcado ningún huérfano por precaución.",
                "GitHub returned an empty list of repositories in “jparga”; no orphan \
                 has been marked, as a precaution.",
            ),
            (
                TextoAviso::HuerfanosCandidatos {
                    login: s("jparga"),
                    candidatos: 5,
                    total_mirrors: 8,
                },
                "5 de 8 repositorios se marcarían huérfanos en «jparga»; no se ha \
                 aplicado por precaución.",
                "5 of 8 repositories would be marked as orphans in “jparga”; this has \
                 not been applied, as a precaution.",
            ),
            (
                TextoAviso::CambioDestructivo {
                    login: s("jparga"),
                    repo: s("jparga/r1"),
                    cambios: vec![
                        CambioAviso::HistoriaReescrita { rama: s("main") },
                        CambioAviso::RamaBorrada { rama: s("dev") },
                        CambioAviso::TagBorrado { tag: s("v1") },
                        CambioAviso::TagMovido { tag: s("v2") },
                    ],
                },
                "Historia reescrita (rama main); Rama dev borrada; Tag v1 borrado; Tag v2 \
                 movido en «jparga/r1» (jparga). La copia anterior está protegida en los \
                 snapshots.",
                "History rewritten (branch main); Branch dev deleted; Tag v1 deleted; Tag v2 \
                 moved in “jparga/r1” (jparga). The previous copy is protected in the \
                 snapshots.",
            ),
            (
                TextoAviso::GiteaParado { login: s("jparga") },
                "El Gitea de «jparga» no responde; revisa «gitmereba doctor».",
                "The Gitea of “jparga” is not responding; check “gitmereba doctor”.",
            ),
            (
                TextoAviso::TokenCaducado { login: s("jparga") },
                "El token de GitHub de «jparga» ha caducado; genera uno nuevo.",
                "The GitHub token of “jparga” has expired; generate a new one.",
            ),
            (
                TextoAviso::TokenCaducaPronto {
                    login: s("jparga"),
                    dias: 3,
                },
                "El token de GitHub de «jparga» caduca en 3 días.",
                "The GitHub token of “jparga” expires in 3 days.",
            ),
            (
                TextoAviso::TokenCaducaPronto {
                    login: s("jparga"),
                    dias: 1,
                },
                "El token de GitHub de «jparga» caduca en 1 día.",
                "The GitHub token of “jparga” expires in 1 day.",
            ),
        ]
    }

    #[test]
    fn cada_frase_de_avisos_se_renderiza_en_espanol_y_en_ingles() {
        for (texto, es, en) in casos() {
            assert_eq!(texto.localizar(Idioma::Es), es, "{texto:?}");
            assert_eq!(texto.localizar(Idioma::En), en, "{texto:?}");
        }
    }

    #[test]
    fn un_error_de_core_en_doctor_sale_en_el_idioma_pedido() {
        use crate::modelo::ErrorNombre;
        let texto = TextoDoctor::LlaveroNoAccesible {
            error: TextoExterno::de(&ErrorNombre::Vacio),
        };
        let en = texto.localizar(Idioma::En);
        assert_eq!(en, "the keyring is not accessible: the name is empty");
        assert!(!en.contains("vacío"), "{en}");
        let es = texto.localizar(Idioma::Es);
        assert_eq!(
            es,
            format!(
                "el llavero no está accesible: {}",
                ErrorNombre::Vacio.localizar(Idioma::Es)
            )
        );
    }

    fn caso_doctor(
        texto: TextoDoctor,
        es: &'static str,
        en: &'static str,
    ) -> (TextoDoctor, &'static str, &'static str) {
        (texto, es, en)
    }

    fn ruta(r: &str) -> PathBuf {
        PathBuf::from(r)
    }

    fn casos_doctor() -> Vec<(TextoDoctor, &'static str, &'static str)> {
        use TextoDoctor as T;
        vec![
            caso_doctor(
                T::GitVersion {
                    version: s("git version 2.43.0"),
                },
                "git version 2.43.0",
                "git version 2.43.0",
            ),
            caso_doctor(
                T::GitNoDisponible {
                    error: TextoExterno::literal("boom"),
                },
                "git no está disponible: boom",
                "git is not available: boom",
            ),
            caso_doctor(
                T::ConsejoInstalarGit,
                "instala git y asegúrate de que está en el PATH",
                "install git and make sure it is on the PATH",
            ),
            caso_doctor(
                T::GpgvDisponible {
                    ruta: ruta("/usr/bin/gpgv"),
                },
                "disponible en /usr/bin/gpgv",
                "available at /usr/bin/gpgv",
            ),
            caso_doctor(
                T::GpgvNoEncontrado {
                    ruta: ruta("/usr/bin/gpgv"),
                },
                "no se encuentra /usr/bin/gpgv",
                "/usr/bin/gpgv not found",
            ),
            caso_doctor(
                T::ConsejoInstalarGnupg,
                "instala el paquete gnupg (necesario para verificar el binario de Gitea)",
                "install the gnupg package (needed to verify the Gitea binary)",
            ),
            caso_doctor(
                T::LlaveroErrorInterno {
                    error: TextoExterno::literal("x"),
                },
                "error interno al comprobar el llavero: x",
                "internal error while checking the keyring: x",
            ),
            caso_doctor(
                T::ConsejoRepetirComprobacion,
                "repite la comprobación; si persiste, informa del error",
                "repeat the check; if it persists, report the error",
            ),
            caso_doctor(T::LlaveroAccesible, "accesible", "accessible"),
            caso_doctor(
                T::LlaveroNoAccesible {
                    error: TextoExterno::literal("x"),
                },
                "el llavero no está accesible: x",
                "the keyring is not accessible: x",
            ),
            caso_doctor(
                T::ConsejoSecretService,
                "comprueba que hay un Secret Service en marcha (GNOME Keyring, KWallet)",
                "check that a Secret Service is running (GNOME Keyring, KWallet)",
            ),
            caso_doctor(
                T::DirectorioDatosNoExiste,
                "todavía no existe (no se ha dado de alta ninguna cuenta)",
                "does not exist yet (no account has been added)",
            ),
            caso_doctor(
                T::ConsejoSeCreaConCuentaAdd,
                "se creará automáticamente con «gitmereba cuenta add»",
                "it will be created automatically by “gitmereba cuenta add”",
            ),
            caso_doctor(
                T::PermisosCorrectos0700,
                "permisos 0700",
                "permissions 0700",
            ),
            caso_doctor(
                T::PermisosIncorrectos0700 { modo: 0o755 },
                "permisos 755, deberían ser 0700",
                "permissions 755, should be 0700",
            ),
            caso_doctor(
                T::ConsejoChmod700 { ruta: ruta("/d") },
                "ejecuta: chmod 700 /d",
                "run: chmod 700 /d",
            ),
            caso_doctor(
                T::PermisosNoLegibles {
                    error: TextoExterno::literal("x"),
                },
                "no se pudo leer sus permisos: x",
                "could not read its permissions: x",
            ),
            caso_doctor(
                T::ConsejoDirectorioAccesible,
                "comprueba que el directorio existe y es accesible",
                "check that the directory exists and is accessible",
            ),
            caso_doctor(
                T::AislamientoAviso,
                "Este sistema impide a los servicios de usuario aislar el sistema de \
                 ficheros: las protecciones de montaje de las unidades no se aplican. \
                 Siguen activas las de llamadas al sistema y red.",
                "This system prevents user services from isolating the file system: the \
                 mount protections of the units are not applied. System call and network \
                 protections remain active.",
            ),
            caso_doctor(
                T::ConsejoAislamiento,
                "las protecciones de montaje (ProtectSystem, ProtectHome, ReadWritePaths, \
                 PrivateTmp) de las unidades de usuario no se aplican en este sistema; las de \
                 llamadas al sistema (seccomp) y red siguen activas",
                "the mount protections (ProtectSystem, ProtectHome, ReadWritePaths, \
                 PrivateTmp) of user units are not applied on this system; the system call \
                 (seccomp) and network ones remain active",
            ),
            caso_doctor(
                T::AislamientoOk,
                "las protecciones de montaje de las unidades de usuario se aplican",
                "the mount protections of user units are applied",
            ),
            caso_doctor(
                T::UfwInstalado,
                "ufw está instalado: revisa que esté activo antes de exponer alguna cuenta a la LAN",
                "ufw is installed: check that it is active before exposing any account to the LAN",
            ),
            caso_doctor(
                T::ConsejoActivarUfw,
                "actívalo con «sudo ufw enable» y, para cada cuenta expuesta, limita el acceso con \
                 «sudo ufw allow from <red>/<prefijo> to any port <puerto> proto tcp»",
                "enable it with “sudo ufw enable” and, for each exposed account, limit access with \
                 “sudo ufw allow from <network>/<prefix> to any port <port> proto tcp”",
            ),
            caso_doctor(
                T::UfwNoEncontrado,
                "no se encontró «ufw» en este sistema",
                "“ufw” was not found on this system",
            ),
            caso_doctor(
                T::ConsejoInstalarUfw,
                "instala ufw (u otro cortafuegos) antes de exponer alguna cuenta a la LAN con \
                 «gitmereba cuenta lan --activar», y limita el acceso a tu red de confianza",
                "install ufw (or another firewall) before exposing any account to the LAN with \
                 “gitmereba cuenta lan --activar”, and limit access to your trusted network",
            ),
            caso_doctor(T::AuditoriaIntegra, "cadena íntegra", "chain intact"),
            caso_doctor(
                T::AuditoriaRota { id: 7 },
                "la cadena de auditoría está rota a partir de la entrada 7",
                "the audit chain is broken from entry 7",
            ),
            caso_doctor(
                T::ConsejoAuditoriaManipulada,
                "investiga si el fichero de la base de datos se ha manipulado a mano",
                "investigate whether the database file has been tampered with by hand",
            ),
            caso_doctor(
                T::AuditoriaNoVerificable {
                    error: TextoExterno::literal("x"),
                },
                "no se pudo verificar: x",
                "could not be verified: x",
            ),
            caso_doctor(
                T::ConsejoAccesoAlmacen,
                "comprueba el acceso al almacén (~/.local/share/gitmereba/gitmereba.db)",
                "check access to the store (~/.local/share/gitmereba/gitmereba.db)",
            ),
            caso_doctor(
                T::CuentasIndiceIlegible {
                    error: TextoExterno::literal("x"),
                },
                "no se pudo leer el índice de cuentas: x",
                "could not read the account index: x",
            ),
            caso_doctor(
                T::ConsejoPermisosIndice,
                "revisa los permisos de ~/.local/share/gitmereba/cuentas.toml",
                "check the permissions of ~/.local/share/gitmereba/cuentas.toml",
            ),
            caso_doctor(
                T::CarpetaNoExiste,
                "la carpeta de la cuenta no existe",
                "the account folder does not exist",
            ),
            caso_doctor(
                T::ConsejoRepetirAltaORestaurar,
                "repite el alta o restaura la carpeta desde una copia",
                "repeat the account setup or restore the folder from a backup",
            ),
            caso_doctor(
                T::CarpetaExisteCon0700,
                "existe con permisos 0700",
                "exists with permissions 0700",
            ),
            caso_doctor(
                T::ErrorSistema {
                    error: TextoExterno::literal("x"),
                },
                "x",
                "x",
            ),
            caso_doctor(
                T::ConsejoRevisarPermisosAMano,
                "revisa los permisos a mano",
                "check the permissions by hand",
            ),
            caso_doctor(
                T::AppIniNoExiste,
                "app.ini no existe",
                "app.ini does not exist",
            ),
            caso_doctor(
                T::ConsejoRepetirAltaProvision,
                "repite el alta: la provisión no llegó a completarse",
                "repeat the account setup: provisioning did not complete",
            ),
            caso_doctor(
                T::ConsejoRevisarFicheroLegible,
                "revisa que el fichero es legible",
                "check that the file is readable",
            ),
            caso_doctor(
                T::ConsejoFicheroLegible,
                "comprueba que el fichero es legible",
                "check that the file is readable",
            ),
            caso_doctor(
                T::AppIniSinLanCorrecto,
                "0600 y HTTP_ADDR = 127.0.0.1",
                "0600 and HTTP_ADDR = 127.0.0.1",
            ),
            caso_doctor(
                T::AppIniMotivos {
                    motivos: vec![
                        MotivoAppIni::Permisos { modo: 0o644 },
                        MotivoAppIni::SinHttpAddrLocal,
                    ],
                },
                "permisos 644 (deberían ser 0600); no contiene «HTTP_ADDR = 127.0.0.1»: Gitea \
                 podría escuchar en la red sin acceso LAN configurado en gitmereba.toml",
                "permissions 644 (should be 0600); does not contain “HTTP_ADDR = 127.0.0.1”: \
                 Gitea might listen on the network without LAN access configured in gitmereba.toml",
            ),
            caso_doctor(
                T::AppIniMotivos {
                    motivos: vec![
                        MotivoAppIni::SinHttps,
                        MotivoAppIni::SinCertificado,
                        MotivoAppIni::ClaveSinPermisos,
                    ],
                },
                "no contiene «PROTOCOL = https» pese a tener acceso LAN configurado; no se \
                 encuentra el certificado del acceso LAN; la clave del certificado no tiene \
                 permisos 0600",
                "does not contain “PROTOCOL = https” despite having LAN access configured; \
                 the LAN access certificate was not found; the certificate key does not have \
                 permissions 0600",
            ),
            caso_doctor(
                T::ConsejoAppIniSinLan,
                "revisa app.ini a mano; sin acceso LAN nunca debe escuchar fuera de 127.0.0.1",
                "check app.ini by hand; without LAN access it must never listen outside 127.0.0.1",
            ),
            caso_doctor(
                T::AppIniExpuestoLan {
                    host: s("a.internal"),
                },
                "expuesto a la LAN por HTTPS (a.internal)",
                "exposed to the LAN over HTTPS (a.internal)",
            ),
            caso_doctor(
                T::ConsejoCortafuegosLan,
                "confirma que hay un cortafuegos limitando el acceso a tu LAN de confianza \
                 (ver la comprobación «cortafuegos»)",
                "make sure a firewall limits access to your trusted LAN (see the “firewall” check)",
            ),
            caso_doctor(
                T::ConsejoRegenerarLan,
                "repite «gitmereba cuenta lan --activar» para regenerar el certificado y app.ini",
                "repeat “gitmereba cuenta lan --activar” to regenerate the certificate and app.ini",
            ),
            caso_doctor(
                T::BinarioNoEncontrado {
                    ruta: ruta("/b/gitea"),
                },
                "no se encuentra /b/gitea",
                "/b/gitea not found",
            ),
            caso_doctor(
                T::ConsejoReinstalarBinario,
                "ejecuta de nuevo el alta o «gitmereba doctor» tras reinstalar",
                "run the account setup again or “gitmereba doctor” after reinstalling",
            ),
            caso_doctor(
                T::BinarioHashCorrecto {
                    version: s("1.27.3"),
                },
                "SHA-256 correcto (1.27.3)",
                "SHA-256 correct (1.27.3)",
            ),
            caso_doctor(
                T::BinarioHashDistinto,
                "el SHA-256 no coincide con el esperado",
                "the SHA-256 does not match the expected one",
            ),
            caso_doctor(
                T::ConsejoBorrarBinario,
                "borra el binario y deja que la app lo vuelva a descargar y verificar",
                "delete the binary and let the app download and verify it again",
            ),
            caso_doctor(
                T::BinarioHashError {
                    error: TextoExterno::literal("x"),
                },
                "no se pudo calcular su SHA-256: x",
                "could not compute its SHA-256: x",
            ),
            caso_doctor(
                T::ConsejoRevisarUrl,
                "revisa la URL de la cuenta",
                "check the account URL",
            ),
            caso_doctor(T::GiteaResponde, "responde", "responding"),
            caso_doctor(T::GiteaNoResponde, "no responde", "not responding"),
            caso_doctor(
                T::ConsejoArrancarGitea,
                "arráncalo con «systemctl --user start» o revisa el servicio",
                "start it with “systemctl --user start” or check the service",
            ),
            caso_doctor(
                T::ConsejoRevisarServicioGitea,
                "revisa el servicio de Gitea",
                "check the Gitea service",
            ),
            caso_doctor(
                T::ConsejoRevisarLlavero,
                "revisa el llavero del sistema",
                "check the system keyring",
            ),
            caso_doctor(
                T::SecretosPresentes,
                "los tres secretos están presentes",
                "all three secrets are present",
            ),
            caso_doctor(
                T::SecretosFaltan {
                    faltan: vec![s("token-github"), s("token-gitea")],
                },
                "faltan en el llavero: token-github, token-gitea",
                "missing from the keyring: token-github, token-gitea",
            ),
            caso_doctor(
                T::ConsejoRegenerarSecretos,
                "repite el alta para regenerarlos",
                "repeat the account setup to regenerate them",
            ),
            caso_doctor(
                T::SnapshotsResumen {
                    total: 3,
                    protegidas: 1,
                },
                "3 capturas, 1 protegida",
                "3 snapshots, 1 protected",
            ),
            caso_doctor(
                T::SnapshotsResumen {
                    total: 1,
                    protegidas: 1,
                },
                "1 captura, 1 protegida",
                "1 snapshot, 1 protected",
            ),
            caso_doctor(
                T::SnapshotsResumen {
                    total: 89,
                    protegidas: 0,
                },
                "89 capturas, 0 protegidas",
                "89 snapshots, 0 protected",
            ),
            caso_doctor(
                T::ConsejoCapturasProtegidas,
                "hay capturas protegidas por un cambio destructivo detectado en el origen \
                 (historia reescrita, rama o tag borrado): revísalas antes de que la retención \
                 normal pueda alcanzarlas",
                "there are snapshots protected by a destructive change detected at the origin \
                 (rewritten history, deleted branch or tag): review them before normal \
                 retention can reach them",
            ),
            caso_doctor(
                T::SnapshotsError {
                    error: TextoExterno::literal("x"),
                },
                "no se pudieron listar los snapshots: x",
                "could not list the snapshots: x",
            ),
            caso_doctor(
                T::ConsejoPermisosSnapshots,
                "comprueba los permisos de la carpeta «snapshots/» de la cuenta",
                "check the permissions of the account's “snapshots/” folder",
            ),
            caso_doctor(
                T::TemporizadorNoInstalado,
                "no hay temporizador de sincronización instalado",
                "no sync timer is installed",
            ),
            caso_doctor(
                T::ConsejoAbrirVentanaInstala,
                "abre la ventana de gitmereba una vez: lo instala sola; hasta entonces solo se \
                 sincroniza a mano",
                "open the gitmereba window once: it installs itself; until then it only syncs \
                 by hand",
            ),
            caso_doctor(
                T::TemporizadorSinExecStart {
                    ruta: ruta("/u/x.service"),
                },
                "«/u/x.service» no tiene un ExecStart reconocible",
                "“/u/x.service” has no recognizable ExecStart",
            ),
            caso_doctor(
                T::ConsejoAbrirVentanaReescribe,
                "abre la ventana de gitmereba una vez: reescribe la unidad",
                "open the gitmereba window once: it rewrites the unit",
            ),
            caso_doctor(
                T::TemporizadorSincroniza {
                    ejecutable: s("/bin/gm"),
                },
                "sincroniza con «/bin/gm»",
                "syncs with “/bin/gm”",
            ),
            caso_doctor(
                T::TemporizadorEjecutablePerdido {
                    ejecutable: s("/bin/gm"),
                },
                "el temporizador apunta a «/bin/gm», que ya no existe o no es ejecutable",
                "the timer points to “/bin/gm”, which no longer exists or is not executable",
            ),
            caso_doctor(
                T::ConsejoActualizarTemporizador,
                "abre la ventana de gitmereba una vez (o guarda Ajustes): el temporizador pasa a \
                 usar el ejecutable actual",
                "open the gitmereba window once (or save Settings): the timer switches to the \
                 current executable",
            ),
            caso_doctor(
                T::IdiomaPreferencia {
                    idioma: Idioma::En,
                    ruta: ruta("/c/preferencias.toml"),
                },
                "en (preferencia, /c/preferencias.toml)",
                "en (preference, /c/preferencias.toml)",
            ),
            caso_doctor(
                T::IdiomaVariable {
                    idioma: Idioma::Es,
                    variable: s("LANG"),
                    valor: s("es_ES.UTF-8"),
                },
                "es (de LANG=es_ES.UTF-8)",
                "es (from LANG=es_ES.UTF-8)",
            ),
            caso_doctor(
                T::IdiomaPorDefecto { idioma: Idioma::En },
                "en (por defecto: no hay LC_ALL, LC_MESSAGES ni LANG)",
                "en (default: LC_ALL, LC_MESSAGES and LANG are not set)",
            ),
            caso_doctor(
                T::ConsejoFijarIdioma,
                "fija el idioma en Ajustes",
                "set the language in Settings",
            ),
            caso_doctor(
                T::TemporizadorIdiomaSinVariables,
                "el temporizador no tiene LC_ALL, LC_MESSAGES ni LANG: sus avisos saldrán en inglés",
                "the timer has no LC_ALL, LC_MESSAGES or LANG: its notices will be in English",
            ),
            caso_doctor(
                T::TemporizadorIdiomaDistinto {
                    temporizador: Idioma::En,
                    sesion: Idioma::Es,
                },
                "el temporizador usa el idioma en y esta sesión es",
                "the timer uses language en and this session es",
            ),
            caso_doctor(
                T::TemporizadorIdiomaCoincide { idioma: Idioma::Es },
                "el temporizador usa el mismo idioma que la sesión (es)",
                "the timer uses the same language as the session (es)",
            ),
            caso_doctor(
                T::TemporizadorIdiomaNoDisponible,
                "no se pudo consultar el entorno del temporizador (systemctl no disponible o sin sesión de usuario)",
                "could not query the timer environment (systemctl not available or no user session)",
            ),
            caso_doctor(
                T::IdiomaPreferenciaNoValida {
                    idioma: Idioma::En,
                    ruta: ruta("/c/p.toml"),
                },
                "no se pudo interpretar «/c/p.toml»; idioma en uso: en",
                "could not parse “/c/p.toml”; language in use: en",
            ),
            caso_doctor(
                T::ConsejoCorregirPreferencias,
                "corrige o borra el fichero, o fija el idioma en Ajustes",
                "fix or delete the file, or set the language in Settings",
            ),
        ]
    }

    #[test]
    fn cada_frase_de_doctor_se_renderiza_en_espanol_y_en_ingles() {
        for (texto, es, en) in casos_doctor() {
            assert_eq!(texto.localizar(Idioma::Es), es, "{texto:?}");
            assert_eq!(texto.localizar(Idioma::En), en, "{texto:?}");
        }
    }

    #[test]
    fn los_nombres_de_comprobacion_se_renderizan_en_ambos_idiomas() {
        use NombreComprobacion as N;
        let cuenta = |parte| N::Cuenta {
            login: s("jparga"),
            parte,
        };
        let casos = [
            (N::Git, "git", "git"),
            (N::Gpgv, "gpgv", "gpgv"),
            (N::Llavero, "llavero", "keyring"),
            (N::DirectorioDatos, "directorio-datos", "data-directory"),
            (N::Auditoria, "auditoria", "audit"),
            (
                N::AislamientoSystemd,
                "aislamiento-systemd",
                "systemd-isolation",
            ),
            (N::Cortafuegos, "cortafuegos", "firewall"),
            (N::Cuentas, "cuentas", "accounts"),
            (N::Idioma, "idioma", "language"),
            (
                cuenta(ParteCuenta::Carpeta),
                "cuenta:jparga:carpeta",
                "account:jparga:folder",
            ),
            (
                cuenta(ParteCuenta::AppIni),
                "cuenta:jparga:app.ini",
                "account:jparga:app.ini",
            ),
            (
                cuenta(ParteCuenta::BinarioGitea),
                "cuenta:jparga:binario-gitea",
                "account:jparga:gitea-binary",
            ),
            (
                cuenta(ParteCuenta::Gitea),
                "cuenta:jparga:gitea",
                "account:jparga:gitea",
            ),
            (
                cuenta(ParteCuenta::Secretos),
                "cuenta:jparga:secretos",
                "account:jparga:secrets",
            ),
            (
                cuenta(ParteCuenta::Snapshots),
                "cuenta:jparga:snapshots",
                "account:jparga:snapshots",
            ),
            (
                cuenta(ParteCuenta::Temporizador),
                "cuenta:jparga:temporizador",
                "account:jparga:timer",
            ),
        ];
        for (nombre, es, en) in casos {
            assert_eq!(nombre.localizar(Idioma::Es), es, "{nombre:?}");
            assert_eq!(nombre.localizar(Idioma::En), en, "{nombre:?}");
        }
    }
}
