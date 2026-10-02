//! Doble en memoria de `ApiGitea`, solo para las pruebas unitarias de este módulo. El
//! comportamiento contra un Gitea real (incluido `git push`, que este doble no puede
//! simular) se prueba aparte, en `crates/core/tests/contingencia_gitea_real.rs`.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Mutex;

use crate::gitea::{ApiGitea, DefinicionEquipo, ErrorGitea, PeticionMirror};
use crate::modelo::{IdRepo, Nombre, RepoLocal};

fn clave(id: &IdRepo) -> String {
    format!(
        "{}/{}",
        id.dueno.as_str().to_lowercase(),
        id.nombre.as_str().to_lowercase()
    )
}

#[derive(Default)]
struct Interior {
    repos: HashMap<String, RepoLocal>,
    intervalos: Vec<(IdRepo, String)>,
    organizaciones: Vec<Nombre>,
    borrados: Vec<IdRepo>,
}

/// Doble de [`ApiGitea`] con estado en memoria: recuerda los repos que se le den con
/// [`GiteaDoble::con_repo`] y los que `crear_repo` vaya creando, para poder probar la
/// idempotencia de `activar` sin hablar con ningún Gitea real.
#[derive(Default)]
pub struct GiteaDoble(Mutex<Interior>);

impl GiteaDoble {
    pub fn nueva() -> Self {
        Self::default()
    }

    pub fn con_repo(&self, repo: RepoLocal) -> &Self {
        self.0.lock().unwrap().repos.insert(clave(&repo.id), repo);
        self
    }

    pub fn intervalos_fijados(&self) -> Vec<(IdRepo, String)> {
        self.0.lock().unwrap().intervalos.clone()
    }

    pub fn organizaciones_aseguradas(&self) -> Vec<Nombre> {
        self.0.lock().unwrap().organizaciones.clone()
    }

    pub fn borrados(&self) -> Vec<IdRepo> {
        self.0.lock().unwrap().borrados.clone()
    }
}

impl ApiGitea for GiteaDoble {
    async fn salud(&self) -> Result<bool, ErrorGitea> {
        Ok(true)
    }

    async fn version(&self) -> Result<String, ErrorGitea> {
        Ok("doble-de-pruebas".to_string())
    }

    async fn asegurar_organizacion(&self, nombre: &Nombre) -> Result<(), ErrorGitea> {
        self.0.lock().unwrap().organizaciones.push(nombre.clone());
        Ok(())
    }

    async fn repos_de(&self, dueno: &Nombre) -> Result<Vec<RepoLocal>, ErrorGitea> {
        let prefijo = format!("{}/", dueno.as_str().to_lowercase());
        Ok(self
            .0
            .lock()
            .unwrap()
            .repos
            .values()
            .filter(|repo| clave(&repo.id).starts_with(&prefijo))
            .cloned()
            .collect())
    }

    async fn repo(&self, id: &IdRepo) -> Result<Option<RepoLocal>, ErrorGitea> {
        Ok(self.0.lock().unwrap().repos.get(&clave(id)).cloned())
    }

    async fn crear_mirror(&self, _peticion: &PeticionMirror) -> Result<RepoLocal, ErrorGitea> {
        panic!("este doble no crea mirrors: `contingencia` nunca los crea")
    }

    async fn sincronizar_mirror(&self, _id: &IdRepo) -> Result<(), ErrorGitea> {
        Ok(())
    }

    async fn fijar_intervalo(&self, id: &IdRepo, intervalo: &str) -> Result<(), ErrorGitea> {
        self.0
            .lock()
            .unwrap()
            .intervalos
            .push((id.clone(), intervalo.to_string()));
        Ok(())
    }

    async fn crear_repo(
        &self,
        dueno: &Nombre,
        nombre: &Nombre,
        privado: bool,
    ) -> Result<RepoLocal, ErrorGitea> {
        let repo = RepoLocal {
            id: IdRepo {
                dueno: dueno.clone(),
                nombre: nombre.clone(),
            },
            es_mirror: false,
            vacio: true,
            privado,
            tamano_kb: 0,
            ultima_sync: None,
        };
        self.0
            .lock()
            .unwrap()
            .repos
            .insert(clave(&repo.id), repo.clone());
        Ok(repo)
    }

    async fn borrar_repo(&self, id: &IdRepo) -> Result<(), ErrorGitea> {
        let mut interior = self.0.lock().unwrap();
        interior.repos.remove(&clave(id));
        interior.borrados.push(id.clone());
        Ok(())
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGitea> {
        Ok(self.0.lock().unwrap().organizaciones.clone())
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
