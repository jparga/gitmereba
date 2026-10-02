//! `compactar`: recolecta de verdad los objetos que ya no referencia ninguna captura.
//!
//! `rotar` no ejecuta esto por sí solo (borrar refs es barato; un `gc --prune=now` no lo
//! es), así que compactar es una decisión explícita y aparte.

use crate::config::RutasCuenta;
use crate::git::{self, Opciones};
use crate::modelo::IdRepo;

use super::error::ErrorSnapshots;
use super::rutas::ruta_bare_snapshot_existente;

/// Ejecuta `git gc --prune=now` en el bare de snapshots de `id`. Lo alcanzable desde
/// `refs/snapshots/*` de una captura viva nunca se pierde: `gc` solo se lleva lo que ya
/// no referencia ninguna ref.
pub async fn compactar(rutas: &RutasCuenta, id: &IdRepo) -> Result<(), ErrorSnapshots> {
    let destino = ruta_bare_snapshot_existente(&rutas.snapshots(), id)?;
    let opciones = Opciones {
        directorio: Some(destino),
        ..Opciones::default()
    };
    git::ejecutar(&["gc", "--prune=now", "--quiet"], &opciones).await?;
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

    async fn crear_bare_origen(raiz: &std::path::Path, dueno: &str, nombre: &str) {
        let trabajo = tempfile::tempdir().expect("árbol de trabajo");
        let opciones = Opciones {
            directorio: Some(trabajo.path().to_path_buf()),
            ..Opciones::default()
        };
        git::ejecutar(&["-c", "init.defaultBranch=main", "init", "-q"], &opciones)
            .await
            .expect("init");
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
        .expect("commit");
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
    }

    #[tokio::test]
    async fn compactar_no_pierde_capturas_vivas() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");

        let captura = capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");
        let Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        compactar(&rutas, &repo).await.expect("compactar no falla");

        let destino = ruta_bare_snapshot_existente(&rutas.snapshots(), &repo).expect("destino");
        let refs = git::refs(&destino).await.expect("refs tras compactar");
        let (_, sha) = &manifiesto.refs[0];
        assert!(
            refs.iter().any(|(nombre, s)| {
                nombre == &format!("refs/snapshots/{}/heads/main", manifiesto.id) && s == sha
            }),
            "{refs:?}"
        );
        // El objeto del commit capturado sigue siendo accesible tras el `gc`.
        git::ejecutar(
            &["cat-file", "-e", sha],
            &Opciones {
                directorio: Some(destino),
                ..Opciones::default()
            },
        )
        .await
        .expect("el objeto sigue existiendo tras compactar");
    }
}
