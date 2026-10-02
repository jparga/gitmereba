//! Cliente de la API de GitHub.
//!
//! Todo el acceso pasa por el trait [`ApiGithub`], para poder sustituirlo por un doble en
//! las pruebas de otros módulos (`sync`, `verificacion`). La implementación real,
//! [`ClienteGithub`], nunca deja escapar el token en un error ni en un registro.

mod caducidad;
mod cliente;
mod dto;
mod error;
mod paginacion;

pub use cliente::ClienteGithub;
pub use error::ErrorGithub;

use time::OffsetDateTime;

use crate::modelo::{IdRepo, Nombre, RepoOrigen};

/// Identidad asociada al token en uso, tal como la informa `GET /user`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identidad {
    /// Login de la cuenta.
    pub login: Nombre,
    /// Permisos del token (cabecera `x-oauth-scopes`); vacío en tokens fine-grained.
    pub scopes: Vec<String>,
    /// Caducidad del token, si GitHub la informa.
    pub caduca: Option<OffsetDateTime>,
}

/// Estado de la plataforma GitHub, según <https://www.githubstatus.com>.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoServicio {
    /// Indicador `none`.
    Operativo,
    /// Indicador `minor`.
    Degradado,
    /// Indicador `major` o `critical`.
    Caido,
    /// Cualquier otro valor no reconocido.
    Desconocido,
}

/// Operaciones de solo lectura contra la API de GitHub necesarias para el resto de
/// módulos.
///
/// Se define como trait (no `dyn`) para poder sustituirlo por un doble de pruebas vía
/// genéricos, sin coste de `Box<dyn Trait>` en el código real.
// Se usa siempre con genéricos (nunca `dyn ApiGithub`), así que la falta de un bound
// `Send` explícito en el futuro no es un problema: cada llamador conoce el tipo concreto.
#[allow(async_fn_in_trait)]
pub trait ApiGithub {
    /// `GET /user`: login, scopes y caducidad del token en uso.
    async fn identidad(&self) -> Result<Identidad, ErrorGithub>;

    /// `GET /user/orgs`: organizaciones a las que pertenece la cuenta.
    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGithub>;

    /// `GET /user/repos`: repos propios de la cuenta (incluye privados y forks).
    async fn repos_de_usuario(&self) -> Result<Vec<RepoOrigen>, ErrorGithub>;

    /// `GET /orgs/{org}/repos`: repos de una organización.
    async fn repos_de_organizacion(&self, org: &Nombre) -> Result<Vec<RepoOrigen>, ErrorGithub>;

    /// `GET /repos/{owner}/{repo}/branches/{rama}`: SHA de la rama, o `None` si no existe.
    async fn sha_de_rama(&self, repo: &IdRepo, rama: &str) -> Result<Option<String>, ErrorGithub>;

    /// Estado de githubstatus.com. No requiere ni envía el token.
    async fn estado_servicio(&self) -> Result<EstadoServicio, ErrorGithub>;
}
