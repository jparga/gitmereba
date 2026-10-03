//! Errores de `core` en cualquiera de los idiomas soportados.
//!
//! El español es el `#[error("…")]` de cada enum (`to_string()`); el inglés sale del
//! catálogo `en::errores`.

use super::en;
use super::{Idioma, Localizable};
use crate::almacen::ErrorAlmacen;
use crate::avisos::ErrorAvisos;
use crate::git::ErrorGit;
use crate::gitea::ErrorGitea;
use crate::github::ErrorGithub;
use crate::modelo::{ErrorHostInterno, ErrorNombre};
use crate::secretos::ErrorLlavero;
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

#[cfg(test)]
mod tests {
    use crate::almacen::ErrorAlmacen;
    use crate::avisos::ErrorAvisos;
    use crate::git::ErrorGit;
    use crate::gitea::ErrorGitea;
    use crate::github::ErrorGithub;
    use crate::idioma::{Idioma, Localizable};
    use crate::modelo::{ErrorHostInterno, ErrorNombre, Nombre};
    use crate::secretos::ErrorLlavero;
    use crate::sync::ErrorSync;
    use crate::verificacion::ErrorEspacio;

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
}
