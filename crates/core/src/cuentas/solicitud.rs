//! Datos de entrada del alta y su previsualización (pasos 1-2 del alta).

use std::path::PathBuf;

use time::OffsetDateTime;

use crate::github::{ApiGithub, Identidad};
use crate::modelo::{Alcance, Cuenta, Nombre, RepoOrigen};
use crate::secretos::Secreto;
use crate::sync::{Accion, EstadoPrevio, planificar};

use super::error::ErrorCuentas;

/// Datos que pide el asistente de alta de una cuenta.
pub struct SolicitudAlta {
    pub login: Nombre,
    /// Token de lectura de GitHub. Nunca se guarda en ningún sitio que no sea el
    /// llavero (ver [`super::alta`]).
    pub token: Secreto,
    pub carpeta: PathBuf,
    pub alcance: Alcance,
    pub intervalo_minutos: u32,
    /// Ver [`crate::instancia::ParametrosAppIni::modo_pruebas`]: se reenvía tal cual a
    /// la provisión de Gitea. Habilita clonar desde una ruta local (`IMPORT_LOCAL_PATHS`)
    /// y desde redes locales (`ALLOW_LOCALNETWORKS`), algo que solo tiene sentido en la
    /// prueba de integración con un Gitea real (`crates/core/tests/cuentas_gitea_real.rs`).
    /// La CLI nunca expone un modo para activarlo: siempre construye la solicitud con
    /// `false`. **Nunca debe ser `true` fuera de esa prueba.**
    pub modo_pruebas: bool,
}

/// Lo que se le muestra al usuario antes de confirmar el alta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Previsualizacion {
    /// Login informado por el token (ya comprobado que coincide con el pedido).
    pub login: Nombre,
    pub caduca_token: Option<OffsetDateTime>,
    /// Organizaciones a las que pertenece la cuenta en GitHub (no solo las pedidas).
    pub organizaciones_disponibles: Vec<Nombre>,
    /// Repos que se clonarían con el alcance pedido.
    pub repos_a_clonar: Vec<RepoOrigen>,
    /// Suma de `tamano_kb` de `repos_a_clonar`.
    pub tamano_total_kb: u64,
}

/// Valida el token y calcula qué se clonaría, sin tocar el disco ni Gitea.
///
/// Pasos 1-2 del alta: valida el token contra GitHub (login,
/// scopes, caducidad), comprueba que el login del token coincide (insensible a
/// mayúsculas) con `solicitud.login`, lista organizaciones y repos, y reutiliza
/// [`crate::sync::planificar`] con un Gitea vacío para no duplicar las reglas de qué se
/// clona (forks, organizaciones en alcance, exclusiones).
pub async fn previsualizar_alta<G: ApiGithub>(
    github: &G,
    solicitud: &SolicitudAlta,
) -> Result<Previsualizacion, ErrorCuentas> {
    let identidad = github.identidad().await?;
    if identidad.login.as_str().to_lowercase() != solicitud.login.as_str().to_lowercase() {
        return Err(ErrorCuentas::TokenNoCoincideConLogin {
            esperado: solicitud.login.clone(),
            obtenido: identidad.login,
        });
    }

    let organizaciones_disponibles = github.organizaciones().await?;

    let mut origen = github.repos_de_usuario().await?;
    for organizacion in &solicitud.alcance.organizaciones {
        origen.extend(github.repos_de_organizacion(organizacion).await?);
    }

    // Cuenta ficticia solo para reutilizar `planificar`: ni `carpeta` ni `puerto`
    // influyen en qué se plantea clonar, así que llevan valores de relleno.
    let cuenta_temporal = Cuenta {
        login: identidad.login.clone(),
        carpeta: solicitud.carpeta.clone(),
        puerto: 0,
        intervalo_minutos: solicitud.intervalo_minutos,
        alcance: solicitud.alcance.clone(),
        lan: None,
    };
    let plan = planificar(&cuenta_temporal, &origen, &[], &EstadoPrevio::default());
    let repos_a_clonar: Vec<RepoOrigen> = plan
        .acciones
        .into_iter()
        .filter_map(|accion| match accion {
            Accion::CrearMirror(repo) => Some(repo),
            _ => None,
        })
        .collect();
    let tamano_total_kb = repos_a_clonar.iter().map(|repo| repo.tamano_kb).sum();

    Ok(Previsualizacion {
        login: identidad.login,
        caduca_token: identidad.caduca,
        organizaciones_disponibles,
        repos_a_clonar,
        tamano_total_kb,
    })
}

/// Todo lo que GitHub sabe de la cuenta antes de aplicar ningún alcance: identidad,
/// organizaciones a las que pertenece y TODOS sus repos (propios y de cada
/// organización, incluidos los forks). Paso 1 del asistente de alta
/// (comando `validar_alta` de la interfaz): el usuario elige después, sobre este
/// resultado, qué organizaciones y qué repos concretos quiere clonar (eso ya no lo
/// calcula este módulo, lo decide la interfaz y se lo pasa a [`super::alta::alta`] como
/// `Alcance::excluidos`).
pub struct Descubrimiento {
    pub identidad: Identidad,
    pub organizaciones: Vec<Nombre>,
    pub repos: Vec<RepoOrigen>,
}

