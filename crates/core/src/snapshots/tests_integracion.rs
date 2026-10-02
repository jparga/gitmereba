//! Escenario de extremo a extremo: captura → el mirror sufre un force-push destructivo que
//! borra de verdad el commit antiguo (con `git gc --prune=now`) → una segunda captura
//! detecta la reescritura de historia y se protege la captura anterior → restaurar esa
//! captura anterior recupera el commit que ya no existe en ningún otro sitio.

use std::path::Path;

use time::OffsetDateTime;

use crate::config::RutasCuenta;
use crate::git::{self, Opciones};
use crate::modelo::{IdRepo, Nombre};

use super::captura::{Captura, capturar};
use super::deteccion::{CambioDestructivo, detectar_en_bare};
use super::proteccion::proteger;
use super::restauracion::restaurar_en;
use super::rutas::ruta_bare_snapshot_existente;

fn id(dueno: &str, nombre: &str) -> IdRepo {
    IdRepo {
        dueno: Nombre::nuevo(dueno).expect("dueño válido"),
        nombre: Nombre::nuevo(nombre).expect("nombre válido"),
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
        .unwrap_or_else(|error| panic!("`git {args:?}` falló: {error}"))
        .stdout
}

/// Comprueba si `sha` sigue existiendo como objeto en el repo `ruta`.
async fn existe_objeto(ruta: &Path, sha: &str) -> bool {
    let opciones = Opciones {
        directorio: Some(ruta.to_path_buf()),
        ..Opciones::default()
    };
    git::ejecutar(&["cat-file", "-e", sha], &opciones)
        .await
        .is_ok()
}

#[tokio::test]
async fn recupera_un_commit_que_un_force_push_borro_de_verdad_del_mirror() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
    let repo = id("jparga", "repo1");

    // 1. El «mirror de Gitea»: un bare con un commit en `main`.
    let trabajo_inicial = tempfile::tempdir().expect("árbol de trabajo inicial");
    git_de_prueba(
        trabajo_inicial.path(),
        &["-c", "init.defaultBranch=main", "init", "-q"],
    )
    .await;
    git_de_prueba(
        trabajo_inicial.path(),
        &["commit", "--allow-empty", "-q", "-m", "commit antiguo"],
    )
    .await;
    let sha_antiguo = git_de_prueba(trabajo_inicial.path(), &["rev-parse", "HEAD"])
        .await
        .trim()
        .to_string();

    let mirror = rutas.gitea_repositorios().join("jparga").join("repo1.git");
    std::fs::create_dir_all(mirror.parent().expect("padre")).expect("crear carpeta del dueño");
    git::ejecutar(
        &[
            "clone",
            "--bare",
            "-q",
            trabajo_inicial.path().to_str().expect("utf8"),
            mirror.to_str().expect("utf8"),
        ],
        &Opciones::default(),
    )
    .await
    .expect("clonar el mirror inicial");

    // 2. Primera captura: se lleva el commit antiguo a nuestro bare propio.
    let primera = capturar(&rutas, &repo, OffsetDateTime::now_utc())
        .await
        .expect("primera captura no falla");
    let Captura::Nueva(manifiesto_primero) = primera else {
        panic!("se esperaba una captura nueva")
    };
    assert_eq!(
        manifiesto_primero.refs,
        vec![("refs/heads/main".to_string(), sha_antiguo.clone())]
    );

    // 3. Simula lo que hace Gitea al sincronizar tras un force-push a GitHub: un clon de
    // trabajo del mirror empuja con `--force` un commit sin relación (huérfano) sobre
    // `main`.
    let trabajo_ataque = tempfile::tempdir().expect("árbol de trabajo del ataque");
    git_de_prueba(
        trabajo_ataque.path(),
        &["clone", "-q", mirror.to_str().expect("utf8"), "."],
    )
    .await;
    git_de_prueba(
        trabajo_ataque.path(),
        &["checkout", "--orphan", "huerfano", "-q"],
    )
    .await;
    git_de_prueba(
        trabajo_ataque.path(),
        &["commit", "--allow-empty", "-q", "-m", "historia reescrita"],
    )
    .await;
    let sha_nuevo = git_de_prueba(trabajo_ataque.path(), &["rev-parse", "HEAD"])
        .await
        .trim()
        .to_string();
    git_de_prueba(
        trabajo_ataque.path(),
        &["push", "--force", "-q", "origin", "huerfano:main"],
    )
    .await;
    assert_ne!(sha_antiguo, sha_nuevo);

    // 4. `git gc --prune=now` en el propio mirror: el commit antiguo, ya inalcanzable
    // desde ninguna ref del mirror, desaparece de verdad de ahí.
    git_de_prueba(&mirror, &["gc", "--prune=now", "-q"]).await;
    assert!(
        !existe_objeto(&mirror, &sha_antiguo).await,
        "el commit antiguo debería haber desaparecido del mirror"
    );

    // 5. Segunda captura: refleja el nuevo (y único) estado del mirror.
    let segunda = capturar(&rutas, &repo, OffsetDateTime::now_utc())
        .await
        .expect("segunda captura no falla");
    let Captura::Nueva(manifiesto_segundo) = segunda else {
        panic!("se esperaba una captura nueva, las refs cambiaron")
    };
    assert_eq!(
        manifiesto_segundo.refs,
        vec![("refs/heads/main".to_string(), sha_nuevo.clone())]
    );

    // 6. Detección: nuestro bare propio conserva los objetos de ambas capturas (el
    // mirror ya no tiene el antiguo), así que puede decidir con `merge-base
    // --is-ancestor` que esto es una reescritura de historia, no un avance normal.
    let bare_snapshots = ruta_bare_snapshot_existente(&rutas.snapshots(), &repo)
        .expect("el bare de snapshots existe");
    let cambios = detectar_en_bare(
        &bare_snapshots,
        &manifiesto_primero,
        &manifiesto_segundo.refs,
    )
    .await
    .expect("detectar_en_bare no falla");
    assert_eq!(
        cambios,
        vec![CambioDestructivo::HistoriaReescrita {
            rama: "main".to_string(),
            antes: sha_antiguo.clone(),
            ahora: sha_nuevo.clone(),
        }]
    );

    // 7. Se protege la captura anterior al cambio destructivo.
    proteger(&rutas, &repo, &manifiesto_primero.id)
        .await
        .expect("proteger no falla");
    let listado = super::listado::listar(&rutas, &repo).expect("listar no falla");
    let resumen_primero = listado
        .iter()
        .find(|r| r.marca == manifiesto_primero.id)
        .expect("la primera captura sigue en el listado");
    assert!(resumen_primero.protegida);

    // 8. Restaurar la primera captura recupera el commit que ya no existe en ningún
    // otro sitio (ni en el mirror, ni en GitHub tras el force-push).
    let destino = raiz.path().join("recuperado.git");
    restaurar_en(&rutas, &repo, &manifiesto_primero.id, &destino)
        .await
        .expect("restaurar_en no falla");

    assert!(
        existe_objeto(&destino, &sha_antiguo).await,
        "el commit antiguo debe existir en el repo restaurado"
    );
    let refs_restauradas = git::refs(&destino).await.expect("refs del restaurado");
    assert_eq!(
        refs_restauradas,
        vec![("refs/heads/main".to_string(), sha_antiguo)]
    );
}
