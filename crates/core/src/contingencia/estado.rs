//! `estado`: compara el repo de contingencia en disco con su punto de partida.
//!
//! Sin red: todo sale de leer el bare local con `git`. Nota de diseño: una plantilla
//! podría sugerir una firma `estado(gitea, cuenta, rutas, id, partida)`,
//! pero un parámetro `gitea: &impl ApiGitea` sin usar (esta función no hace ninguna
//! petición) sería código muerto y `clippy -D warnings` lo rechaza; por eso esta firma
//! solo lleva lo que de verdad hace falta para cumplir «sin red».

use std::collections::BTreeMap;
use std::path::Path;

use crate::config::RutasCuenta;
use crate::git::{self, ErrorGit, Opciones, validar_ref};
use crate::modelo::IdRepo;

use super::error::ErrorContingencia;
use super::modelo::{EstadoContingencia, EstadoRama, PuntoDePartida};
use super::nombres::org_contingencia;
use super::rutas::ruta_bare_validada;

/// Compara las ramas del repo de contingencia de `id_original` con `punto_de_partida`:
/// commits de más, ramas nuevas y ramas borradas.
pub async fn estado(
    rutas: &RutasCuenta,
    id_original: &IdRepo,
    punto_de_partida: &PuntoDePartida,
) -> Result<EstadoContingencia, ErrorContingencia> {
    let dueno_contingencia = org_contingencia(&id_original.dueno)?;
    let id_contingencia = IdRepo {
        dueno: dueno_contingencia,
        nombre: id_original.nombre.clone(),
    };
    let bare = ruta_bare_validada(&rutas.gitea_repositorios(), &id_contingencia)
        .ok_or_else(|| ErrorContingencia::NoActivada(id_original.clone()))?;

    let refs_actuales = git::refs(&bare).await?;
    let cabezas_actuales: BTreeMap<&str, &str> = refs_actuales
        .iter()
        .filter_map(|(nombre, sha)| {
            nombre
                .strip_prefix("refs/heads/")
                .map(|rama| (rama, sha.as_str()))
        })
        .collect();

    let mut ramas = Vec::with_capacity(cabezas_actuales.len());
    for (rama, sha) in &cabezas_actuales {
        let referencia = format!("refs/heads/{rama}");
        let rama_info = match punto_de_partida.sha_de(&referencia) {
            Some(partida) if partida == *sha => EstadoRama {
                rama: (*rama).to_string(),
                commits_de_mas: 0,
                es_nueva: false,
            },
            Some(partida) => EstadoRama {
                rama: (*rama).to_string(),
                commits_de_mas: git::commits_de_mas(&bare, sha, partida).await?,
                es_nueva: false,
            },
            None => EstadoRama {
                rama: (*rama).to_string(),
                commits_de_mas: contar_commits(&bare, sha).await?,
                es_nueva: true,
            },
        };
        ramas.push(rama_info);
    }

    let ramas_borradas: Vec<String> = punto_de_partida
        .refs
        .keys()
        .filter_map(|referencia| referencia.strip_prefix("refs/heads/"))
        .filter(|rama| !cabezas_actuales.contains_key(rama))
        .map(str::to_string)
        .collect();

    Ok(EstadoContingencia {
        ramas,
        ramas_borradas,
    })
}

/// Como [`estado`], pero relee el punto de partida directamente del bare del mirror
/// original en vez de que el llamador lo guarde aparte.
///
/// Al pausarse (paso 4 de «Activar»), el mirror deja de sincronizar y sus refs quedan
/// congeladas: volver a leerlas más tarde da el mismo punto de partida que se anotó al
/// activar la contingencia. Pensado para consultar el estado sin más contexto que las
/// rutas de la cuenta (p. ej. desde `app/`, que no tiene dónde guardar el punto de
/// partida entre invocaciones).
pub async fn estado_de_mirror(
    rutas: &RutasCuenta,
    id_original: &IdRepo,
) -> Result<EstadoContingencia, ErrorContingencia> {
    let bare_mirror = ruta_bare_validada(&rutas.gitea_repositorios(), id_original)
        .ok_or_else(|| ErrorContingencia::RutaBareInvalida(id_original.clone()))?;
    let punto_de_partida = PuntoDePartida::leer(&bare_mirror).await?;
    estado(rutas, id_original, &punto_de_partida).await
}

