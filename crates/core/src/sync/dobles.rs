//! Dobles en memoria de `ApiGithub` y `ApiGitea`, y pequeños constructores para las
//! pruebas de `sync`. Todo este fichero se compila solo con `cfg(test)` (ver `mod.rs`).

use std::collections::HashMap;
use std::sync::Mutex;

use time::OffsetDateTime;

use crate::gitea::{ApiGitea, DefinicionEquipo, ErrorGitea, PeticionMirror};
use crate::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use crate::modelo::{IdRepo, Nombre, RepoLocal, RepoOrigen};

fn minusculas(nombre: &Nombre) -> String {
    nombre.as_str().to_lowercase()
}

fn clave(id: &IdRepo) -> (String, String) {
    (minusculas(&id.dueno), minusculas(&id.nombre))
}

/// Un fallo programado para una llamada de escritura del doble de Gitea.
#[derive(Debug, Clone)]
pub enum FalloProgramado {
    YaExiste,
    Otro(String),
}

/// Doble en memoria de [`ApiGithub`]. Sin identidad programada, `identidad()` falla como
/// un token inválido, para que un test que se olvide de programarla falle pronto.
#[derive(Debug, Default)]
pub struct GithubDoble {
    identidad: Mutex<Option<Identidad>>,
    repos_usuario: Mutex<Vec<RepoOrigen>>,
    repos_organizacion: Mutex<HashMap<String, Vec<RepoOrigen>>>,
    fallo_repos_usuario: Mutex<bool>,
    fallo_organizaciones: Mutex<Vec<String>>,
    /// Organizaciones cuyo listado se ha pedido, para comprobar qué se consultó.
    pub llamadas_repos_de_organizacion: Mutex<Vec<Nombre>>,
}

impl GithubDoble {
    pub fn con_identidad(login: &str, caduca: Option<OffsetDateTime>) -> Self {
        let doble = Self::default();
        doble.identidad.lock().unwrap().replace(Identidad {
            login: Nombre::nuevo(login).unwrap(),
            scopes: Vec::new(),
            caduca,
        });
        doble
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

    pub fn con_fallo_repos_usuario(&self) {
        *self.fallo_repos_usuario.lock().unwrap() = true;
    }

    pub fn con_fallo_organizacion(&self, org: &str) {
        self.fallo_organizaciones
            .lock()
            .unwrap()
            .push(org.to_lowercase());
    }
}

impl ApiGithub for GithubDoble {
    async fn identidad(&self) -> Result<Identidad, ErrorGithub> {
        self.identidad
            .lock()
            .unwrap()
            .clone()
            .ok_or(ErrorGithub::TokenInvalido)
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGithub> {
        Ok(Vec::new())
    }

    async fn repos_de_usuario(&self) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        if *self.fallo_repos_usuario.lock().unwrap() {
            return Err(ErrorGithub::Red("fallo simulado de red".to_string()));
        }
        Ok(self.repos_usuario.lock().unwrap().clone())
    }

