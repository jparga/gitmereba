//! Formas del JSON de la API de Gitea: lectura de respuestas y construcción de cuerpos.

use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::modelo::{IdRepo, Nombre, RepoLocal};

use super::error::ErrorGitea;
use super::peticion::PeticionMirror;

/// Fecha que Gitea usa para «sin sincronizar todavía».
const FECHA_CERO: &str = "0001-01-01T00:00:00Z";

#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaVersion {
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaError {
    pub message: Option<String>,
}

/// Repo tal como lo describe la API de Gitea (solo los campos que usamos).
#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaRepo {
    pub name: String,
    pub owner: RespuestaPropietario,
    pub mirror: bool,
    pub empty: bool,
    pub private: bool,
    pub size: u64,
    #[serde(default)]
    pub mirror_updated: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaPropietario {
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// Organización tal como la describe `GET /api/v1/orgs` (solo el campo que usamos).
#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaOrganizacion {
    pub username: String,
}

/// Equipo tal como lo describen `GET /orgs/{org}/teams` y `POST /orgs/{org}/teams`.
#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaEquipo {
    pub id: u64,
    pub name: String,
}

/// Miembro de un equipo, tal como lo describe `GET /teams/{id}/members`.
#[derive(Debug, Deserialize)]
pub(crate) struct RespuestaMiembroEquipo {
    pub login: String,
}

impl RespuestaRepo {
    /// Convierte la respuesta de Gitea al tipo de dominio, validando nombres y fecha.
    pub(crate) fn a_repo_local(self) -> Result<RepoLocal, ErrorGitea> {
        let dueno_str = self
            .owner
            .login
            .or(self.owner.username)
            .ok_or_else(|| ErrorGitea::DatosInvalidos("repo sin propietario".to_string()))?;
        let dueno = Nombre::nuevo(dueno_str)
            .map_err(|e| ErrorGitea::DatosInvalidos(format!("propietario inválido: {e}")))?;
        let nombre = Nombre::nuevo(self.name)
            .map_err(|e| ErrorGitea::DatosInvalidos(format!("nombre de repo inválido: {e}")))?;
        let ultima_sync = match self.mirror_updated.as_deref() {
            None | Some("") | Some(FECHA_CERO) => None,
            Some(fecha) => Some(OffsetDateTime::parse(fecha, &Rfc3339).map_err(|e| {
                ErrorGitea::DatosInvalidos(format!("fecha de mirror inválida: {e}"))
            })?),
        };
        Ok(RepoLocal {
            id: IdRepo { dueno, nombre },
            es_mirror: self.mirror,
            vacio: self.empty,
            privado: self.private,
            tamano_kb: self.size,
            ultima_sync,
        })
    }
}

/// Cuerpo de `POST /api/v1/repos/migrate`. Se construye aquí, y no con `#[derive(Serialize)]`
/// en `PeticionMirror`, para que el token nunca pase por un tipo que alguien pueda `Debug`.
pub(crate) fn cuerpo_migrate(peticion: &PeticionMirror) -> Value {
    let mut cuerpo = json!({
        "clone_addr": peticion.url_clon,
        "repo_owner": peticion.dueno.as_str(),
        "repo_name": peticion.nombre.as_str(),
        "mirror": true,
        "mirror_interval": peticion.intervalo,
        "private": peticion.privado,
        "wiki": true,
        "lfs": true,
        "service": servicio_de(&peticion.url_clon),
    });
    // El token de GitHub solo viaja si el origen es exactamente `github.com` por https y
    // sin puerto: con cualquier otro host se omite (el mirror se intenta sin credenciales)
    // para no entregarle a un tercero un token que no es suyo.
    if let Some(token) = peticion
        .token
        .as_ref()
        .filter(|_| es_github(&peticion.url_clon))
    {
        cuerpo["auth_token"] = Value::String(token.exponer().to_string());
    }
    if let Some(descripcion) = &peticion.descripcion {
        cuerpo["description"] = Value::String(descripcion.clone());
    }
    cuerpo
}

/// Servicio de migración de Gitea para `url_clon`.
///
/// Con `github`, Gitea deriva la URL de la API a partir del origen (y así trae también la
/// wiki); con cualquier otro origen eso falla antes de clonar, así que se usa `git`.
fn servicio_de(url_clon: &str) -> &'static str {
    if es_github(url_clon) { "github" } else { "git" }
}

