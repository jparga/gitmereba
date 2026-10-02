//! Dobles en memoria de `ApiGithub`, `ApiGitea`, `Lanzador` y `Aprovisionador`, para las
//! pruebas de `cuentas`. Todo este fichero se compila solo con `cfg(test)` (ver `mod.rs`).
//!
//! No se reutilizan los de `crate::sync::dobles`: ese módulo también es `#[cfg(test)]`,
//! así que no está disponible fuera de las pruebas de `sync` (ni siquiera dentro del
//! mismo crate).

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;

use crate::config::Rutas;
use crate::gitea::{ApiGitea, DefinicionEquipo, ErrorGitea, PeticionMirror};
use crate::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use crate::instancia::{ParametrosProvision, ResultadoProvision};
use crate::modelo::{IdRepo, Nombre, RepoLocal, RepoOrigen};
use crate::secretos::{ClaveSecreto, Secreto};

use super::aprovisionador::Aprovisionador;
use super::error::ErrorCuentas;
use super::lanzador::{Lanzador, ParametrosLanzamiento};

fn nombre(v: &str) -> Nombre {
    Nombre::nuevo(v).expect("nombre de prueba válido")
}

/// Doble en memoria de [`ApiGithub`].
#[derive(Default)]
pub struct GithubDoble {
    identidad: Mutex<Option<Identidad>>,
    organizaciones: Mutex<Vec<Nombre>>,
    repos_usuario: Mutex<Vec<RepoOrigen>>,
    repos_organizacion: Mutex<HashMap<String, Vec<RepoOrigen>>>,
    llamadas: Mutex<Vec<&'static str>>,
}

impl GithubDoble {
    pub fn con_identidad(login: &str, caduca: Option<OffsetDateTime>) -> Self {
        let doble = Self::default();
        doble.identidad.lock().unwrap().replace(Identidad {
            login: nombre(login),
            scopes: Vec::new(),
            caduca,
        });
        doble
    }

    /// Organizaciones a las que pertenece la cuenta, tal como las devolvería
    /// `GET /user/orgs`. Vacío por defecto.
    pub fn con_organizaciones(&self, organizaciones: &[&str]) {
        *self.organizaciones.lock().unwrap() = organizaciones.iter().map(|o| nombre(o)).collect();
    }

    pub fn con_repos_usuario(&self, repos: Vec<RepoOrigen>) {
        *self.repos_usuario.lock().unwrap() = repos;
    }

    pub fn con_repos_organizacion(&self, org: &str, repos: Vec<RepoOrigen>) {
        self.repos_organizacion
            .lock()
            .unwrap()
            .insert(org.to_lowercase(), repos);
    }

    pub fn repo(dueno: &str, nombre_repo: &str, privado: bool, es_fork: bool) -> RepoOrigen {
        RepoOrigen {
            id: IdRepo {
                dueno: nombre(dueno),
                nombre: nombre(nombre_repo),
            },
            url_clon: format!("https://github.com/{dueno}/{nombre_repo}.git"),
            privado,
            es_fork,
            archivado: false,
            rama_por_defecto: Some("main".to_string()),
            descripcion: None,
            tamano_kb: 10,
        }
    }

    pub fn repo_con_tamano(dueno: &str, nombre_repo: &str, tamano_kb: u64) -> RepoOrigen {
        let mut repo = Self::repo(dueno, nombre_repo, false, false);
        repo.tamano_kb = tamano_kb;
        repo
    }

    /// Nombres de los métodos llamados, en orden: para comprobar, p. ej., que un Gitea
    /// parado impide llegar a tocar GitHub.
    pub fn llamadas(&self) -> Vec<&'static str> {
        self.llamadas.lock().unwrap().clone()
    }
}

impl ApiGithub for GithubDoble {
    async fn identidad(&self) -> Result<Identidad, ErrorGithub> {
        self.llamadas.lock().unwrap().push("identidad");
        self.identidad
            .lock()
            .unwrap()
            .clone()
            .ok_or(ErrorGithub::TokenInvalido)
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGithub> {
        self.llamadas.lock().unwrap().push("organizaciones");
        Ok(self.organizaciones.lock().unwrap().clone())
    }

    async fn repos_de_usuario(&self) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        self.llamadas.lock().unwrap().push("repos_de_usuario");
        Ok(self.repos_usuario.lock().unwrap().clone())
    }