    async fn repos_de_organizacion(&self, org: &Nombre) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        self.llamadas_repos_de_organizacion
            .lock()
            .unwrap()
            .push(org.clone());
        let k = minusculas(org);
        if self.fallo_organizaciones.lock().unwrap().contains(&k) {
            return Err(ErrorGithub::NoEncontrado);
        }
        Ok(self
            .repos_organizacion
            .lock()
            .unwrap()
            .get(&k)
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

/// Doble en memoria de [`ApiGitea`]. `borrar_repo` entra en pánico si se llama: `sync`
/// nunca debe borrar un repo, así que el pánico es la propia comprobación.
#[derive(Debug, Default)]
pub struct GiteaDoble {
    pub organizaciones_aseguradas: Mutex<Vec<Nombre>>,
    fallo_asegurar_organizacion: Mutex<Vec<String>>,
    repos_por_dueno: Mutex<HashMap<String, Vec<RepoLocal>>>,
    fallo_repos_de: Mutex<Vec<String>>,
    pub llamadas_crear_mirror: Mutex<Vec<PeticionMirror>>,
    fallo_crear_mirror: Mutex<HashMap<(String, String), FalloProgramado>>,
    pub llamadas_fijar_intervalo: Mutex<Vec<(IdRepo, String)>>,
    fallo_fijar_intervalo: Mutex<Vec<(String, String)>>,
    pub llamadas_sincronizar_mirror: Mutex<Vec<IdRepo>>,
}

impl GiteaDoble {
    pub fn con_repos(&self, dueno: &str, repos: Vec<RepoLocal>) {
        self.repos_por_dueno
            .lock()
            .unwrap()
            .insert(dueno.to_lowercase(), repos);
    }

    pub fn con_fallo_repos_de(&self, dueno: &str) {
        self.fallo_repos_de
            .lock()
            .unwrap()
            .push(dueno.to_lowercase());
    }

    pub fn con_fallo_asegurar_organizacion(&self, dueno: &str) {
        self.fallo_asegurar_organizacion
            .lock()
            .unwrap()
            .push(dueno.to_lowercase());
    }

    pub fn con_fallo_crear_mirror(&self, id: &IdRepo, fallo: FalloProgramado) {
        self.fallo_crear_mirror
            .lock()
            .unwrap()
            .insert(clave(id), fallo);
    }

    pub fn con_fallo_fijar_intervalo(&self, id: &IdRepo) {
        self.fallo_fijar_intervalo.lock().unwrap().push(clave(id));
    }
}

impl ApiGitea for GiteaDoble {
    async fn salud(&self) -> Result<bool, ErrorGitea> {
        Ok(true)
    }

    async fn version(&self) -> Result<String, ErrorGitea> {
        Ok("doble".to_string())
    }

    async fn asegurar_organizacion(&self, nombre: &Nombre) -> Result<(), ErrorGitea> {
        self.organizaciones_aseguradas
            .lock()
            .unwrap()
            .push(nombre.clone());
        if self
            .fallo_asegurar_organizacion
            .lock()
            .unwrap()
            .contains(&minusculas(nombre))
        {
            return Err(ErrorGitea::SinPermiso);
        }
        Ok(())
    }

    async fn repos_de(&self, dueno: &Nombre) -> Result<Vec<RepoLocal>, ErrorGitea> {
        let k = minusculas(dueno);
        if self.fallo_repos_de.lock().unwrap().contains(&k) {
            return Err(ErrorGitea::NoDisponible);
        }
        Ok(self
            .repos_por_dueno
            .lock()
            .unwrap()
            .get(&k)
            .cloned()
            .unwrap_or_default())
    }

    async fn repo(&self, id: &IdRepo) -> Result<Option<RepoLocal>, ErrorGitea> {
        let k = minusculas(&id.dueno);
        Ok(self
            .repos_por_dueno
            .lock()
            .unwrap()
            .get(&k)
            .and_then(|repos| repos.iter().find(|r| r.id == *id).cloned()))
    }

    async fn crear_mirror(&self, peticion: &PeticionMirror) -> Result<RepoLocal, ErrorGitea> {
        let k = (
            peticion.dueno.as_str().to_lowercase(),
            peticion.nombre.as_str().to_lowercase(),
        );
        self.llamadas_crear_mirror
            .lock()
            .unwrap()
            .push(peticion.clone());
        match self.fallo_crear_mirror.lock().unwrap().get(&k) {
            Some(FalloProgramado::YaExiste) => return Err(ErrorGitea::YaExiste),
            Some(FalloProgramado::Otro(detalle)) => {
                return Err(ErrorGitea::RespuestaInesperada {
                    estado: 500,
                    detalle: detalle.clone(),
                });
            }
            None => {}
        }
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
        self.llamadas_sincronizar_mirror
            .lock()
            .unwrap()
            .push(id.clone());
        Ok(())
    }

    async fn fijar_intervalo(&self, id: &IdRepo, intervalo: &str) -> Result<(), ErrorGitea> {
        self.llamadas_fijar_intervalo
            .lock()
            .unwrap()
            .push((id.clone(), intervalo.to_string()));
        if self
            .fallo_fijar_intervalo
            .lock()
            .unwrap()
            .contains(&clave(id))
        {
            return Err(ErrorGitea::NoDisponible);
        }
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

    async fn borrar_repo(&self, _id: &IdRepo) -> Result<(), ErrorGitea> {
        panic!("sync nunca debe llamar a borrar_repo: un huérfano se pausa, no se borra");
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGitea> {
        Ok(Vec::new())
    }

    async fn buscar_equipo(
        &self,
        _org: &Nombre,
        _nombre_equipo: &str,
    ) -> Result<Option<u64>, ErrorGitea> {
        Ok(None)
    }

    async fn asegurar_equipo(
        &self,
        _org: &Nombre,
        _equipo: &DefinicionEquipo,
    ) -> Result<u64, ErrorGitea> {
        Ok(0)
    }

    async fn miembros_equipo(&self, _id: u64) -> Result<Vec<Nombre>, ErrorGitea> {
        Ok(Vec::new())
    }

    async fn anadir_miembro_equipo(&self, _id: u64, _usuario: &Nombre) -> Result<(), ErrorGitea> {
        Ok(())
    }

    async fn quitar_miembro_equipo(&self, _id: u64, _usuario: &Nombre) -> Result<(), ErrorGitea> {
        Ok(())
    }
}

/// Constructores breves para los tipos de dominio, usados solo en las pruebas de `sync`.
pub mod pruebas {
    use std::path::PathBuf;

    use crate::modelo::{Alcance, Cuenta, IdRepo, Nombre, RepoLocal, RepoOrigen};

    pub fn nombre(valor: &str) -> Nombre {
        Nombre::nuevo(valor).unwrap()
    }

    pub fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    pub fn cuenta(
        login: &str,
        incluir_forks: bool,
        organizaciones: &[&str],
        excluidos: &[IdRepo],
        intervalo_minutos: u32,
    ) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: PathBuf::from("/tmp/gitmereba-test"),
            puerto: 3999,
            intervalo_minutos,
            alcance: Alcance {
                incluir_forks,
                organizaciones: organizaciones.iter().map(|o| nombre(o)).collect(),
                excluidos: excluidos.to_vec(),
            },
            lan: None,
        }
    }

    pub fn repo_origen(dueno: &str, nombre_repo: &str, privado: bool, es_fork: bool) -> RepoOrigen {
        RepoOrigen {
            id: id(dueno, nombre_repo),
            url_clon: format!("https://github.com/{dueno}/{nombre_repo}.git"),
            privado,
            es_fork,
            archivado: false,
            rama_por_defecto: Some("main".to_string()),
            descripcion: None,
            tamano_kb: 10,
        }
    }

    pub fn repo_local(dueno: &str, nombre_repo: &str, es_mirror: bool) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre_repo),
            es_mirror,
            vacio: false,
            privado: true,
            tamano_kb: 10,
            ultima_sync: None,
        }
    }

    pub fn repo_local_no_mirror(dueno: &str, nombre_repo: &str) -> RepoLocal {
        repo_local(dueno, nombre_repo, false)
    }
}
