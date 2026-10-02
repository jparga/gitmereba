//! Formas JSON de la API de GitHub y su conversión al modelo de dominio.

use serde::Deserialize;
use url::Url;

use crate::modelo::{IdRepo, Nombre, RepoOrigen};

#[derive(Debug, Deserialize)]
pub(crate) struct UsuarioDto {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizacionDto {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PropietarioDto {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RepoDto {
    pub name: String,
    pub owner: PropietarioDto,
    pub clone_url: String,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub description: Option<String>,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CommitDto {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RamaDto {
    pub commit: CommitDto,
}

#[derive(Debug, Deserialize)]
pub(crate) struct IndicadorDto {
    pub indicator: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EstadoServicioDto {
    pub status: IndicadorDto,
}

/// `true` si la URL de clonado es https y no lleva credenciales embebidas (`user@host`).
fn url_clon_es_segura(valor: &str) -> bool {
    match Url::parse(valor) {
        Ok(url) => url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
        Err(_) => false,
    }
}

/// Convierte un repo de GitHub al modelo de dominio, o `None` si no es representable
/// de forma segura (nombre inválido o URL de clonado sospechosa). El motivo se registra
/// con `tracing::warn!` para no perder la incidencia sin detener el resto del listado.
pub(crate) fn convertir_repo(dto: RepoDto) -> Option<RepoOrigen> {
    let dueno = match Nombre::nuevo(dto.owner.login.as_str()) {
        Ok(dueno) => dueno,
        Err(motivo) => {
            tracing::warn!(repo = %dto.name, %motivo, "repo omitido: propietario con nombre inválido");
            return None;
        }
    };
    let nombre = match Nombre::nuevo(dto.name.as_str()) {
        Ok(nombre) => nombre,
        Err(motivo) => {
            tracing::warn!(repo = %dto.name, %motivo, "repo omitido: nombre inválido");
            return None;
        }
    };
    if !url_clon_es_segura(&dto.clone_url) {
        tracing::warn!(
            repo = %dto.name,
            "repo omitido: la url de clonado no es https o lleva credenciales"
        );
        return None;
    }
    Some(RepoOrigen {
        id: IdRepo { dueno, nombre },
        url_clon: dto.clone_url,
        privado: dto.private,
        es_fork: dto.fork,
        archivado: dto.archived,
        rama_por_defecto: dto.default_branch,
        descripcion: dto.description,
        tamano_kb: dto.size,
    })
}

/// Convierte una página de repos, omitiendo los que no se puedan representar.
pub(crate) fn convertir_repos(dtos: Vec<RepoDto>) -> Vec<RepoOrigen> {
    dtos.into_iter().filter_map(convertir_repo).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_valido() -> RepoDto {
        RepoDto {
            name: "gitmereba".to_string(),
            owner: PropietarioDto {
                login: "jparga".to_string(),
            },
            clone_url: "https://github.com/jparga/gitmereba.git".to_string(),
            private: false,
            fork: false,
            archived: false,
            default_branch: Some("main".to_string()),
            description: None,
            size: 42,
        }
    }

    #[test]
    fn convierte_un_repo_valido() {
        let repo = convertir_repo(repo_valido()).unwrap();
        assert_eq!(repo.id.to_string(), "jparga/gitmereba");
        assert_eq!(repo.url_clon, "https://github.com/jparga/gitmereba.git");
        assert_eq!(repo.tamano_kb, 42);
    }

    #[test]
    fn omite_repo_con_nombre_invalido() {
        let mut dto = repo_valido();
        dto.name = "..".to_string();
        assert!(convertir_repo(dto).is_none());
    }

    #[test]
    fn omite_repo_con_propietario_invalido() {
        let mut dto = repo_valido();
        dto.owner.login = "a/b".to_string();
        assert!(convertir_repo(dto).is_none());
    }

    #[test]
    fn omite_repo_con_credenciales_en_la_url_de_clonado() {
        let mut dto = repo_valido();
        dto.clone_url = "https://usuario:token@github.com/jparga/gitmereba.git".to_string();
        assert!(convertir_repo(dto).is_none());
    }

    #[test]
    fn omite_repo_con_url_de_clonado_no_https() {
        let mut dto = repo_valido();
        dto.clone_url = "git://github.com/jparga/gitmereba.git".to_string();
        assert!(convertir_repo(dto).is_none());
    }

    #[test]
    fn convertir_repos_conserva_solo_los_validos() {
        let mut invalido = repo_valido();
        invalido.name = "..".to_string();
        let repos = convertir_repos(vec![repo_valido(), invalido]);
        assert_eq!(repos.len(), 1);
    }
}
