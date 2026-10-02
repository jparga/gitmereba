//! Recogida de datos para verificar la salud de los mirrors de una cuenta: la parte con
//! E/S del módulo. La evaluación en sí, sin E/S, está en [`super::evaluar`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use time::{Duration, OffsetDateTime};

use crate::config::RutasCuenta;
use crate::git;
use crate::github::{ApiGithub, ErrorGithub};
use crate::modelo::{Cuenta, IdRepo, RepoLocal, RepoOrigen};

use super::aviso::Aviso;
use super::caducidad::{DIAS_AVISO_POR_DEFECTO, aviso_caducidad};
use super::evaluar::{EntradaRepo, ResultadoFsck, evaluar};
use super::informe::InformeVerificacion;
use super::umbrales::Umbrales;

/// Cuántos SHA de GitHub se consultan como máximo en una pasada, por defecto.
pub const MAX_CONSULTAS_SHA_POR_DEFECTO: usize = 50;
/// Cuántos `git fsck` se ejecutan como máximo en una pasada, por defecto.
pub const MAX_FSCK_POR_DEFECTO: usize = 5;

/// Prefijo de las organizaciones de contingencia.
const PREFIJO_CONTINGENCIA: &str = "contingencia-";

/// Lo que se sabe de pasadas anteriores, para priorizar qué repos consultar primero
/// cuando no se puede consultar todo (límite de peticiones a GitHub, `fsck` caro).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextoVerificacion {
    /// Última vez que se consultó el SHA de GitHub de cada repo.
    pub ultima_verificacion: BTreeMap<IdRepo, OffsetDateTime>,
    /// Última vez que se ejecutó `git fsck` sobre cada repo.
    pub ultimo_fsck: BTreeMap<IdRepo, OffsetDateTime>,
    /// Cuánto hace que existe la cuenta, si se sabe (desde su primera sincronización
    /// registrada). Se usa como `antiguedad_conocida` de cada repo: sin ella, la regla
    /// «nunca sincronizado» de [`super::evaluar::evaluar`] nunca puede afirmar nada.
    pub antiguedad_de_la_cuenta: Option<Duration>,
    /// Mirrors originales con una contingencia abierta (`Almacen::contingencias_de`). Su
    /// sincronización está pausada a propósito mientras se trabaja en el repo hermano
    /// `contingencia-*`: no son obsoletos ni divergentes, están en contingencia.
    pub en_contingencia: BTreeSet<IdRepo>,
}

/// Opciones de una pasada de verificación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcionesVerificacion {
    /// Cuántos SHA de GitHub se consultan como máximo en esta pasada.
    pub max_consultas_sha: usize,
    /// Si se ejecuta `git fsck` en esta pasada: es caro, así que es opt-in.
    pub fsck: bool,
    /// Cuántos `git fsck` se ejecutan como máximo en esta pasada.
    pub max_fsck: usize,
}

impl Default for OpcionesVerificacion {
    fn default() -> Self {
        Self {
            max_consultas_sha: MAX_CONSULTAS_SHA_POR_DEFECTO,
            fsck: false,
            max_fsck: MAX_FSCK_POR_DEFECTO,
        }
    }
}

fn minusculas(valor: &str) -> String {
    valor.to_lowercase()
}

fn clave(id: &IdRepo) -> (String, String) {
    (
        minusculas(id.dueno.as_str()),
        minusculas(id.nombre.as_str()),
    )
}

fn en_contingencia(id: &IdRepo) -> bool {
    minusculas(id.dueno.as_str()).starts_with(PREFIJO_CONTINGENCIA)
}

