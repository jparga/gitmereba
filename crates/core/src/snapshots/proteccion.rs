//! Marcar y desmarcar una captura como protegida: `rotar` nunca borra una captura
//! protegida, aunque le tocase por antigüedad.

use crate::config::RutasCuenta;
use crate::modelo::IdRepo;

use super::error::ErrorSnapshots;
use super::manifiesto::{DIR_MANIFIESTOS, escribir_manifiesto, leer_manifiesto, validar_marca};
use super::rutas::ruta_bare_snapshot_existente;

async fn fijar_protegida(
    rutas: &RutasCuenta,
    id: &IdRepo,
    marca: &str,
    valor: bool,
) -> Result<(), ErrorSnapshots> {
    validar_marca(marca)?;
    let destino = ruta_bare_snapshot_existente(&rutas.snapshots(), id)?;
    let ruta_manifiesto = destino.join(DIR_MANIFIESTOS).join(format!("{marca}.json"));
    if !ruta_manifiesto.exists() {
        return Err(ErrorSnapshots::CapturaNoEncontrada(marca.to_string()));
    }
    let mut manifiesto = leer_manifiesto(&ruta_manifiesto)?;
    manifiesto.protegida = valor;
    escribir_manifiesto(&ruta_manifiesto, &manifiesto)
}

/// Marca la captura `marca` de `id` como protegida: `rotar` no la borrará hasta que se
/// llame a [`liberar`]. Se usa tras detectar un cambio destructivo, sobre la captura
/// inmediatamente anterior a él.
pub async fn proteger(rutas: &RutasCuenta, id: &IdRepo, marca: &str) -> Result<(), ErrorSnapshots> {
    fijar_protegida(rutas, id, marca, true).await
}

/// Quita la protección de la captura `marca` de `id`: vuelve a quedar sujeta a la
/// política de retención normal en la siguiente llamada a `rotar`.
pub async fn liberar(rutas: &RutasCuenta, id: &IdRepo, marca: &str) -> Result<(), ErrorSnapshots> {
    fijar_protegida(rutas, id, marca, false).await
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use crate::modelo::Nombre;
    use crate::snapshots::capturar;

    use super::*;

    fn id(dueno: &str, nombre: &str) -> IdRepo {
        IdRepo {
            dueno: Nombre::nuevo(dueno).expect("dueño válido"),
            nombre: Nombre::nuevo(nombre).expect("nombre válido"),
        }
    }

    async fn crear_bare_origen(raiz: &std::path::Path, dueno: &str, nombre: &str) {
        use crate::git::{self, Opciones};
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
    async fn proteger_y_liberar_cambian_el_manifiesto_en_disco() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");

        let captura = capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");
        let crate::snapshots::Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        proteger(&rutas, &repo, &manifiesto.id)
            .await
            .expect("proteger no falla");
        let listado = crate::snapshots::listar(&rutas, &repo).expect("listar no falla");
        assert!(listado[0].protegida);

        liberar(&rutas, &repo, &manifiesto.id)
            .await
            .expect("liberar no falla");
        let listado = crate::snapshots::listar(&rutas, &repo).expect("listar no falla");
        assert!(!listado[0].protegida);
    }

    #[tokio::test]
    async fn proteger_una_marca_inexistente_falla() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let repo = id("jparga", "repo1");
        capturar(&rutas, &repo, OffsetDateTime::now_utc())
            .await
            .expect("capturar no falla");

        let resultado = proteger(&rutas, &repo, "20200101T000000Z").await;
        assert!(matches!(
            resultado,
            Err(ErrorSnapshots::CapturaNoEncontrada(_))
        ));
    }

    #[tokio::test]
    async fn proteger_rechaza_una_marca_con_formato_invalido() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let repo = id("jparga", "repo1");

        let resultado = proteger(&rutas, &repo, "../../etc/passwd").await;
        assert!(matches!(resultado, Err(ErrorSnapshots::MarcaInvalida(_))));
    }
}
