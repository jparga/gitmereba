//! `capturar`: toma una foto de las refs de un repo y la asegura en el bare propio de
//! snapshots, sin tocar nunca el bare de Gitea.

use time::OffsetDateTime;

use crate::config::RutasCuenta;
use crate::git::{self, Opciones};
use crate::modelo::IdRepo;

use super::comparacion::{filtrar_ramas_y_tags, mismas_refs};
use super::error::ErrorSnapshots;
use super::manifiesto::{
    DIR_MANIFIESTOS, Manifiesto, escribir_manifiesto, generar_marca_unica, leer_todos,
};
use super::rutas::{asegurar_directorio_bare_destino, ruta_bare_origen, ruta_candidata};

/// Resultado de [`capturar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captura {
    /// Se ha creado una captura nueva.
    Nueva(Manifiesto),
    /// Las refs no han cambiado desde la última captura: no se ha creado nada.
    SinCambios,
}

/// Configura el bare de snapshots para que nada de lo alcanzable por `refs/snapshots/*`
/// se recolecte nunca por accidente (lo referenciado por esas refs no se poda de todos
/// modos, pero esto evita además un `gc` automático a mitad de una captura).
async fn configurar_bare_destino(destino: &std::path::Path) -> Result<(), ErrorSnapshots> {
    let opciones = Opciones {
        directorio: Some(destino.to_path_buf()),
        ..Opciones::default()
    };
    git::ejecutar(&["config", "--local", "gc.auto", "0"], &opciones).await?;
    git::ejecutar(&["config", "--local", "gc.pruneExpire", "never"], &opciones).await?;
    Ok(())
}

