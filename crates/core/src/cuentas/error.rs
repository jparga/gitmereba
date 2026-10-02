//! Error del módulo `cuentas`.

use std::path::PathBuf;

use crate::almacen::ErrorAlmacen;
use crate::config::ErrorConfig;
use crate::gitea::ErrorGitea;
use crate::github::ErrorGithub;
use crate::instancia::ErrorInstancia;
use crate::modelo::{IdRepo, Nombre};
use crate::secretos::ErrorLlavero;
use crate::sync::ErrorSync;

/// Fallos de la orquestación de cuentas. Nunca contiene secretos: los tipos de error de
/// los demás módulos ya lo garantizan y aquí no se añade ningún dato nuevo que pueda
/// llevarlos.
#[derive(Debug, thiserror::Error)]
pub enum ErrorCuentas {
    /// El login del token no coincide con el login pedido en el alta.
    #[error("el token pertenece a «{obtenido}», no a «{esperado}»")]
    TokenNoCoincideConLogin { esperado: Nombre, obtenido: Nombre },
    /// El Gitea de la cuenta no responde: no se ha intentado nada más.
    #[error("el Gitea de «{0}» no responde; compruébalo con «gitmereba doctor»")]
    GiteaParado(Nombre),
    /// La cuenta no existe en el índice.
    #[error("no existe ninguna cuenta con el login «{0}»")]
    CuentaNoExiste(Nombre),
    /// El repo pedido no pertenece a la cuenta (ni es su login ni una de sus
    /// organizaciones en alcance) o no existe en el Gitea local de la cuenta.
    #[error("no existe el repositorio «{0}» en esta cuenta")]
    RepoNoExiste(IdRepo),
    /// El repo tiene una contingencia activa: su mirror no se sincroniza hasta reconciliar.
    #[error("«{0}» está en contingencia: su mirror no se sincroniza hasta reconciliar")]
    RepoEnContingencia(IdRepo),
    /// El usuario excluyó el repo: hay que volver a incluirlo antes de sincronizarlo.
    #[error("«{0}» está excluido: vuelve a incluirlo para sincronizarlo")]
    RepoExcluido(IdRepo),
    /// `provisionar` no dejó un token de Gitea en el llavero: no se puede continuar.
    #[error("no se generó un token de Gitea durante la provisión de «{0}»")]
    TokenGiteaNoGenerado(Nombre),
    /// `nombre` no es válido para un usuario de la LAN: es el administrador de
    /// Gitea, o tiene forma de organización de contingencia (`contingencia-*`).
    #[error("«{0}» no es un nombre válido para un usuario de la LAN")]
    UsuarioLanNoValido(Nombre),
    /// Ya existe un usuario de Gitea con ese nombre.
    #[error("ya existe un usuario de Gitea llamado «{0}»")]
    UsuarioLanYaExiste(Nombre),
    /// No hay ningún usuario de la LAN con ese nombre.
    #[error("no hay ningún usuario de la LAN llamado «{0}»")]
    UsuarioLanNoExiste(Nombre),
    /// La carpeta a borrar no es válida como carpeta de datos de la cuenta (defensa
    /// contra borrar una carpeta equivocada).
    #[error("la carpeta «{0}» no es la carpeta de datos de «{1}»: no se borra")]
    CarpetaNoValidaParaBorrar(PathBuf, Nombre),
    /// La carpeta a borrar es la raíz, el HOME o un ancestro del HOME.
    #[error("no se borra «{0}»: es una ruta protegida (raíz, HOME o un ancestro suyo)")]
    RutaProtegida(PathBuf),
    #[error("error de E/S: {0}")]
    Io(String),
    #[error("error interno: {0}")]
    Interno(String),
    #[error("GitHub: {0}")]
    Github(#[from] ErrorGithub),
    #[error("Gitea: {0}")]
    Gitea(#[from] ErrorGitea),
    #[error("configuración: {0}")]
    Config(#[from] ErrorConfig),
    #[error("instancia de Gitea: {0}")]
    Instancia(#[from] ErrorInstancia),
    #[error("llavero: {0}")]
    Llavero(#[from] ErrorLlavero),
    #[error("almacén: {0}")]
    Almacen(#[from] ErrorAlmacen),
    #[error("sincronización: {0}")]
    Sync(#[from] ErrorSync),
}

impl ErrorCuentas {
    /// Código estable, en minúsculas y con guiones, para que la interfaz pueda
    /// reaccionar sin analizar el mensaje.
    pub fn codigo(&self) -> &'static str {
        match self {
            ErrorCuentas::TokenNoCoincideConLogin { .. } => "token-no-coincide-login",
            ErrorCuentas::GiteaParado(_) => "gitea-parado",
            ErrorCuentas::CuentaNoExiste(_) => "cuenta-no-existe",
            ErrorCuentas::RepoNoExiste(_) => "repo-no-existe",
            ErrorCuentas::RepoEnContingencia(_) => "repo-en-contingencia",
            ErrorCuentas::RepoExcluido(_) => "repo-excluido",
            ErrorCuentas::TokenGiteaNoGenerado(_) => "token-gitea-no-generado",
            ErrorCuentas::UsuarioLanNoValido(_) => "usuario-no-valido",
            ErrorCuentas::UsuarioLanYaExiste(_) => "usuario-ya-existe",
            ErrorCuentas::UsuarioLanNoExiste(_) => "usuario-no-existe",
            ErrorCuentas::CarpetaNoValidaParaBorrar(..) => "carpeta-no-valida-para-borrar",
            ErrorCuentas::RutaProtegida(_) => "ruta-protegida",
            ErrorCuentas::Io(_) => "io",
            ErrorCuentas::Interno(_) => "interno",
            ErrorCuentas::Github(ErrorGithub::TokenInvalido) => "token-invalido",
            ErrorCuentas::Github(ErrorGithub::LimiteDePeticiones { .. }) => {
                "limite-de-peticiones-github"
            }
            ErrorCuentas::Github(_) => "github",
            ErrorCuentas::Gitea(ErrorGitea::TokenInvalido) => "token-invalido",
            ErrorCuentas::Gitea(_) => "gitea",
            ErrorCuentas::Config(ErrorConfig::CarpetaNoVacia) => "carpeta-no-vacia",
            ErrorCuentas::Config(ErrorConfig::LoginDuplicado(_)) => "login-duplicado",
            ErrorCuentas::Config(ErrorConfig::CarpetaDuplicada(_)) => "carpeta-duplicada",
            ErrorCuentas::Config(ErrorConfig::PuertoDuplicado(_)) => "puerto-duplicado",
            ErrorCuentas::Config(ErrorConfig::IntervaloDemasiadoCorto) => "intervalo-corto",
            ErrorCuentas::Config(_) => "configuracion",
            ErrorCuentas::Instancia(_) => "instancia",
            ErrorCuentas::Llavero(_) => "llavero",
            ErrorCuentas::Almacen(_) => "almacen",
            ErrorCuentas::Sync(_) => "sincronizacion",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_es_estable_para_token_invalido_de_github() {
        let error = ErrorCuentas::Github(ErrorGithub::TokenInvalido);
        assert_eq!(error.codigo(), "token-invalido");
    }

    #[test]
    fn el_mensaje_no_incluye_la_palabra_ghp() {
        let error = ErrorCuentas::TokenNoCoincideConLogin {
            esperado: Nombre::nuevo("a").expect("nombre"),
            obtenido: Nombre::nuevo("b").expect("nombre"),
        };
        assert!(!error.to_string().contains("ghp_"));
    }
}