/// Valida el token contra GitHub (debe informar el mismo login que `login_esperado`,
/// insensible a mayúsculas) y descubre organizaciones y repos, sin aplicar ningún
/// alcance ni tocar el disco, el llavero o Gitea.
pub async fn descubrir_repos<G: ApiGithub>(
    github: &G,
    login_esperado: &Nombre,
) -> Result<Descubrimiento, ErrorCuentas> {
    let identidad = github.identidad().await?;
    if identidad.login.as_str().to_lowercase() != login_esperado.as_str().to_lowercase() {
        return Err(ErrorCuentas::TokenNoCoincideConLogin {
            esperado: login_esperado.clone(),
            obtenido: identidad.login,
        });
    }

    let organizaciones = github.organizaciones().await?;
    let mut repos = github.repos_de_usuario().await?;
    for organizacion in &organizaciones {
        repos.extend(github.repos_de_organizacion(organizacion).await?);
    }

    Ok(Descubrimiento {
        identidad,
        organizaciones,
        repos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuentas::dobles::GithubDoble;
    use crate::modelo::IdRepo;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn solicitud(login: &str, organizaciones: &[&str]) -> SolicitudAlta {
        SolicitudAlta {
            login: nombre(login),
            token: Secreto::nuevo("ghp_de_prueba"),
            carpeta: PathBuf::from("/tmp/gitmereba-test-jamas-creada"),
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: organizaciones.iter().map(|o| nombre(o)).collect(),
                excluidos: vec![],
            },
            intervalo_minutos: 30,
            modo_pruebas: false,
        }
    }

    #[tokio::test]
    async fn login_distinto_del_token_es_un_error() {
        let github = GithubDoble::con_identidad("otra-persona", None);
        let resultado = previsualizar_alta(&github, &solicitud("jparga", &[])).await;
        assert!(matches!(
            resultado,
            Err(ErrorCuentas::TokenNoCoincideConLogin { .. })
        ));
    }

    #[tokio::test]
    async fn login_distinto_en_mayusculas_no_es_un_error() {
        let github = GithubDoble::con_identidad("JParga", None);
        let resultado = previsualizar_alta(&github, &solicitud("jparga", &[])).await;
        assert!(resultado.is_ok());
    }

    #[tokio::test]
    async fn incluye_repos_propios_y_de_las_organizaciones_pedidas() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "propio", false, false)]);
        github.con_repos_organizacion(
            "acme",
            vec![GithubDoble::repo("acme", "repo1", false, false)],
        );

        let previsualizacion = previsualizar_alta(&github, &solicitud("jparga", &["acme"]))
            .await
            .expect("previsualizar_alta no falla");

        let ids: Vec<IdRepo> = previsualizacion
            .repos_a_clonar
            .iter()
            .map(|r| r.id.clone())
            .collect();
        assert!(ids.contains(&IdRepo {
            dueno: nombre("jparga"),
            nombre: nombre("propio")
        }));
        assert!(ids.contains(&IdRepo {
            dueno: nombre("acme"),
            nombre: nombre("repo1")
        }));
    }

    #[tokio::test]
    async fn los_forks_no_se_clonan_por_defecto() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "fork1", false, true)]);

        let previsualizacion = previsualizar_alta(&github, &solicitud("jparga", &[]))
            .await
            .expect("previsualizar_alta no falla");

        assert!(previsualizacion.repos_a_clonar.is_empty());
    }

    #[tokio::test]
    async fn el_tamano_total_suma_el_de_los_repos_a_clonar() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![
            GithubDoble::repo_con_tamano("jparga", "uno", 10),
            GithubDoble::repo_con_tamano("jparga", "dos", 20),
        ]);

        let previsualizacion = previsualizar_alta(&github, &solicitud("jparga", &[]))
            .await
            .expect("previsualizar_alta no falla");

        assert_eq!(previsualizacion.tamano_total_kb, 30);
    }

    #[tokio::test]
    async fn devuelve_la_caducidad_del_token() {
        let caduca = OffsetDateTime::from_unix_timestamp(2_000_000_000).ok();
        let github = GithubDoble::con_identidad("jparga", caduca);
        let previsualizacion = previsualizar_alta(&github, &solicitud("jparga", &[]))
            .await
            .expect("previsualizar_alta no falla");
        assert_eq!(previsualizacion.caduca_token, caduca);
    }

    #[tokio::test]
    async fn descubrir_repos_incluye_forks_y_repos_de_todas_las_organizaciones() {
        let github = GithubDoble::con_identidad("jparga", None);
        github.con_organizaciones(&["mereba-oss"]);
        github.con_repos_usuario(vec![
            GithubDoble::repo("jparga", "propio", false, false),
            GithubDoble::repo("jparga", "un-fork", false, true),
        ]);
        github.con_repos_organizacion(
            "mereba-oss",
            vec![GithubDoble::repo("mereba-oss", "gitmereba", false, false)],
        );

        let descubrimiento = descubrir_repos(&github, &nombre("jparga"))
            .await
            .expect("descubrir_repos no falla");

        assert_eq!(descubrimiento.organizaciones, vec![nombre("mereba-oss")]);
        let ids: Vec<IdRepo> = descubrimiento.repos.iter().map(|r| r.id.clone()).collect();
        assert!(ids.contains(&IdRepo {
            dueno: nombre("jparga"),
            nombre: nombre("un-fork")
        }));
        assert_eq!(descubrimiento.repos.len(), 3);
    }

    #[tokio::test]
    async fn descubrir_repos_falla_si_el_login_del_token_no_coincide() {
        let github = GithubDoble::con_identidad("otra-persona", None);
        let resultado = descubrir_repos(&github, &nombre("jparga")).await;
        assert!(matches!(
            resultado,
            Err(ErrorCuentas::TokenNoCoincideConLogin { .. })
        ));
    }
}