    async fn repos_de_organizacion(&self, org: &Nombre) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        self.llamadas.lock().unwrap().push("repos_de_organizacion");
        Ok(self
            .repos_organizacion
            .lock()
            .unwrap()
            .get(&org.as_str().to_lowercase())
            .cloned()
            .unwrap_or_default())
    }

    async fn sha_de_rama(
        &self,
        _repo: &IdRepo,
        _rama: &str,
    ) -> Result<Option<String>, ErrorGithub> {
        Ok(None)
    }

    async fn estado_servicio(&self) -> Result<EstadoServicio, ErrorGithub> {
        Ok(EstadoServicio::Operativo)
    }
}

/// Estado interior de [`GiteaDoble`], compartido a través de un `Arc` para que la
/// fábrica que usa `cuentas::alta` pueda devolver una copia barata que siga viendo (y
/// dejando ver a las pruebas) las mismas llamadas.
#[derive(Default)]
struct GiteaDobleInterior {
    salud: Mutex<bool>,
    repos_por_dueno: Mutex<HashMap<String, Vec<RepoLocal>>>,
    llamadas_crear_mirror: Mutex<Vec<PeticionMirror>>,
    llamadas_fijar_intervalo: Mutex<Vec<(IdRepo, String)>>,
    llamadas_sincronizar_mirror: Mutex<Vec<IdRepo>>,
    borrados: Mutex<Vec<IdRepo>>,
    /// Organizaciones de esta instancia, tal como las vería `GET /api/v1/orgs`.
    organizaciones: Mutex<Vec<Nombre>>,
    /// `(organización en minúsculas, nombre de equipo)` → id del equipo.
    equipos: Mutex<HashMap<(String, String), u64>>,
    /// id de equipo → sus miembros.
    miembros_equipo: Mutex<HashMap<u64, Vec<Nombre>>>,
    siguiente_id_equipo: Mutex<u64>,
}

/// Doble en memoria de [`ApiGitea`]. `salud()` es `true` por defecto en
/// [`GiteaDoble::nueva`] (la mayoría de las pruebas quieren un Gitea arriba).
#[derive(Default, Clone)]
pub struct GiteaDoble(Arc<GiteaDobleInterior>);

impl GiteaDoble {
    pub fn nueva() -> Self {
        let doble = Self::default();
        *doble.0.salud.lock().unwrap() = true;
        doble
    }

    pub fn con_salud(&self, salud: bool) -> &Self {
        *self.0.salud.lock().unwrap() = salud;
        self
    }

    pub fn con_repos(&self, dueno: &str, repos: Vec<RepoLocal>) -> &Self {
        self.0
            .repos_por_dueno
            .lock()
            .unwrap()
            .insert(dueno.to_lowercase(), repos);
        self
    }

    pub fn llamadas_crear_mirror(&self) -> Vec<PeticionMirror> {
        self.0.llamadas_crear_mirror.lock().unwrap().clone()
    }

    pub fn llamadas_fijar_intervalo(&self) -> Vec<(IdRepo, String)> {
        self.0.llamadas_fijar_intervalo.lock().unwrap().clone()
    }

    pub fn llamadas_sincronizar_mirror(&self) -> Vec<IdRepo> {
        self.0.llamadas_sincronizar_mirror.lock().unwrap().clone()
    }

    pub fn borrados(&self) -> Vec<IdRepo> {
        self.0.borrados.lock().unwrap().clone()
    }

    /// Organizaciones que devuelve `organizaciones()`, tal como las vería
    /// `GET /api/v1/orgs`. Vacío por defecto.
    pub fn con_organizaciones(&self, organizaciones: &[&str]) -> &Self {
        *self.0.organizaciones.lock().unwrap() = organizaciones.iter().map(|o| nombre(o)).collect();
        self
    }

    /// Miembros actuales del equipo `nombre_equipo` de `org`, o `None` si ese equipo no
    /// se ha creado todavía. Para comprobar el resultado de `reconciliar_permisos_lan` en
    /// las pruebas sin pasar por la asincronía de `miembros_equipo`.
    pub fn miembros_de(&self, org: &str, nombre_equipo: &str) -> Option<Vec<Nombre>> {
        let clave = (org.to_lowercase(), nombre_equipo.to_string());
        let id = *self.0.equipos.lock().unwrap().get(&clave)?;
        Some(
            self.0
                .miembros_equipo
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .unwrap_or_default(),
        )
    }
}

