//! Errores de la orquestación de cuentas, en inglés.

use crate::cuentas::ErrorCuentas;
use crate::idioma::{Idioma, Localizable};

pub(crate) fn texto(e: &ErrorCuentas) -> String {
    match e {
        ErrorCuentas::TokenNoCoincideConLogin { esperado, obtenido } => {
            format!("the token belongs to “{obtenido}”, not to “{esperado}”")
        }
        ErrorCuentas::GiteaParado(login) => {
            format!("the Gitea of “{login}” is not responding; check it with “gitmereba doctor”")
        }
        ErrorCuentas::CuentaNoExiste(login) => {
            format!("there is no account with the login “{login}”")
        }
        ErrorCuentas::RepoNoExiste(repo) => {
            format!("the repository “{repo}” does not exist in this account")
        }
        ErrorCuentas::RepoEnContingencia(repo) => format!(
            "“{repo}” is in contingency: its mirror is not synchronized until it is reconciled"
        ),
        ErrorCuentas::RepoExcluido(repo) => {
            format!("“{repo}” is excluded: include it again to synchronize it")
        }
        ErrorCuentas::TokenGiteaNoGenerado(login) => {
            format!("no Gitea token was generated while provisioning “{login}”")
        }
        ErrorCuentas::UsuarioLanNoValido(nombre) => {
            format!("“{nombre}” is not a valid name for a LAN user")
        }
        ErrorCuentas::UsuarioLanYaExiste(nombre) => {
            format!("a Gitea user named “{nombre}” already exists")
        }
        ErrorCuentas::UsuarioLanNoExiste(nombre) => {
            format!("there is no LAN user named “{nombre}”")
        }
        ErrorCuentas::CarpetaNoValidaParaBorrar(ruta, login) => format!(
            "the folder “{}” is not the data folder of “{login}”: it will not be deleted",
            ruta.display()
        ),
        ErrorCuentas::RutaProtegida(ruta) => format!(
            "“{}” will not be deleted: it is a protected path (root, HOME or one of its ancestors)",
            ruta.display()
        ),
        ErrorCuentas::Io(detalle) => format!("I/O error: {detalle}"),
        ErrorCuentas::Interno(detalle) => format!("internal error: {detalle}"),
        ErrorCuentas::Github(interno) => format!("GitHub: {}", interno.localizar(Idioma::En)),
        ErrorCuentas::Gitea(interno) => format!("Gitea: {}", interno.localizar(Idioma::En)),
        ErrorCuentas::Config(interno) => {
            format!("configuration: {}", interno.localizar(Idioma::En))
        }
        ErrorCuentas::Instancia(interno) => {
            format!("Gitea instance: {}", interno.localizar(Idioma::En))
        }
        ErrorCuentas::Llavero(interno) => format!("keyring: {}", interno.localizar(Idioma::En)),
        ErrorCuentas::Almacen(interno) => format!("store: {}", interno.localizar(Idioma::En)),
        ErrorCuentas::Sync(interno) => {
            format!("synchronization: {}", interno.localizar(Idioma::En))
        }
    }
}
