//! Doble en memoria de [`ApiGithub`] y constructores breves para las pruebas de
//! `verificacion`. Solo se compila con `cfg(test)` (ver `mod.rs`).
//!
//! No reutiliza el doble de `sync` (`crate::sync::dobles`): es privado a ese módulo, así
//! que no es accesible desde aquí, y este doble solo necesita lo que usa
//! [`super::verificar_cuenta`].

use std::collections::HashMap;
use std::sync::Mutex;

use time::OffsetDateTime;

use crate::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use crate::modelo::{IdRepo, Nombre, RepoOrigen};

fn clave(id: &IdRepo) -> (String, String) {
    (
        id.dueno.as_str().to_lowercase(),
        id.nombre.as_str().to_lowercase(),
    )
}

/// Copia un [`ErrorGithub`] a mano: no implementa `Clone`.
fn clonar_error(error: &ErrorGithub) -> ErrorGithub {
    match error {
        ErrorGithub::TokenInvalido => ErrorGithub::TokenInvalido,
        ErrorGithub::SinPermiso => ErrorGithub::SinPermiso,
        ErrorGithub::LimiteDePeticiones { reinicio } => ErrorGithub::LimiteDePeticiones {
            reinicio: *reinicio,
        },
        ErrorGithub::NoEncontrado => ErrorGithub::NoEncontrado,
        ErrorGithub::Red(mensaje) => ErrorGithub::Red(mensaje.clone()),
        ErrorGithub::RespuestaInesperada { estado, detalle } => ErrorGithub::RespuestaInesperada {
            estado: *estado,
            detalle: detalle.clone(),
        },
        ErrorGithub::UrlNoPermitida => ErrorGithub::UrlNoPermitida,
        ErrorGithub::DatosInvalidos(mensaje) => ErrorGithub::DatosInvalidos(mensaje.clone()),
    }
}

/// Doble en memoria de [`ApiGithub`]: solo implementa lo que usa
/// [`super::verificar_cuenta`] (`identidad` y `sha_de_rama`); el resto de operaciones
/// devuelven listas vacías.
#[derive(Debug, Default)]
pub struct GithubDoble {
    identidad: Mutex<Option<Identidad>>,
    shas: Mutex<HashMap<(String, String), String>>,
    fallo_limite: Mutex<bool>,
    fallos: Mutex<HashMap<(String, String), ErrorGithub>>,
    /// Ids consultados, en el orden en que se han pedido: para comprobar priorización y
    /// el límite de consultas por pasada.
    pub llamadas_sha: Mutex<Vec<IdRepo>>,
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

    pub fn con_sha(&self, id: &IdRepo, sha: &str) {
        self.shas.lock().unwrap().insert(clave(id), sha.to_string());
    }

    /// A partir de ahora, cualquier consulta de SHA devuelve `LimiteDePeticiones`.
    pub fn con_fallo_de_limite(&self) {
        *self.fallo_limite.lock().unwrap() = true;
    }

    pub fn con_fallo(&self, id: &IdRepo, error: ErrorGithub) {
        self.fallos.lock().unwrap().insert(clave(id), error);
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
        Ok(Vec::new())
    }

    async fn repos_de_organizacion(&self, _org: &Nombre) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        Ok(Vec::new())
    }

    async fn sha_de_rama(&self, repo: &IdRepo, _rama: &str) -> Result<Option<String>, ErrorGithub> {
        self.llamadas_sha.lock().unwrap().push(repo.clone());
        if *self.fallo_limite.lock().unwrap() {
            return Err(ErrorGithub::LimiteDePeticiones { reinicio: None });
        }
        let k = clave(repo);
        if let Some(error) = self.fallos.lock().unwrap().get(&k) {
            return Err(clonar_error(error));
        }
        Ok(self.shas.lock().unwrap().get(&k).cloned())
    }

    async fn estado_servicio(&self) -> Result<EstadoServicio, ErrorGithub> {
        Ok(EstadoServicio::Operativo)
    }
}

/// Constructores breves para los tipos de dominio, usados solo en las pruebas de
/// `verificacion`.
pub mod pruebas {
    use std::path::PathBuf;

    use crate::modelo::{Alcance, Cuenta, IdRepo, Nombre, RepoLocal, RepoOrigen};
    use time::OffsetDateTime;

    pub fn nombre(valor: &str) -> Nombre {
        Nombre::nuevo(valor).unwrap()
    }

    pub fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    pub fn cuenta(login: &str, intervalo_minutos: u32, carpeta: impl Into<PathBuf>) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: carpeta.into(),
            puerto: 3999,
            intervalo_minutos,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: Vec::new(),
                excluidos: Vec::new(),
            },
            lan: None,
        }
    }

    pub fn repo_origen(
        dueno: &str,
        nombre_repo: &str,
        tamano_kb: u64,
        rama_por_defecto: Option<&str>,
    ) -> RepoOrigen {
        RepoOrigen {
            id: id(dueno, nombre_repo),
            url_clon: format!("https://github.com/{dueno}/{nombre_repo}.git"),
            privado: false,
            es_fork: false,
            archivado: false,
            rama_por_defecto: rama_por_defecto.map(str::to_string),
            descripcion: None,
            tamano_kb,
        }
    }

    pub fn repo_local(
        dueno: &str,
        nombre_repo: &str,
        vacio: bool,
        ultima_sync: Option<OffsetDateTime>,
    ) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre_repo),
            es_mirror: true,
            vacio,
            privado: true,
            tamano_kb: 10,
            ultima_sync,
        }
    }
}
