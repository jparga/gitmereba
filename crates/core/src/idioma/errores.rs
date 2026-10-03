//! Errores de `core` en cualquiera de los idiomas soportados.
//!
//! El español es el `#[error("…")]` de cada enum (`to_string()`); el inglés sale del
//! catálogo `en::errores`.

use super::en;
use super::{Idioma, Localizable};
use crate::almacen::ErrorAlmacen;
use crate::avisos::ErrorAvisos;
use crate::config::ErrorConfig;
use crate::contingencia::ErrorContingencia;
use crate::cuentas::ErrorCuentas;
use crate::git::ErrorGit;
use crate::gitea::ErrorGitea;
use crate::github::ErrorGithub;
use crate::instancia::ErrorInstancia;
use crate::modelo::{ErrorHostInterno, ErrorNombre};
use crate::secretos::ErrorLlavero;
use crate::snapshots::ErrorSnapshots;
use crate::sync::ErrorSync;
use crate::verificacion::ErrorEspacio;

/// Implementa `Localizable` para un error: español = `to_string()`, inglés = catálogo.
macro_rules! error_localizable {
    ($tipo:ty, $texto:path) => {
        impl Localizable for $tipo {
            fn localizar(&self, idioma: Idioma) -> String {
                match idioma {
                    Idioma::Es => self.to_string(),
                    Idioma::En => $texto(self),
                }
            }
        }
    };
}

error_localizable!(ErrorSync, en::errores::sync::texto);
error_localizable!(ErrorGithub, en::errores::github::texto);
error_localizable!(ErrorGitea, en::errores::gitea::texto);
error_localizable!(ErrorLlavero, en::errores::llavero::texto);
error_localizable!(ErrorGit, en::errores::git::texto);
error_localizable!(ErrorEspacio, en::errores::espacio::texto);
error_localizable!(ErrorAvisos, en::errores::avisos::texto);
error_localizable!(ErrorAlmacen, en::errores::almacen::texto);
error_localizable!(ErrorNombre, en::errores::modelo::nombre);
error_localizable!(ErrorHostInterno, en::errores::modelo::host);
error_localizable!(ErrorConfig, en::errores::config::texto);
error_localizable!(ErrorSnapshots, en::errores::snapshots::texto);
error_localizable!(ErrorInstancia, en::errores::instancia::texto);
error_localizable!(ErrorContingencia, en::errores::contingencia::texto);
error_localizable!(ErrorCuentas, en::errores::cuentas::texto);

#[cfg(test)]
mod tests {
    use crate::almacen::ErrorAlmacen;
    use crate::avisos::ErrorAvisos;
    use crate::config::ErrorConfig;
    use crate::contingencia::{ErrorContingencia, PasoActivar};
    use crate::cuentas::ErrorCuentas;
    use crate::git::ErrorGit;
    use crate::gitea::ErrorGitea;
    use crate::github::ErrorGithub;
    use crate::idioma::{Idioma, Localizable};
    use crate::instancia::ErrorInstancia;
    use crate::modelo::{ErrorHostInterno, ErrorNombre, IdRepo, Nombre};
    use crate::secretos::ErrorLlavero;
    use crate::snapshots::ErrorSnapshots;
    use crate::sync::ErrorSync;
    use crate::verificacion::ErrorEspacio;
    use std::path::PathBuf;

    fn nombre(n: &str) -> Nombre {
        Nombre::nuevo(n).expect("nombre válido")
    }

    /// Cada caso: error, inglés esperado. El español debe coincidir siempre con `to_string()`.
    fn comprobar<E: Localizable + std::fmt::Display>(casos: Vec<(E, &str)>) {
        for (error, ingles) in casos {
            assert_eq!(error.localizar(Idioma::Es), error.to_string());
            assert_eq!(error.localizar(Idioma::En), ingles);
        }
    }

    #[test]
    fn sync() {
        comprobar(vec![
            (
                ErrorSync::TokenDeOtraCuenta {
                    esperado: nombre("ana"),
                    obtenido: nombre("luis"),
                },
                "the token belongs to “luis”, not to the expected account “ana”",
            ),
            (
                ErrorSync::Github(ErrorGithub::TokenInvalido),
                "GitHub: the GitHub token is invalid or has expired",
            ),
            (
                ErrorSync::Gitea(ErrorGitea::NoDisponible),
                "Gitea: Gitea is not available (connection refused or timed out)",
            ),
        ]);
    }