/// Número total de commits alcanzables desde `referencia` (para una rama que no existía
/// en el punto de partida: todos sus commits cuentan como «de más»).
async fn contar_commits(bare: &Path, referencia: &str) -> Result<u64, ErrorGit> {
    validar_ref(referencia)?;
    let opciones = Opciones {
        directorio: Some(bare.to_path_buf()),
        ..Opciones::default()
    };
    let salida = git::ejecutar(
        &["rev-list", "--count", "--end-of-options", referencia],
        &opciones,
    )
    .await?;
    salida
        .stdout
        .trim()
        .parse::<u64>()
        .map_err(|_| ErrorGit::Sistema("salida inesperada de «rev-list --count»".to_string()))
}

#[cfg(test)]
mod tests {
    use crate::modelo::Nombre;

    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(repo),
        }
    }

    async fn git_de_prueba(directorio: &Path, args: &[&str]) -> String {
        let mut completos = vec!["-c", "user.name=Test", "-c", "user.email=test@test.invalid"];
        completos.extend_from_slice(args);
        let opciones = Opciones {
            directorio: Some(directorio.to_path_buf()),
            ..Opciones::default()
        };
        git::ejecutar(&completos, &opciones)
            .await
            .unwrap_or_else(|e| panic!("«git {args:?}» falló: {e}"))
            .stdout
    }

    /// Crea el bare de contingencia de `jparga/repo1` con un commit inicial en `main`,
    /// devolviendo el directorio raíz de `repositories/` y el SHA del commit.
    async fn preparar_bare_contingencia() -> (tempfile::TempDir, RutasCuenta, String) {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let repositorios = rutas.gitea_repositorios();

        let trabajo = tempfile::tempdir().expect("directorio de trabajo temporal");
        git_de_prueba(
            trabajo.path(),
            &["-c", "init.defaultBranch=main", "init", "-q"],
        )
        .await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "inicial"],
        )
        .await;
        let sha = git_de_prueba(trabajo.path(), &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_string();

        let bare = repositorios.join("contingencia-jparga").join("repo1.git");
        std::fs::create_dir_all(bare.parent().expect("padre")).expect("crear carpeta del dueño");
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

        (raiz, rutas, sha)
    }

    #[tokio::test]
    async fn sin_commits_de_mas_si_coincide_con_el_punto_de_partida() {
        let (_raiz, rutas, sha) = preparar_bare_contingencia().await;
        let punto = PuntoDePartida {
            refs: [("refs/heads/main".to_string(), sha)].into(),
        };

        let estado_actual = estado(&rutas, &id("jparga", "repo1"), &punto)
            .await
            .expect("estado no falla");
        assert!(!estado_actual.hay_commits_de_mas());
        assert_eq!(estado_actual.ramas.len(), 1);
        assert_eq!(estado_actual.ramas[0].commits_de_mas, 0);
        assert!(!estado_actual.ramas[0].es_nueva);
    }

    #[tokio::test]
    async fn detecta_un_commit_de_mas_tras_el_punto_de_partida() {
        let (_raiz, rutas, sha_partida) = preparar_bare_contingencia().await;
        let bare = rutas
            .gitea_repositorios()
            .join("contingencia-jparga")
            .join("repo1.git");

        // Un segundo commit directamente sobre el bare (equivalente a lo que dejaría un
        // `git push` de un clon con escritura).
        let trabajo = tempfile::tempdir().expect("clon de trabajo");
        git_de_prueba(
            trabajo.path(),
            &["clone", "-q", bare.to_str().expect("utf8"), "."],
        )
        .await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "segundo"],
        )
        .await;
        git_de_prueba(trabajo.path(), &["push", "-q", "origin", "main"]).await;

        let punto = PuntoDePartida {
            refs: [("refs/heads/main".to_string(), sha_partida)].into(),
        };
        let estado_actual = estado(&rutas, &id("jparga", "repo1"), &punto)
            .await
            .expect("estado no falla");

        assert!(estado_actual.hay_commits_de_mas());
        assert_eq!(estado_actual.ramas[0].commits_de_mas, 1);
    }

    #[tokio::test]
    async fn una_rama_nueva_se_marca_como_tal() {
        let (_raiz, rutas, _sha) = preparar_bare_contingencia().await;
        let bare = rutas
            .gitea_repositorios()
            .join("contingencia-jparga")
            .join("repo1.git");

        let trabajo = tempfile::tempdir().expect("clon de trabajo");
        git_de_prueba(
            trabajo.path(),
            &["clone", "-q", bare.to_str().expect("utf8"), "."],
        )
        .await;
        git_de_prueba(trabajo.path(), &["checkout", "-q", "-b", "nueva"]).await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "en rama nueva"],
        )
        .await;
        git_de_prueba(trabajo.path(), &["push", "-q", "origin", "nueva"]).await;

        let punto = PuntoDePartida::default();
        let estado_actual = estado(&rutas, &id("jparga", "repo1"), &punto)
            .await
            .expect("estado no falla");

        let nueva = estado_actual
            .ramas
            .iter()
            .find(|r| r.rama == "nueva")
            .expect("rama nueva presente");
        assert!(nueva.es_nueva);
        assert!(nueva.commits_de_mas > 0);
    }

    #[tokio::test]
    async fn una_rama_borrada_se_informa_sin_tocar_nada() {
        let (_raiz, rutas, sha) = preparar_bare_contingencia().await;
        let punto = PuntoDePartida {
            refs: [
                ("refs/heads/main".to_string(), sha),
                ("refs/heads/desaparecida".to_string(), "0".repeat(40)),
            ]
            .into(),
        };

        let estado_actual = estado(&rutas, &id("jparga", "repo1"), &punto)
            .await
            .expect("estado no falla");
        assert_eq!(
            estado_actual.ramas_borradas,
            vec!["desaparecida".to_string()]
        );
    }

    #[tokio::test]
    async fn falla_si_la_contingencia_no_esta_activada() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        std::fs::create_dir_all(rutas.gitea_repositorios()).expect("crear repositories vacío");

        let resultado = estado(&rutas, &id("jparga", "repo1"), &PuntoDePartida::default()).await;
        assert!(matches!(resultado, Err(ErrorContingencia::NoActivada(_))));
    }

    #[tokio::test]
    async fn estado_de_mirror_relee_el_punto_de_partida_del_bare_del_mirror() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let repositorios = rutas.gitea_repositorios();

        // Bare del mirror original, pausado tras un único commit.
        let trabajo = tempfile::tempdir().expect("directorio de trabajo temporal");
        git_de_prueba(
            trabajo.path(),
            &["-c", "init.defaultBranch=main", "init", "-q"],
        )
        .await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "inicial"],
        )
        .await;
        let bare_mirror = repositorios.join("jparga").join("repo1.git");
        std::fs::create_dir_all(bare_mirror.parent().expect("padre"))
            .expect("crear carpeta del dueño");
        git::ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                trabajo.path().to_str().expect("utf8"),
                bare_mirror.to_str().expect("utf8"),
            ],
            &Opciones::default(),
        )
        .await
        .expect("clonar el mirror en bare");

        // Bare de contingencia: clon del mirror con un commit de más.
        let bare_contingencia = repositorios.join("contingencia-jparga").join("repo1.git");
        std::fs::create_dir_all(bare_contingencia.parent().expect("padre"))
            .expect("crear carpeta de contingencia");
        git::ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                bare_mirror.to_str().expect("utf8"),
                bare_contingencia.to_str().expect("utf8"),
            ],
            &Opciones::default(),
        )
        .await
        .expect("clonar la contingencia en bare");
        let clon_trabajo = tempfile::tempdir().expect("clon de trabajo");
        git_de_prueba(
            clon_trabajo.path(),
            &[
                "clone",
                "-q",
                bare_contingencia.to_str().expect("utf8"),
                ".",
            ],
        )
        .await;
        git_de_prueba(
            clon_trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "de más"],
        )
        .await;
        git_de_prueba(clon_trabajo.path(), &["push", "-q", "origin", "main"]).await;

        let estado_actual = estado_de_mirror(&rutas, &id("jparga", "repo1"))
            .await
            .expect("estado_de_mirror no falla");

        assert_eq!(estado_actual.ramas.len(), 1);
        assert_eq!(estado_actual.ramas[0].commits_de_mas, 1);
    }

    #[tokio::test]
    async fn estado_de_mirror_falla_si_el_mirror_no_existe() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        std::fs::create_dir_all(rutas.gitea_repositorios()).expect("crear repositories vacío");

        let resultado = estado_de_mirror(&rutas, &id("jparga", "repo1")).await;
        assert!(matches!(
            resultado,
            Err(ErrorContingencia::RutaBareInvalida(_))
        ));
    }
}