/// Ruta del bare de `id` bajo `raiz` (la carpeta `gitea_repositorios()` de la cuenta),
/// comprobando con `canonicalize` que, tras resolver enlaces simbólicos, sigue quedando
/// dentro de `raiz`. La ruta se construye solo con [`crate::modelo::Nombre`] ya validados.
fn ruta_bare_validada(raiz: &Path, id: &IdRepo) -> Option<PathBuf> {
    let candidata = raiz
        .join(minusculas(id.dueno.as_str()))
        .join(format!("{}.git", minusculas(id.nombre.as_str())));
    let real = candidata.canonicalize().ok()?;
    let raiz_real = raiz.canonicalize().ok()?;
    if real.starts_with(&raiz_real) {
        Some(real)
    } else {
        None
    }
}

/// Ordena `candidatos` por antigüedad ascendente según `conocido` (lo nunca visto va
/// primero, con la fecha más antigua posible) y se queda con los `limite` primeros.
fn priorizar(
    mut candidatos: Vec<IdRepo>,
    conocido: &BTreeMap<IdRepo, OffsetDateTime>,
    limite: usize,
) -> Vec<IdRepo> {
    candidatos.sort_by_key(|id| {
        conocido
            .get(id)
            .copied()
            .unwrap_or(OffsetDateTime::UNIX_EPOCH)
    });
    candidatos.truncate(limite);
    candidatos
}