/// `true` solo para `https://github.com/...` sin credenciales embebidas ni puerto
/// explícito (el 443 por defecto cuenta como sin puerto).
fn es_github(url_clon: &str) -> bool {
    url::Url::parse(url_clon).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

/// Extrae `message` del JSON de error de Gitea, si el cuerpo es JSON válido.
pub(crate) fn extraer_mensaje(cuerpo: &str) -> Option<String> {
    serde_json::from_str::<RespuestaError>(cuerpo)
        .ok()
        .and_then(|e| e.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretos::Secreto;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).unwrap()
    }

    #[test]
    fn repo_con_fecha_cero_no_tiene_ultima_sync() {
        let respuesta = RespuestaRepo {
            name: "demo".to_string(),
            owner: RespuestaPropietario {
                login: Some("acme".to_string()),
                username: None,
            },
            mirror: true,
            empty: false,
            private: true,
            size: 10,
            mirror_updated: Some(FECHA_CERO.to_string()),
        };
        let local = respuesta.a_repo_local().unwrap();
        assert_eq!(local.ultima_sync, None);
    }

    #[test]
    fn repo_usa_username_si_no_hay_login() {
        let respuesta = RespuestaRepo {
            name: "demo".to_string(),
            owner: RespuestaPropietario {
                login: None,
                username: Some("acme".to_string()),
            },
            mirror: false,
            empty: true,
            private: false,
            size: 0,
            mirror_updated: None,
        };
        let local = respuesta.a_repo_local().unwrap();
        assert_eq!(local.id.dueno, nombre("acme"));
    }

    #[test]
    fn cuerpo_migrate_incluye_los_campos_de_mirror() {
        let peticion = PeticionMirror {
            url_clon: "https://github.com/acme/demo.git".to_string(),
            token: Some(Secreto::nuevo("ghp_x")),
            dueno: nombre("acme"),
            nombre: nombre("demo"),
            intervalo: "30m".to_string(),
            privado: true,
            descripcion: Some("réplica".to_string()),
        };
        let cuerpo = cuerpo_migrate(&peticion);
        assert_eq!(cuerpo["mirror"], true);
        assert_eq!(cuerpo["wiki"], true);
        assert_eq!(cuerpo["lfs"], true);
        assert_eq!(cuerpo["service"], "github");
        assert_eq!(cuerpo["auth_token"], "ghp_x");
    }

    #[test]
    fn cuerpo_migrate_no_lleva_token_si_el_origen_no_es_github() {
        for otro in [
            "https://gitlab.com/acme/demo.git",
            "https://github.com.evil.example/acme/demo.git",
            "https://github.com:8443/acme/demo.git",
            "http://github.com/acme/demo.git",
            "https://u:p@github.com/acme/demo.git",
        ] {
            let peticion = PeticionMirror {
                url_clon: otro.to_string(),
                token: Some(Secreto::nuevo("ghp_x")),
                dueno: nombre("acme"),
                nombre: nombre("demo"),
                intervalo: "30m".to_string(),
                privado: true,
                descripcion: None,
            };
            let cuerpo = cuerpo_migrate(&peticion);
            assert!(cuerpo.get("auth_token").is_none(), "{otro}");
            assert_eq!(cuerpo["service"], "git", "{otro}");
        }
    }

    #[test]
    fn extraer_mensaje_lee_el_campo_message() {
        assert_eq!(
            extraer_mensaje(r#"{"message":"boom"}"#),
            Some("boom".to_string())
        );
        assert_eq!(extraer_mensaje("no es json"), None);
    }

    #[test]
    fn el_servicio_es_github_solo_para_github_por_https() {
        assert_eq!(servicio_de("https://github.com/jparga/x.git"), "github");
        for otro in [
            "http://github.com/jparga/x.git",
            "https://github.com.evil.example/x.git",
            "https://gitlab.com/a/b.git",
            "/tmp/origen/repo.git",
            "file:///tmp/origen/repo.git",
        ] {
            assert_eq!(servicio_de(otro), "git", "{otro}");
        }
    }
}