impl ApiGitea for GiteaDoble {
    async fn salud(&self) -> Result<bool, ErrorGitea> {
        Ok(*self.0.salud.lock().unwrap())
    }

    async fn version(&self) -> Result<String, ErrorGitea> {
        Ok("doble-de-pruebas".to_string())
    }

    async fn asegurar_organizacion(&self, _nombre: &Nombre) -> Result<(), ErrorGitea> {
        Ok(())
    }

    async fn repos_de(&self, dueno: &Nombre) -> Result<Vec<RepoLocal>, ErrorGitea> {
        Ok(self
            .0
            .repos_por_dueno
            .lock()
            .unwrap()
            .get(&dueno.as_str().to_lowercase())
            .cloned()
            .unwrap_or_default())
    }

    async fn repo(&self, id: &IdRepo) -> Result<Option<RepoLocal>, ErrorGitea> {
        Ok(self
            .0
            .repos_por_dueno
            .lock()
            .unwrap()
            .get(&id.dueno.as_str().to_lowercase())
            .and_then(|repos| repos.iter().find(|r| r.id == *id).cloned()))
    }

    async fn crear_mirror(&self, peticion: &PeticionMirror) -> Result<RepoLocal, ErrorGitea> {
        self.0
            .llamadas_crear_mirror
            .lock()
            .unwrap()
            .push(peticion.clone());
        Ok(RepoLocal {
            id: IdRepo {
                dueno: peticion.dueno.clone(),
                nombre: peticion.nombre.clone(),
            },
            es_mirror: true,
            vacio: false,
            privado: peticion.privado,
            tamano_kb: 0,
            ultima_sync: None,
        })
    }

    async fn sincronizar_mirror(&self, id: &IdRepo) -> Result<(), ErrorGitea> {
        self.0
            .llamadas_sincronizar_mirror
            .lock()
            .unwrap()
            .push(id.clone());
        Ok(())
    }

    async fn fijar_intervalo(&self, id: &IdRepo, intervalo: &str) -> Result<(), ErrorGitea> {
        self.0
            .llamadas_fijar_intervalo
            .lock()
            .unwrap()
            .push((id.clone(), intervalo.to_string()));
        Ok(())
    }

    async fn crear_repo(
        &self,
        dueno: &Nombre,
        nombre: &Nombre,
        privado: bool,
    ) -> Result<RepoLocal, ErrorGitea> {
        Ok(RepoLocal {
            id: IdRepo {
                dueno: dueno.clone(),
                nombre: nombre.clone(),
            },
            es_mirror: false,
            vacio: true,
            privado,
            tamano_kb: 0,
            ultima_sync: None,
        })
    }