/// Captura las refs de `id` (bajo `rutas.gitea_repositorios()`) en su bare propio de
/// snapshots (bajo `rutas.snapshots()`), creándolo si es la primera vez.
///
/// Si las refs son idénticas a las de la última captura, no se crea nada nuevo y se
/// devuelve [`Captura::SinCambios`]: llamar a `capturar` en cada pasada de
/// sincronización es barato. Nunca escribe en el bare de origen.
pub async fn capturar(
    rutas: &RutasCuenta,
    id: &IdRepo,
    ahora: OffsetDateTime,
) -> Result<Captura, ErrorSnapshots> {
    let origen = ruta_bare_origen(&rutas.gitea_repositorios(), id)?;
    let refs_origen = filtrar_ramas_y_tags(git::refs(&origen).await?);

    let candidata_destino = ruta_candidata(&rutas.snapshots(), id);
    let anteriores = if candidata_destino.exists() {
        leer_todos(&candidata_destino.join(DIR_MANIFIESTOS))?
    } else {
        Vec::new()
    };
    if let Some(ultima) = anteriores.last()
        && mismas_refs(&ultima.refs, &refs_origen)
    {
        return Ok(Captura::SinCambios);
    }

    let destino = asegurar_directorio_bare_destino(&rutas.snapshots(), id)?;
    let opciones_destino = Opciones {
        directorio: Some(destino.clone()),
        ..Opciones::default()
    };
    git::ejecutar(&["init", "--bare", "-q"], &opciones_destino).await?;
    configurar_bare_destino(&destino).await?;

    let marca = generar_marca_unica(&anteriores, ahora);
    let origen_str = origen
        .to_str()
        .ok_or(ErrorSnapshots::RutaNoUtf8)?
        .to_string();
    let refspec_heads = format!("+refs/heads/*:refs/snapshots/{marca}/heads/*");
    let refspec_tags = format!("+refs/tags/*:refs/snapshots/{marca}/tags/*");
    // `--no-tags` evita que git siga automáticamente tags que apunten a los commits
    // recién traídos: solo queremos lo que dicen los dos refspecs explícitos, todo
    // bajo el espacio propio de esta marca.
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

    let manifiesto = Manifiesto {
        momento: ahora.to_offset(time::UtcOffset::UTC),
        id: marca.clone(),
        refs: refs_origen,
        protegida: false,
    };
    escribir_manifiesto(
        &destino.join(DIR_MANIFIESTOS).join(format!("{marca}.json")),
        &manifiesto,
    )?;

    Ok(Captura::Nueva(manifiesto))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use crate::modelo::Nombre;

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

    /// Crea un bare «de Gitea» en `raiz/<dueno>/<nombre>.git` con un commit en `main`.
    async fn crear_bare_origen(
        raiz: &Path,
        dueno: &str,
        nombre: &str,
    ) -> (std::path::PathBuf, String) {
        let trabajo = tempfile::tempdir().expect("directorio temporal de trabajo");
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
        (bare, sha)
    }

    fn rutas_de(raiz: &Path) -> RutasCuenta {
        RutasCuenta::nueva(raiz.join("cuenta"))
    }

    fn ahora() -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }

    #[tokio::test]
    async fn primera_captura_crea_un_manifiesto_con_las_refs() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let (_, sha) = crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        let captura = capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("capturar no falla");

        match captura {
            Captura::Nueva(manifiesto) => {
                assert_eq!(manifiesto.refs, vec![("refs/heads/main".to_string(), sha)]);
                assert!(!manifiesto.protegida);
            }
            Captura::SinCambios => panic!("la primera captura no puede ser «sin cambios»"),
        }
    }

    #[tokio::test]
    async fn una_segunda_captura_sin_cambios_no_crea_nada() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("primera captura");
        let manifiestos_dir =
            ruta_candidata(&rutas.snapshots(), &id("jparga", "repo1")).join(DIR_MANIFIESTOS);
        let contados_antes = std::fs::read_dir(&manifiestos_dir)
            .expect("leer manifiestos")
            .count();

        let segunda = capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("segunda captura no falla");

        assert_eq!(segunda, Captura::SinCambios);
        let contados_despues = std::fs::read_dir(&manifiestos_dir)
            .expect("leer manifiestos")
            .count();
        assert_eq!(contados_antes, contados_despues);
    }

    #[tokio::test]
    async fn capturar_deja_permisos_0700_y_0600() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        let captura = capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("capturar no falla");
        let Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };

        let destino = ruta_candidata(&rutas.snapshots(), &id("jparga", "repo1"));
        let permisos_destino = std::fs::metadata(&destino).expect("metadata").permissions();
        assert_eq!(permisos_destino.mode() & 0o777, 0o700);

        let ruta_manifiesto = destino
            .join(DIR_MANIFIESTOS)
            .join(format!("{}.json", manifiesto.id));
        let permisos_manifiesto = std::fs::metadata(&ruta_manifiesto)
            .expect("metadata")
            .permissions();
        assert_eq!(permisos_manifiesto.mode() & 0o777, 0o600);
    }

    #[tokio::test]
    async fn capturar_no_modifica_el_bare_de_origen() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let (bare, _) = crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        let refs_antes = git::refs(&bare).await.expect("refs antes");
        let mut ficheros_antes: Vec<_> = listar_ficheros(&bare);

        capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("capturar no falla");

        let refs_despues = git::refs(&bare).await.expect("refs después");
        let mut ficheros_despues: Vec<_> = listar_ficheros(&bare);
        ficheros_antes.sort();
        ficheros_despues.sort();

        assert_eq!(refs_antes, refs_despues);
        assert_eq!(ficheros_antes, ficheros_despues);
    }

    fn listar_ficheros(raiz: &Path) -> Vec<std::path::PathBuf> {
        let mut resultado = Vec::new();
        let mut pendientes = vec![raiz.to_path_buf()];
        while let Some(directorio) = pendientes.pop() {
            for entrada in std::fs::read_dir(&directorio).expect("leer directorio") {
                let entrada = entrada.expect("entrada");
                let ruta = entrada.path();
                if entrada.file_type().expect("tipo").is_dir() {
                    pendientes.push(ruta);
                } else {
                    resultado.push(ruta);
                }
            }
        }
        resultado
    }

    #[tokio::test]
    async fn capturar_rechaza_un_origen_que_no_existe() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        std::fs::create_dir_all(rutas.gitea_repositorios()).expect("crear repositories");

        let resultado = capturar(&rutas, &id("jparga", "no-existe"), ahora()).await;
        assert!(matches!(resultado, Err(ErrorSnapshots::OrigenNoExiste)));
    }

    #[tokio::test]
    async fn capturar_rechaza_un_enlace_simbolico_que_escapa_de_repositories() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        std::fs::create_dir_all(rutas.gitea_repositorios().join("jparga"))
            .expect("crear carpeta del dueño");
        let fuera = tempfile::tempdir().expect("directorio fuera");
        std::os::unix::fs::symlink(
            fuera.path(),
            rutas.gitea_repositorios().join("jparga").join("repo1.git"),
        )
        .expect("crear enlace simbólico");

        let resultado = capturar(&rutas, &id("jparga", "repo1"), ahora()).await;
        assert!(matches!(
            resultado,
            Err(ErrorSnapshots::OrigenFueraDeRepositorios)
        ));
        // No se ha llegado a crear nada en `snapshots/`: la validación corta antes de
        // ejecutar ningún `git` sobre la ruta que escapaba.
        assert!(!rutas.snapshots().exists());
    }

    #[tokio::test]
    async fn una_captura_tras_un_cambio_real_crea_una_marca_nueva() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let (bare, _) = crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("primera captura");

        // Un commit nuevo llega al bare de origen como llegaría de un push real: se
        // clona a un árbol de trabajo, se añade el commit y se empuja de vuelta (un
        // bare no tiene árbol de trabajo, así que no se puede usar `git commit` ahí
        // directamente).
        let trabajo = tempfile::tempdir().expect("árbol de trabajo");
        git::ejecutar(
            &["clone", "-q", bare.to_str().expect("utf8"), "."],
            &Opciones {
                directorio: Some(trabajo.path().to_path_buf()),
                ..Opciones::default()
            },
        )
        .await
        .expect("clonar para añadir un commit");
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "segundo"],
        )
        .await;
        git_de_prueba(trabajo.path(), &["push", "-q", "origin", "main"]).await;

        let segunda = capturar(&rutas, &id("jparga", "repo1"), ahora())
            .await
            .expect("segunda captura no falla");
        assert!(matches!(segunda, Captura::Nueva(_)));
    }
}