    #[test]
    fn github() {
        comprobar(vec![
            (
                ErrorGithub::TokenInvalido,
                "the GitHub token is invalid or has expired",
            ),
            (
                ErrorGithub::LimiteDePeticiones { reinicio: None },
                "the GitHub request limit has been reached",
            ),
            (
                ErrorGithub::Red("boom".into()),
                "network error talking to GitHub: boom",
            ),
            (
                ErrorGithub::RespuestaInesperada {
                    estado: 502,
                    detalle: "x".into(),
                },
                "GitHub responded unexpectedly (502): x",
            ),
        ]);
    }

    #[test]
    fn gitea() {
        comprobar(vec![
            (
                ErrorGitea::UrlNoLocal,
                "the Gitea URL must be local (127.0.0.1, localhost or [::1]), with no user/password or path",
            ),
            (
                ErrorGitea::IntervaloInvalido("5m".into()),
                "invalid mirror interval: “5m” (use “0” or “<n>m”/“<n>h”, minimum 10 minutes)",
            ),
            (
                ErrorGitea::RespuestaInesperada {
                    estado: 500,
                    detalle: "d".into(),
                },
                "unexpected response from Gitea (500): d",
            ),
            (
                ErrorGitea::DatosInvalidos("j".into()),
                "invalid data received from Gitea: j",
            ),
        ]);
    }

    #[test]
    fn llavero_git_espacio_avisos_almacen() {
        comprobar(vec![
            (
                ErrorLlavero::Bloqueado("x".into()),
                "the system keyring is locked: x",
            ),
            (
                ErrorLlavero::NoDisponible("y".into()),
                "no system keyring is available: y",
            ),
        ]);
        comprobar(vec![
            (
                ErrorGit::NoEncontrado,
                "git is not installed or not found in the PATH",
            ),
            (
                ErrorGit::Fallo {
                    codigo: 128,
                    stderr: "fatal".into(),
                },
                "git exited with code 128: fatal",
            ),
        ]);
        comprobar(vec![(
            ErrorEspacio::Io("e".into()),
            "I/O error computing disk space: e",
        )]);
        comprobar(vec![(
            ErrorAvisos::Notificador("n".into()),
            "could not send the desktop notification: n",
        )]);
        comprobar(vec![
            (
                ErrorAlmacen::VersionFutura {
                    encontrada: 9,
                    conocida: 3,
                },
                "the database is at version 9, newer than the one known to this version of the app (3)",
            ),
            (ErrorAlmacen::Sqlite("s".into()), "sqlite error: s"),
        ]);
    }

    #[test]
    fn nombres_y_hosts() {
        comprobar(vec![
            (
                ErrorNombre::DemasiadoLargo(100),
                "the name is longer than 100 characters",
            ),
            (ErrorNombre::Vacio, "the name is empty"),
        ]);
        comprobar(vec![
            (
                ErrorHostInterno::NoTerminaEnInternal,
                "the host name must end in “.internal”",
            ),
            (
                ErrorHostInterno::EtiquetaInvalida("a_b".into()),
                "the host name label “a_b” is not valid",
            ),
        ]);
    }

    #[test]
    fn config() {
        comprobar(vec![
            (
                ErrorConfig::SinDirectorioHome,
                "the user's home directory was not found",
            ),
            (
                ErrorConfig::Toml("t".into()),
                "error reading or writing TOML: t",
            ),
            (
                ErrorConfig::LoginDuplicado("ana".into()),
                "the login “ana” is already in use by another account",
            ),
            (
                ErrorConfig::CarpetaDuplicada(PathBuf::from("/x/y")),
                "the folder “/x/y” is already in use by another account",
            ),
            (
                ErrorConfig::PuertoDuplicado(3000),
                "port 3000 is already in use by another account",
            ),
        ]);
    }

    #[test]
    fn snapshots() {
        comprobar(vec![
            (
                ErrorSnapshots::Git(ErrorGit::Timeout),
                "git: the git command exceeded the time limit",
            ),
            (
                ErrorSnapshots::MarcaInvalida("zz".into()),
                "the timestamp “zz” does not have the format YYYYMMDDTHHMMSSZ",
            ),
            (
                ErrorSnapshots::ManifiestoDemasiadoGrande(7),
                "the manifest takes 7 bytes, more than the allowed maximum",
            ),
            (
                ErrorSnapshots::OrigenNoExiste,
                "the source repository does not exist in “gitea/repositories/”",
            ),
        ]);
    }