    /// Solo se admite borrar un mirror vacío que nunca llegó a clonarse (ver
    /// `cuentas::sincronizar_repo_con`). Cualquier otro borrado rompe la regla de que
    /// `cuentas` nunca destruye contenido: un huérfano se pausa, no se borra.
    async fn borrar_repo(&self, id: &IdRepo) -> Result<(), ErrorGitea> {
        let mut repos = self.0.repos_por_dueno.lock().unwrap();
        let del_dueno = repos
            .get_mut(&id.dueno.as_str().to_lowercase())
            .expect("borrar_repo de un dueño desconocido");
        let posicion = del_dueno
            .iter()
            .position(|r| r.id == *id)
            .expect("borrar_repo de un repo desconocido");
        let repo = &del_dueno[posicion];
        assert!(
            repo.vacio && repo.ultima_sync.is_none(),
            "cuentas nunca debe borrar un repo con contenido: {id}"
        );
        del_dueno.remove(posicion);
        self.0.borrados.lock().unwrap().push(id.clone());
        Ok(())
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGitea> {
        Ok(self.0.organizaciones.lock().unwrap().clone())
    }

    async fn buscar_equipo(
        &self,
        org: &Nombre,
        nombre_equipo: &str,
    ) -> Result<Option<u64>, ErrorGitea> {
        let clave = (org.as_str().to_lowercase(), nombre_equipo.to_string());
        Ok(self.0.equipos.lock().unwrap().get(&clave).copied())
    }

    async fn asegurar_equipo(
        &self,
        org: &Nombre,
        equipo: &DefinicionEquipo,
    ) -> Result<u64, ErrorGitea> {
        if let Some(id) = self.buscar_equipo(org, equipo.nombre).await? {
            return Ok(id);
        }
        let clave = (org.as_str().to_lowercase(), equipo.nombre.to_string());
        let mut siguiente = self.0.siguiente_id_equipo.lock().unwrap();
        *siguiente += 1;
        let id = *siguiente;
        drop(siguiente);
        self.0.equipos.lock().unwrap().insert(clave, id);
        self.0
            .miembros_equipo
            .lock()
            .unwrap()
            .entry(id)
            .or_default();
        Ok(id)
    }

    async fn miembros_equipo(&self, id: u64) -> Result<Vec<Nombre>, ErrorGitea> {
        Ok(self
            .0
            .miembros_equipo
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .unwrap_or_default())
    }

    async fn anadir_miembro_equipo(&self, id: u64, usuario: &Nombre) -> Result<(), ErrorGitea> {
        let mut mapa = self.0.miembros_equipo.lock().unwrap();
        let miembros = mapa.entry(id).or_default();
        if !miembros.contains(usuario) {
            miembros.push(usuario.clone());
        }
        Ok(())
    }

    async fn quitar_miembro_equipo(&self, id: u64, usuario: &Nombre) -> Result<(), ErrorGitea> {
        if let Some(miembros) = self.0.miembros_equipo.lock().unwrap().get_mut(&id) {
            miembros.retain(|m| m != usuario);
        }
        Ok(())
    }
}

/// Doble de [`Lanzador`]: no arranca ni para nada real.
#[derive(Default)]
pub struct LanzadorDoble {
    pub arrancadas: Mutex<Vec<Nombre>>,
    pub paradas: Mutex<Vec<Nombre>>,
    pub fallar_arrancar: Mutex<bool>,
}

impl LanzadorDoble {
    pub fn con_fallo_al_arrancar(&self) -> &Self {
        *self.fallar_arrancar.lock().unwrap() = true;
        self
    }
}

impl Lanzador for LanzadorDoble {
    async fn arrancar(
        &self,
        _rutas: &Rutas,
        parametros: &ParametrosLanzamiento<'_>,
    ) -> Result<(), ErrorCuentas> {
        if *self.fallar_arrancar.lock().unwrap() {
            return Err(ErrorCuentas::Interno(
                "fallo simulado al arrancar Gitea".to_string(),
            ));
        }
        self.arrancadas
            .lock()
            .unwrap()
            .push(parametros.login.clone());
        Ok(())
    }

    async fn parar(&self, _rutas: &Rutas, login: &Nombre) -> Result<(), ErrorCuentas> {
        self.paradas.lock().unwrap().push(login.clone());
        Ok(())
    }
}

/// Doble de [`Aprovisionador`]: no descarga ni ejecuta ningún binario. Deja en el
/// llavero el token de Gitea que necesita el resto del alta, igual que haría
/// `instancia::provisionar` de verdad.
#[derive(Default)]
pub struct AprovisionadorDoble {
    pub fallar: Mutex<bool>,
}

impl AprovisionadorDoble {
    pub fn con_fallo(&self) -> &Self {
        *self.fallar.lock().unwrap() = true;
        self
    }
}

impl Aprovisionador for AprovisionadorDoble {
    async fn asegurar_binario(&self, _rutas: &Rutas) -> Result<PathBuf, ErrorCuentas> {
        Ok(PathBuf::from("/no-existe/gitea-doble-de-pruebas"))
    }

    async fn provisionar(
        &self,
        parametros: &ParametrosProvision<'_>,
    ) -> Result<ResultadoProvision, ErrorCuentas> {
        if *self.fallar.lock().unwrap() {
            return Err(ErrorCuentas::Interno(
                "fallo simulado al provisionar".to_string(),
            ));
        }
        parametros
            .secretos
            .guardar(
                parametros.login,
                ClaveSecreto::TokenGitea,
                &Secreto::nuevo("token-gitea-de-pruebas"),
            )
            .map_err(ErrorCuentas::from)?;
        Ok(ResultadoProvision {
            app_ini_generado: true,
            admin_creado: true,
            token_generado: true,
        })
    }
}