/// Verifica la salud de los mirrors ya existentes de `cuenta`: compara el SHA de la rama
/// por defecto con GitHub, ejecuta `git fsck` (si se pide) y aplica el resto de reglas de
/// [`super::evaluar`] a cada uno. Un error en un repo no detiene a los demás.
///
/// `origen` y `local` son lo ya listado de GitHub y de Gitea (no los vuelve a pedir esta
/// función: eso es responsabilidad de `sync`); esta función solo diagnostica lo que ya
/// existe como mirror en `local`, no da de alta nada nuevo.
pub async fn verificar_cuenta<G: ApiGithub>(
    github: &G,
    cuenta: &Cuenta,
    rutas: &RutasCuenta,
    origen: &[RepoOrigen],
    local: &[RepoLocal],
    contexto: &ContextoVerificacion,
    opciones: &OpcionesVerificacion,
) -> InformeVerificacion {
    let momento = OffsetDateTime::now_utc();
    let umbrales = Umbrales::para_intervalo(cuenta.intervalo_minutos);
    let mut avisos = Vec::new();

    match github.identidad().await {
        Ok(identidad) => {
            if let Some(aviso) = aviso_caducidad(identidad.caduca, momento, DIAS_AVISO_POR_DEFECTO)
            {
                avisos.push(aviso);
            }
        }
        Err(e) => avisos.push(Aviso::ErrorIdentidad {
            mensaje: e.to_string(),
        }),
    }

    let origen_por_clave: BTreeMap<(String, String), &RepoOrigen> =
        origen.iter().map(|r| (clave(&r.id), r)).collect();
    let excluidos: BTreeSet<(String, String)> =
        cuenta.alcance.excluidos.iter().map(clave).collect();

    // Rutas validadas: solo se toca el disco de los repos cuyo bare cae dentro de
    // `gitea_repositorios()` tras resolver enlaces simbólicos.
    let base = rutas.gitea_repositorios();
    let mut rutas_validas: BTreeMap<IdRepo, PathBuf> = BTreeMap::new();
    for repo in local {
        match ruta_bare_validada(&base, &repo.id) {
            Some(ruta) => {
                rutas_validas.insert(repo.id.clone(), ruta);
            }
            None => avisos.push(Aviso::RutaFueraDeRepositorios {
                id: repo.id.clone(),
            }),
        }
    }

    // Candidatos «normales»: con ruta válida, no excluidos y ajenos a contingencia (para
    // esos, `evaluar` no necesita SHA ni fsck, así que no merece la pena gastar peticiones
    // ni tiempo en consultarlos).
    let candidatos_normales: Vec<&RepoLocal> = local
        .iter()
        .filter(|r| rutas_validas.contains_key(&r.id))
        .filter(|r| !excluidos.contains(&clave(&r.id)))
        .filter(|r| !en_contingencia(&r.id) && !contexto.en_contingencia.contains(&r.id))
        .collect();

    // SHA local: `git refs` sobre el bare, buscando la rama por defecto de GitHub.
    let mut sha_local: BTreeMap<IdRepo, Option<String>> = BTreeMap::new();
    let mut error_lectura: BTreeMap<IdRepo, String> = BTreeMap::new();
    for repo in &candidatos_normales {
        let ruta = &rutas_validas[&repo.id];
        let Some(rama) = origen_por_clave
            .get(&clave(&repo.id))
            .and_then(|o| o.rama_por_defecto.as_deref())
        else {
            continue;
        };
        match git::refs(ruta).await {
            Ok(refs) => {
                let objetivo = format!("refs/heads/{rama}");
                let sha = refs
                    .into_iter()
                    .find(|(nombre, _)| *nombre == objetivo)
                    .map(|(_, sha)| sha);
                sha_local.insert(repo.id.clone(), sha);
            }
            Err(e) => {
                error_lectura.insert(repo.id.clone(), e.to_string());
            }
        }
    }

    // SHA de GitHub: como mucho `max_consultas_sha`, priorizando los que llevan más
    // tiempo sin verificarse. Si se agota el límite de peticiones, se deja de consultar
    // sin marcar ningún fallo por ello.
    let candidatos_sha: Vec<IdRepo> = candidatos_normales
        .iter()
        .filter(|r| {
            origen_por_clave
                .get(&clave(&r.id))
                .is_some_and(|o| o.rama_por_defecto.is_some())
        })
        .map(|r| r.id.clone())
        .collect();
    let candidatos_sha = priorizar(
        candidatos_sha,
        &contexto.ultima_verificacion,
        opciones.max_consultas_sha,
    );

    let mut sha_github: BTreeMap<IdRepo, Option<String>> = BTreeMap::new();
    let mut consultas_sha = 0usize;
    let mut limite_api_alcanzado = false;
    for id in candidatos_sha {
        if limite_api_alcanzado {
            break;
        }
        let rama = origen_por_clave
            .get(&clave(&id))
            .and_then(|o| o.rama_por_defecto.clone())
            .unwrap_or_default();
        consultas_sha += 1;
        match github.sha_de_rama(&id, &rama).await {
            Ok(sha) => {
                sha_github.insert(id, sha);
            }
            Err(ErrorGithub::LimiteDePeticiones { .. }) => {
                limite_api_alcanzado = true;
                avisos.push(Aviso::LimiteDePeticiones);
            }
            Err(e) => avisos.push(Aviso::ErrorRepo {
                id,
                mensaje: e.to_string(),
            }),
        }
    }

    // `git fsck`: caro, opt-in, como mucho `max_fsck`, priorizando igual que arriba.
    let mut fsck: BTreeMap<IdRepo, ResultadoFsck> = BTreeMap::new();
    let mut fsck_hechos = 0usize;
    if opciones.fsck {
        let candidatos_fsck: Vec<IdRepo> =
            candidatos_normales.iter().map(|r| r.id.clone()).collect();
        let candidatos_fsck = priorizar(candidatos_fsck, &contexto.ultimo_fsck, opciones.max_fsck);
        for id in candidatos_fsck {
            let ruta = &rutas_validas[&id];
            fsck_hechos += 1;
            let resultado = match git::fsck(ruta).await {
                Ok(()) => ResultadoFsck::Ok,
                Err(e) => ResultadoFsck::Corrupto(e.to_string()),
            };
            fsck.insert(id, resultado);
        }
    }

    let mut diagnosticos = Vec::with_capacity(local.len());
    for repo in local {
        let k = clave(&repo.id);
        let excluido = excluidos.contains(&k);
        let origen_repo = origen_por_clave.get(&k).map(|r| (*r).clone());
        let en_contingencia =
            en_contingencia(&repo.id) || contexto.en_contingencia.contains(&repo.id);
        let huerfano = !excluido && origen_repo.is_none() && !en_contingencia;
        let entrada = EntradaRepo {
            local: repo.clone(),
            origen: origen_repo,
            huerfano,
            excluido,
            en_contingencia,
            sha_github: sha_github.get(&repo.id).cloned().flatten(),
            sha_local: sha_local.get(&repo.id).cloned().flatten(),
            fsck: fsck.get(&repo.id).cloned(),
            error_lectura: error_lectura.get(&repo.id).cloned(),
            antiguedad_conocida: contexto.antiguedad_de_la_cuenta,
        };
        diagnosticos.push(evaluar(&entrada, momento, &umbrales));
    }

    InformeVerificacion {
        cuenta: cuenta.clone(),
        momento,
        diagnosticos,
        consultas_sha,
        fsck_hechos,
        limite_api_alcanzado,
        avisos,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use crate::config::Rutas;
    use crate::git::Opciones;
    use crate::github::ErrorGithub;
    use crate::modelo::EstadoRepo;
    use crate::verificacion::Motivo;

    use super::*;
    use crate::verificacion::dobles::GithubDoble;
    use crate::verificacion::dobles::pruebas::{cuenta, id, repo_local, repo_origen};

    /// Crea un bare de Gitea en `raiz/<dueno>/<nombre>.git` con un commit en `rama`, y
    /// devuelve su SHA. Usa `crate::git::ejecutar`, que ignora el gitconfig del sistema
    /// que corre las pruebas: por eso el commit fija `user.name`/`user.email` con `-c`.
    async fn crear_bare(raiz: &Path, dueno: &str, nombre: &str, rama: &str) -> String {
        let trabajo = tempfile::tempdir().expect("directorio temporal de trabajo");
        let opciones = Opciones {
            directorio: Some(trabajo.path().to_path_buf()),
            ..Opciones::default()
        };
        git::ejecutar(
            &["-c", &format!("init.defaultBranch={rama}"), "init", "-q"],
            &opciones,
        )
        .await
        .expect("git init");
        git::ejecutar(
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test.invalid",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "inicial",
            ],
            &opciones,
        )
        .await
        .expect("git commit");
        let salida = git::ejecutar(&["rev-parse", "HEAD"], &opciones)
            .await
            .expect("rev-parse");
        let sha = salida.stdout.trim().to_string();

        let bare = raiz
            .join(dueno.to_lowercase())
            .join(format!("{}.git", nombre.to_lowercase()));
        std::fs::create_dir_all(bare.parent().expect("padre de la ruta bare"))
            .expect("crear carpeta del dueño");
        git::ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                trabajo.path().to_str().expect("utf8"),
                bare.to_str().expect("utf8"),
            ],
            &Opciones::default(),
        )
        .await
        .expect("clonar en bare");
        sha
    }

    fn rutas_cuenta(gitea_repositorios: &Path) -> RutasCuenta {
        // `gitea_repositorios()` = carpeta/gitea/repositories: se crea esa estructura y
        // se ancla `RutasCuenta` a la carpeta de la cuenta que la contiene.
        let carpeta = gitea_repositorios
            .parent()
            .expect("gitea")
            .parent()
            .expect("carpeta de la cuenta");
        RutasCuenta::nueva(carpeta)
    }

    fn preparar_repositorios() -> (TempDir, PathBuf) {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let carpeta_cuenta = raiz.path().join("cuenta");
        let repositorios = RutasCuenta::nueva(&carpeta_cuenta).gitea_repositorios();
        std::fs::create_dir_all(&repositorios).expect("crear gitea/repositories");
        (raiz, repositorios)
    }

    #[tokio::test]
    async fn sha_igual_es_ok() {
        let (_raiz, repositorios) = preparar_repositorios();
        let sha = crear_bare(&repositorios, "jparga", "repo1", "main").await;

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_sha(&id("jparga", "repo1"), &sha);

        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "repo1", 10, Some("main"))];
        let local = vec![repo_local(
            "jparga",
            "repo1",
            false,
            Some(OffsetDateTime::now_utc()),
        )];
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos.len(), 1);
        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Ok);
        assert_eq!(informe.consultas_sha, 1);
    }

    #[tokio::test]
    async fn sha_distinto_y_sync_antigua_es_fallo() {
        let (_raiz, repositorios) = preparar_repositorios();
        crear_bare(&repositorios, "jparga", "repo1", "main").await;

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_sha(&id("jparga", "repo1"), "0".repeat(40).as_str());

        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "repo1", 10, Some("main"))];
        let local = vec![repo_local(
            "jparga",
            "repo1",
            false,
            Some(OffsetDateTime::now_utc() - time::Duration::hours(3)),
        )];
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Fallo);
    }

    #[tokio::test]
    async fn objeto_corrupto_con_fsck_activado_es_fallo() {
        let (_raiz, repositorios) = preparar_repositorios();
        let sha = crear_bare(&repositorios, "jparga", "repo1", "main").await;
        let bare = repositorios.join("jparga").join("repo1.git");

        // Corrompe el objeto del commit: le quita el permiso de solo lectura y le
        // machaca el contenido.
        use std::os::unix::fs::PermissionsExt;
        let ruta_objeto = bare.join("objects").join(&sha[..2]).join(&sha[2..]);
        let mut permisos = std::fs::metadata(&ruta_objeto)
            .expect("metadata del objeto")
            .permissions();
        permisos.set_mode(0o644);
        std::fs::set_permissions(&ruta_objeto, permisos).expect("permitir escritura");
        std::fs::write(&ruta_objeto, b"no es un objeto git valido").expect("corromper objeto");

        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "repo1", 10, Some("main"))];
        let local = vec![repo_local("jparga", "repo1", false, None)];
        let opciones = OpcionesVerificacion {
            fsck: true,
            ..OpcionesVerificacion::default()
        };
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &opciones,
        )
        .await;

        assert_eq!(informe.fsck_hechos, 1);
        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Fallo);
    }

    #[tokio::test]
    async fn respeta_el_maximo_de_consultas_sha_y_prioriza_lo_mas_antiguo() {
        let (_raiz, repositorios) = preparar_repositorios();
        for n in 1..=3 {
            crear_bare(&repositorios, "jparga", &format!("repo{n}"), "main").await;
        }

        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen: Vec<_> = (1..=3)
            .map(|n| repo_origen("jparga", &format!("repo{n}"), 10, Some("main")))
            .collect();
        let local: Vec<_> = (1..=3)
            .map(|n| repo_local("jparga", &format!("repo{n}"), false, None))
            .collect();

        // "repo2" es el que lleva más tiempo sin verificarse (no está en el contexto);
        // "repo1" y "repo3" sí, con "repo3" más reciente que "repo1".
        let mut contexto = ContextoVerificacion::default();
        contexto.ultima_verificacion.insert(
            id("jparga", "repo1"),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
        );
        contexto.ultima_verificacion.insert(
            id("jparga", "repo3"),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(2),
        );

        let opciones = OpcionesVerificacion {
            max_consultas_sha: 2,
            ..OpcionesVerificacion::default()
        };
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &contexto,
            &opciones,
        )
        .await;

        assert_eq!(informe.consultas_sha, 2);
        let llamadas = github.llamadas_sha.lock().unwrap().clone();
        assert_eq!(llamadas.len(), 2);
        // Los dos con más prioridad: el nunca visto ("repo2") y el más antiguo con
        // historial ("repo1"). "repo3" (el más reciente) se queda fuera.
        assert!(llamadas.contains(&id("jparga", "repo2")));
        assert!(llamadas.contains(&id("jparga", "repo1")));
        assert!(!llamadas.contains(&id("jparga", "repo3")));
    }

    #[tokio::test]
    async fn el_limite_de_peticiones_corta_las_consultas_sin_marcar_fallos() {
        let (_raiz, repositorios) = preparar_repositorios();
        for n in 1..=2 {
            crear_bare(&repositorios, "jparga", &format!("repo{n}"), "main").await;
        }

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_fallo_de_limite();
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen: Vec<_> = (1..=2)
            .map(|n| repo_origen("jparga", &format!("repo{n}"), 10, Some("main")))
            .collect();
        let local: Vec<_> = (1..=2)
            .map(|n| repo_local("jparga", &format!("repo{n}"), false, None))
            .collect();

        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert!(informe.limite_api_alcanzado);
        // Se ha intentado una consulta (la que ha agotado el límite) y ninguna más.
        assert_eq!(github.llamadas_sha.lock().unwrap().len(), 1);
        // No se marca ningún mirror como fallo solo por no haber podido comprobar el SHA.
        assert!(
            informe
                .diagnosticos
                .iter()
                .all(|d| d.estado != EstadoRepo::Fallo)
        );
    }

    #[tokio::test]
    async fn un_fallo_puntual_de_github_no_detiene_el_resto() {
        let (_raiz, repositorios) = preparar_repositorios();
        for n in 1..=2 {
            crear_bare(&repositorios, "jparga", &format!("repo{n}"), "main").await;
        }

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_fallo(&id("jparga", "repo1"), ErrorGithub::NoEncontrado);
        github.con_sha(&id("jparga", "repo2"), "cualquiera");

        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen: Vec<_> = (1..=2)
            .map(|n| repo_origen("jparga", &format!("repo{n}"), 10, Some("main")))
            .collect();
        let local: Vec<_> = (1..=2)
            .map(|n| repo_local("jparga", &format!("repo{n}"), false, None))
            .collect();

        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.consultas_sha, 2);
        assert!(!informe.limite_api_alcanzado);
        assert!(informe.avisos.iter().any(
            |a| matches!(a, Aviso::ErrorRepo { id: repo_id, .. } if *repo_id == id("jparga", "repo1"))
        ));
    }

    #[tokio::test]
    async fn un_enlace_simbolico_fuera_de_la_carpeta_no_se_toca() {
        let (raiz, repositorios) = preparar_repositorios();
        std::fs::create_dir_all(repositorios.join("jparga")).expect("crear carpeta del dueño");
        let fuera = raiz.path().join("fuera-de-repositorios");
        std::fs::create_dir_all(&fuera).expect("crear carpeta fuera");
        symlink(&fuera, repositorios.join("jparga").join("repo1.git"))
            .expect("crear enlace simbólico");

        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "repo1", 10, Some("main"))];
        let local = vec![repo_local("jparga", "repo1", false, None)];

        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert!(informe.avisos.iter().any(
            |a| matches!(a, Aviso::RutaFueraDeRepositorios { id: repo_id } if *repo_id == id("jparga", "repo1"))
        ));
        // Nunca se ha llegado a consultar el SHA de GitHub para este repo.
        assert!(github.llamadas_sha.lock().unwrap().is_empty());
        assert_eq!(informe.consultas_sha, 0);
    }

    #[tokio::test]
    async fn nombres_en_mayusculas_en_github_encuentran_el_bare_en_minusculas() {
        let (_raiz, repositorios) = preparar_repositorios();
        let sha = crear_bare(&repositorios, "jparga", "repo1", "main").await;

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_sha(&id("JParga", "Repo1"), &sha);

        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("JParga", "Repo1", 10, Some("main"))];
        let local = vec![repo_local(
            "jparga",
            "repo1",
            false,
            Some(OffsetDateTime::now_utc()),
        )];
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Ok);
    }

    #[tokio::test]
    async fn un_repo_que_ya_no_esta_en_github_es_huerfano() {
        let (_raiz, repositorios) = preparar_repositorios();
        crear_bare(&repositorios, "jparga", "desaparecido", "main").await;

        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let local = vec![repo_local("jparga", "desaparecido", false, None)];

        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &[],
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Huerfano);
        // Un huérfano no necesita SHA de GitHub: no se ha gastado ninguna consulta.
        assert_eq!(informe.consultas_sha, 0);
    }

    #[tokio::test]
    async fn el_aviso_de_caducidad_del_token_llega_al_informe() {
        let (_raiz, repositorios) = preparar_repositorios();
        let github = GithubDoble::con_identidad(
            "jparga",
            Some(OffsetDateTime::now_utc() + time::Duration::days(1)),
        );
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &[],
            &[],
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;

        assert!(
            informe
                .avisos
                .iter()
                .any(|a| matches!(a, Aviso::TokenCaducaPronto { .. }))
        );
    }

    #[tokio::test]
    async fn el_mirror_pausado_de_una_contingencia_abierta_no_es_obsoleto() {
        let (_raiz, repositorios) = preparar_repositorios();
        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "pausado", 100, Some("main"))];
        let hace_mucho = OffsetDateTime::now_utc() - time::Duration::days(10);
        let local = vec![repo_local("jparga", "pausado", false, Some(hace_mucho))];
        let sin_contexto = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &ContextoVerificacion::default(),
            &OpcionesVerificacion::default(),
        )
        .await;
        assert_ne!(
            sin_contexto.diagnosticos[0].estado,
            EstadoRepo::Contingencia
        );

        let contexto = ContextoVerificacion {
            en_contingencia: [local[0].id.clone()].into_iter().collect(),
            ..ContextoVerificacion::default()
        };
        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &contexto,
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Contingencia);
        assert_eq!(informe.consultas_sha, 0);
    }

    #[tokio::test]
    async fn nunca_sincronizado_con_antiguedad_de_la_cuenta_conocida_y_vieja_es_fallo() {
        let (_raiz, repositorios) = preparar_repositorios();
        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "vacio", 100, Some("main"))];
        let local = vec![repo_local("jparga", "vacio", true, None)];
        let contexto = ContextoVerificacion {
            antiguedad_de_la_cuenta: Some(time::Duration::hours(2)),
            ..ContextoVerificacion::default()
        };

        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &contexto,
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Fallo);
        assert_eq!(
            informe.diagnosticos[0].motivos,
            vec![Motivo::NuncaSincronizado]
        );
    }

    #[tokio::test]
    async fn nunca_sincronizado_con_cuenta_recien_creada_no_es_fallo() {
        let (_raiz, repositorios) = preparar_repositorios();
        let github = GithubDoble::con_identidad("jparga", None);
        let c = cuenta("jparga", 30, "/tmp/gitmereba-test");
        let origen = vec![repo_origen("jparga", "vacio", 100, Some("main"))];
        let local = vec![repo_local("jparga", "vacio", true, None)];
        // Sin antigüedad conocida (cuenta recién dada de alta: aún no hay ninguna
        // sincronización previa registrada), no se puede afirmar nada.
        let contexto = ContextoVerificacion::default();

        let informe = verificar_cuenta(
            &github,
            &c,
            &rutas_cuenta(&repositorios),
            &origen,
            &local,
            &contexto,
            &OpcionesVerificacion::default(),
        )
        .await;

        assert_eq!(informe.diagnosticos[0].estado, EstadoRepo::Ok);
    }

    #[test]
    fn opciones_por_defecto() {
        let opciones = OpcionesVerificacion::default();
        assert_eq!(opciones.max_consultas_sha, MAX_CONSULTAS_SHA_POR_DEFECTO);
        assert_eq!(opciones.max_fsck, MAX_FSCK_POR_DEFECTO);
        assert!(!opciones.fsck);
    }

    #[test]
    fn rutas_reales_no_se_usan_en_las_pruebas() {
        // Comprobación de que `Rutas::con_raiz` sigue disponible para futuras pruebas de
        // integración de este módulo, sin usar `Rutas::del_sistema`.
        let r = Rutas::con_raiz("/tmp/gitmereba-verificacion-test");
        assert!(
            r.directorio_datos()
                .starts_with("/tmp/gitmereba-verificacion-test")
        );
    }
}