    #[test]
    fn instancia() {
        comprobar(vec![
            (
                ErrorInstancia::TamanoExcedido(50),
                "the download exceeds the maximum allowed size (50 MB)",
            ),
            (
                ErrorInstancia::GiteaFallo {
                    codigo: 1,
                    stderr: "e".into(),
                },
                "gitea exited with code 1: e",
            ),
            (
                ErrorInstancia::UsuarioGiteaYaExiste(nombre("ana")),
                "the user “ana” already exists on Gitea",
            ),
            (
                ErrorInstancia::Llavero(ErrorLlavero::Bloqueado("x".into())),
                "error accessing the keyring: the system keyring is locked: x",
            ),
        ]);
    }

    #[test]
    fn contingencia() {
        let repo = IdRepo {
            dueno: nombre("ana"),
            nombre: nombre("web"),
        };
        comprobar(vec![
            (
                ErrorContingencia::NoEsMirror(repo.clone()),
                "“ana/web” is not a mirror",
            ),
            (
                ErrorContingencia::FalloAlActivar {
                    repo: repo.clone(),
                    paso: PasoActivar::CrearRepo,
                    origen: "o".into(),
                },
                "activating the contingency of “ana/web” failed while creating the sibling repo: o",
            ),
            (
                ErrorContingencia::ConfirmacionIncorrecta {
                    esperado: "ana/web".into(),
                },
                "incorrect confirmation: “ana/web” was expected",
            ),
            (
                ErrorContingencia::NombreInvalido(ErrorNombre::Vacio),
                "invalid contingency organization name: the name is empty",
            ),
            (
                ErrorContingencia::Git(ErrorGit::NoEncontrado),
                "git: git is not installed or not found in the PATH",
            ),
        ]);
    }

    #[test]
    fn cuentas() {
        comprobar(vec![
            (
                ErrorCuentas::GiteaParado(nombre("ana")),
                "the Gitea of “ana” is not responding; check it with “gitmereba doctor”",
            ),
            (
                ErrorCuentas::CarpetaNoValidaParaBorrar(PathBuf::from("/d"), nombre("ana")),
                "the folder “/d” is not the data folder of “ana”: it will not be deleted",
            ),
            (
                ErrorCuentas::Github(ErrorGithub::TokenInvalido),
                "GitHub: the GitHub token is invalid or has expired",
            ),
            (
                ErrorCuentas::Config(ErrorConfig::CarpetaRaiz),
                "configuration: the destination folder cannot be the filesystem root",
            ),
            (
                ErrorCuentas::Sync(ErrorSync::Gitea(ErrorGitea::SinPermiso)),
                "synchronization: Gitea: no permission for this operation on Gitea",
            ),
            (
                ErrorCuentas::Almacen(ErrorAlmacen::Io("i".into())),
                "store: I/O error: i",
            ),
        ]);
    }

    /// Transversal: el español nunca cambia respecto a `#[error]` y el inglés no queda vacío.
    #[test]
    fn el_espanol_es_el_error_original_y_el_ingles_existe() {
        trait Ambos: Localizable + std::fmt::Display {}
        impl<T: Localizable + std::fmt::Display> Ambos for T {}
        let todos: Vec<Box<dyn Ambos>> = vec![
            Box::new(ErrorConfig::CarpetaNoVacia),
            Box::new(ErrorConfig::Io("i".into())),
            Box::new(ErrorSnapshots::SinCapturas),
            Box::new(ErrorSnapshots::Json("j".into())),
            Box::new(ErrorInstancia::Timeout),
            Box::new(ErrorInstancia::HostNoPermitido("h".into())),
            Box::new(ErrorContingencia::OrigenNoGithub),
            Box::new(ErrorContingencia::Io("i".into())),
            Box::new(ErrorCuentas::Interno("x".into())),
            Box::new(ErrorCuentas::Io("x".into())),
            Box::new(ErrorCuentas::RutaProtegida(PathBuf::from("/"))),
            Box::new(ErrorCuentas::Llavero(ErrorLlavero::Backend("b".into()))),
            Box::new(ErrorCuentas::Instancia(ErrorInstancia::Sha256NoCoincide)),
            Box::new(ErrorCuentas::Gitea(ErrorGitea::YaExiste)),
        ];
        for error in todos {
            assert_eq!(error.localizar(Idioma::Es), error.to_string());
            let ingles = error.localizar(Idioma::En);
            assert!(!ingles.is_empty());
            assert_ne!(ingles, error.to_string(), "{ingles}");
        }
    }
}
