//! `restaurar_en`: recupera una captura como un repo bare normal, fuera de Gitea.

use std::path::Path;

use crate::config::RutasCuenta;
use crate::git::{self, Opciones};
use crate::modelo::IdRepo;

use super::comparacion::mismas_refs;
use super::error::ErrorSnapshots;
use super::fichero::crear_directorio_privado;
use super::manifiesto::{DIR_MANIFIESTOS, leer_manifiesto, validar_marca};
use super::rutas::{ruta_bare_snapshot_existente, validar_destino_restauracion};

/// Recupera la captura `marca` de `id` en un repo bare nuevo en `destino`, con las
/// ramas y tags de esa captura como `refs/heads/*` y `refs/tags/*` normales.
///
/// `destino` no debe existir ya, ni estar dentro de `gitea/` ni de `snapshots/`. Esta
/// función nunca escribe en Gitea ni empuja a ningún sitio: dos usos típicos del repo
/// resultante son (a) inspeccionarlo con `git log`/`git show` tal cual queda, en modo
/// solo lectura, o (b) que el usuario decida hacerle `git push` a mano a donde quiera
/// (por ejemplo, de vuelta al bare de Gitea, una vez a salvo de lo que provocó la
/// captura).
pub async fn restaurar_en(
    rutas: &RutasCuenta,
    id: &IdRepo,
    marca: &str,
    destino: &Path,
) -> Result<(), ErrorSnapshots> {
    validar_marca(marca)?;
    validar_destino_restauracion(rutas, destino)?;

    let origen = ruta_bare_snapshot_existente(&rutas.snapshots(), id)?;
    let ruta_manifiesto = origen.join(DIR_MANIFIESTOS).join(format!("{marca}.json"));
    if !ruta_manifiesto.exists() {
        return Err(ErrorSnapshots::CapturaNoEncontrada(marca.to_string()));
    }
    let manifiesto = leer_manifiesto(&ruta_manifiesto)?;

    crear_directorio_privado(destino)?;
    let opciones_destino = Opciones {
        directorio: Some(destino.to_path_buf()),
        ..Opciones::default()
    };
    git::ejecutar(&["init", "--bare", "-q"], &opciones_destino).await?;

    let origen_str = origen
        .to_str()
        .ok_or(ErrorSnapshots::RutaNoUtf8)?
        .to_string();
    let refspec_heads = format!("+refs/snapshots/{marca}/heads/*:refs/heads/*");
    let refspec_tags = format!("+refs/snapshots/{marca}/tags/*:refs/tags/*");
    git::ejecutar(
        &[
            "fetch",
            "--no-tags",
            "--quiet",
            &origen_str,
            &refspec_heads,
            &refspec_tags,
        ],
        &opciones_destino,
    )
    .await?;

    let refs_restauradas = git::refs(destino).await?;
    if !mismas_refs(&manifiesto.refs, &refs_restauradas) {
        return Err(ErrorSnapshots::RestauracionInconsistente);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use crate::modelo::Nombre;
    use crate::snapshots::{Captura, capturar};

    use super::*;

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

    async fn crear_bare_origen(raiz: &Path, dueno: &str, nombre: &str) -> std::path::PathBuf {
        let trabajo = tempfile::tempdir().expect("árbol de trabajo");
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
        let bare = raiz
            .join(dueno.to_lowercase())
            .join(format!("{}.git", nombre.to_lowercase()));
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
        bare
    }

    fn rutas_de(raiz: &Path) -> RutasCuenta {
        RutasCuenta::nueva(raiz.join("cuenta"))
    }

    #[tokio::test]
    async fn restaura_una_captura_con_sus_ramas_y_tags() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");

        let captura = capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");
        let Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        let destino = raiz.path().join("restaurado.git");
        restaurar_en(&rutas, &repo, &manifiesto.id, &destino)
            .await
            .expect("restaurar_en no falla");

        let refs = git::refs(&destino).await.expect("refs del restaurado");
        assert_eq!(refs, manifiesto.refs);
    }

    #[tokio::test]
    async fn restaurar_en_un_destino_existente_falla() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");
        let captura = capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");
        let Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        let destino = raiz.path().join("ya-existe");
        std::fs::create_dir_all(&destino).expect("crear destino");

        let resultado = restaurar_en(&rutas, &repo, &manifiesto.id, &destino).await;
        assert!(matches!(resultado, Err(ErrorSnapshots::DestinoExiste)));
    }

    #[tokio::test]
    async fn restaurar_dentro_de_gitea_falla() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");
        let captura = capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");
        let Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        let destino = rutas.gitea().join("restaurado.git");
        let resultado = restaurar_en(&rutas, &repo, &manifiesto.id, &destino).await;
        assert!(matches!(
            resultado,
            Err(ErrorSnapshots::DestinoDentroDeGiteaOSnapshots)
        ));
    }

    #[tokio::test]
    async fn restaurar_dentro_de_snapshots_falla() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");
        let captura = capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");
        let Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        let destino = rutas.snapshots().join("restaurado.git");
        let resultado = restaurar_en(&rutas, &repo, &manifiesto.id, &destino).await;
        assert!(matches!(
            resultado,
            Err(ErrorSnapshots::DestinoDentroDeGiteaOSnapshots)
        ));
    }

    #[tokio::test]
    async fn restaurar_una_marca_con_formato_invalido_falla() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let repo = id("jparga", "repo1");
        let destino = raiz.path().join("restaurado.git");

        let resultado = restaurar_en(&rutas, &repo, "../../etc/passwd", &destino).await;
        assert!(matches!(resultado, Err(ErrorSnapshots::MarcaInvalida(_))));
        assert!(!destino.exists());
    }

    #[tokio::test]
    async fn restaurar_una_captura_inexistente_falla() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");
        capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");

        let destino = raiz.path().join("restaurado.git");
        let resultado = restaurar_en(&rutas, &repo, "20200101T000000Z", &destino).await;
        assert!(matches!(
            resultado,
            Err(ErrorSnapshots::CapturaNoEncontrada(_))
        ));
    }
}
