//! Módulo `gitea`.
//!
//! Cliente de la API REST v1 de un Gitea **local**: organizaciones, mirrors de solo
//! lectura y repos normales. La app siempre le habla por loopback
//! (`127.0.0.1`/`localhost`/`[::1]`); [`ClienteGitea::nuevo`] rechaza cualquier otro
//! host y solo acepta `http`. Una cuenta puede exponer su Gitea a la LAN
//! por HTTPS con un nombre `.internal` (ver `crate::cuentas::exponer_lan`): en ese caso la app le
//! sigue hablando por loopback, pero con `https://127.0.0.1:<puerto>` y confiando
//! **solo** en el certificado autofirmado generado para esa cuenta
//! ([`ClienteGitea::nuevo_con_certificado`]), nunca en las CA del sistema.

mod cliente;
mod dto;
mod equipo;
mod error;
mod peticion;
mod saneado;

pub use cliente::ClienteGitea;
pub use equipo::{DefinicionEquipo, PermisoEquipo};
pub use error::ErrorGitea;
pub use peticion::PeticionMirror;

use crate::modelo::{IdRepo, Nombre, RepoLocal};

/// Operaciones contra la API de Gitea.
///
/// Se implementa sobre un Gitea real (`ClienteGitea`) y sobre dobles en pruebas de otros
/// módulos; por eso es un trait y se usa con genéricos, no con `dyn`.
#[allow(async_fn_in_trait)] // uso previsto: genéricos, nunca objetos `dyn ApiGitea`.
pub trait ApiGitea {
    /// `GET /api/healthz`. `Ok(false)` si Gitea no responde (no es un error); solo es
    /// `Err` si algo distinto de la disponibilidad falla.
    async fn salud(&self) -> Result<bool, ErrorGitea>;

    /// `GET /api/v1/version`.
    async fn version(&self) -> Result<String, ErrorGitea>;

    /// Crea la organización si no existe todavía. Idempotente, incluida la carrera en la
    /// que otro proceso la crea entre el `GET` y el `POST`.
    async fn asegurar_organizacion(&self, nombre: &Nombre) -> Result<(), ErrorGitea>;

    /// Repos de una organización, paginando toda la colección.
    async fn repos_de(&self, dueno: &Nombre) -> Result<Vec<RepoLocal>, ErrorGitea>;

    /// Un repo por id, o `None` si no existe (404).
    async fn repo(&self, id: &IdRepo) -> Result<Option<RepoLocal>, ErrorGitea>;

    /// Crea un pull-mirror de un repo de GitHub. `Err(ErrorGitea::YaExiste)` si ya hay un
    /// repo con ese nombre.
    async fn crear_mirror(&self, peticion: &PeticionMirror) -> Result<RepoLocal, ErrorGitea>;

    /// Fuerza una sincronización inmediata del mirror.
    async fn sincronizar_mirror(&self, id: &IdRepo) -> Result<(), ErrorGitea>;

    /// Cambia el intervalo de sincronización del mirror (`"0"` lo pausa).
    async fn fijar_intervalo(&self, id: &IdRepo, intervalo: &str) -> Result<(), ErrorGitea>;

    /// Crea un repo normal, vacío, sin `auto_init`.
    async fn crear_repo(
        &self,
        dueno: &Nombre,
        nombre: &Nombre,
        privado: bool,
    ) -> Result<RepoLocal, ErrorGitea>;

    /// Borra un repo. Idempotente: si no existe (404), `Ok(())`.
    async fn borrar_repo(&self, id: &IdRepo) -> Result<(), ErrorGitea>;

    /// Todas las organizaciones de esta instancia (`GET /api/v1/orgs`), paginando toda
    /// la colección. Usado por `crate::cuentas::usuarios_lan` para reconciliar los
    /// equipos de acceso LAN.
    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGitea>;

    /// Busca `nombre_equipo` en los equipos de `org`, sin crearlo. `None` si no existe.
    ///
    /// A diferencia de [`ApiGitea::asegurar_equipo`], nunca escribe nada: sirve para
    /// descubrir usuarios LAN ya conocidos sin crear equipos en organizaciones que nunca
    /// los han tenido (sin ningún usuario LAN no se crea ningún equipo).
    async fn buscar_equipo(
        &self,
        org: &Nombre,
        nombre_equipo: &str,
    ) -> Result<Option<u64>, ErrorGitea>;

    /// Asegura que `org` tiene el equipo descrito por `equipo`: lo busca por nombre y,
    /// si no existe, lo crea. Idempotente, incluida la carrera en la que otro proceso lo
    /// crea entre la búsqueda y la creación. Devuelve el id del equipo.
    async fn asegurar_equipo(
        &self,
        org: &Nombre,
        equipo: &DefinicionEquipo,
    ) -> Result<u64, ErrorGitea>;

    /// Miembros del equipo `id`, paginando toda la colección.
    async fn miembros_equipo(&self, id: u64) -> Result<Vec<Nombre>, ErrorGitea>;

    /// Añade `usuario` al equipo `id`. Idempotente.
    async fn anadir_miembro_equipo(&self, id: u64, usuario: &Nombre) -> Result<(), ErrorGitea>;

    /// Quita a `usuario` del equipo `id`. Idempotente: si no era miembro (404), `Ok(())`.
    async fn quitar_miembro_equipo(&self, id: u64, usuario: &Nombre) -> Result<(), ErrorGitea>;
}
