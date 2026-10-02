//! Helpers de alto nivel sobre `git`, todos validando sus entradas.

use std::path::Path;

use crate::git::proceso::{ErrorGit, Opciones, ejecutar};
use crate::git::validar::validar_ref;

/// Versión de git instalado, tal como la reporta `git --version`.
pub async fn version() -> Result<String, ErrorGit> {
    let salida = ejecutar(&["--version"], &Opciones::default()).await?;
    Ok(salida.stdout.trim().to_string())
}

/// Comprueba la integridad de un repositorio bare con `git fsck`.
///
/// Devuelve error si `git fsck` reporta cualquier problema (objeto corrupto, enlace
/// roto, etc.), no solo si no puede ejecutarse.
pub async fn fsck(ruta_bare: &Path) -> Result<(), ErrorGit> {
    let opciones = Opciones {
        directorio: Some(ruta_bare.to_path_buf()),
        ..Opciones::default()
    };
    ejecutar(&["fsck", "--no-progress"], &opciones).await?;
    Ok(())
}

/// Lista las referencias de un repositorio bare como pares `(nombre, sha)`.
pub async fn refs(ruta_bare: &Path) -> Result<Vec<(String, String)>, ErrorGit> {
    let opciones = Opciones {
        directorio: Some(ruta_bare.to_path_buf()),
        ..Opciones::default()
    };
    let salida = ejecutar(
        &["for-each-ref", "--format=%(refname) %(objectname)"],
        &opciones,
    )
    .await?;
    Ok(salida
        .stdout
        .lines()
        .filter_map(|linea| linea.split_once(' '))
        .map(|(nombre, sha)| (nombre.to_string(), sha.to_string()))
        .collect())
}

/// Cuántos commits tiene `rama_local` que no están en `ref_remota`.
pub async fn commits_de_mas(
    ruta: &Path,
    rama_local: &str,
    ref_remota: &str,
) -> Result<u64, ErrorGit> {
    validar_ref(rama_local)?;
    validar_ref(ref_remota)?;
    let rango = format!("{ref_remota}..{rama_local}");
    let opciones = Opciones {
        directorio: Some(ruta.to_path_buf()),
        ..Opciones::default()
    };
    let salida = ejecutar(
        &["rev-list", "--count", "--end-of-options", &rango],
        &opciones,
    )
    .await?;
    salida
        .stdout
        .trim()
        .parse::<u64>()
        .map_err(|_| ErrorGit::Sistema("salida inesperada de «rev-list --count»".to_string()))
}

/// Si `a` es antepasado de `b` en el historial de `ruta`.
pub async fn es_ancestro(ruta: &Path, a: &str, b: &str) -> Result<bool, ErrorGit> {
    validar_ref(a)?;
    validar_ref(b)?;
    let opciones = Opciones {
        directorio: Some(ruta.to_path_buf()),
        ..Opciones::default()
    };
    match ejecutar(
        &["merge-base", "--is-ancestor", "--end-of-options", a, b],
        &opciones,
    )
    .await
    {
        Ok(_) => Ok(true),
        // `merge-base --is-ancestor` usa el código 1 para «no es antepasado», no para
        // un fallo real; cualquier otro código sí lo es (repo inválido, refs que no
        // existen, etc.) y se propaga.
        Err(ErrorGit::Fallo { codigo: 1, .. }) => Ok(false),
        Err(otro) => Err(otro),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::*;

    /// Ejecuta `git` con `user.name`/`user.email` fijos, para no depender de la
    /// configuración global de la máquina que corre los tests.
    async fn git_de_prueba(directorio: &Path, args: &[&str]) -> String {
        let mut completos = vec!["-c", "user.name=Test", "-c", "user.email=test@test.invalid"];
        completos.extend_from_slice(args);
        let opciones = Opciones {
            directorio: Some(directorio.to_path_buf()),
            ..Opciones::default()
        };
        ejecutar(&completos, &opciones)
            .await
            .unwrap_or_else(|error| panic!("`git {args:?}` falló: {error}"))
            .stdout
    }

    /// Crea un repo de trabajo con dos commits y devuelve el directorio temporal y
    /// los SHA de cada commit (primero, segundo).
    async fn repo_con_dos_commits() -> (TempDir, String, String) {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        git_de_prueba(
            directorio.path(),
            &["-c", "init.defaultBranch=main", "init", "-q"],
        )
        .await;
        git_de_prueba(
            directorio.path(),
            &["commit", "--allow-empty", "-q", "-m", "uno"],
        )
        .await;
        let sha_uno = git_de_prueba(directorio.path(), &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_string();
        git_de_prueba(
            directorio.path(),
            &["commit", "--allow-empty", "-q", "-m", "dos"],
        )
        .await;
        let sha_dos = git_de_prueba(directorio.path(), &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_string();
        (directorio, sha_uno, sha_dos)
    }

    #[tokio::test]
    async fn version_devuelve_algo_con_git_version() {
        let texto = version().await.expect("version no falla");
        assert!(texto.starts_with("git version"), "{texto}");
    }

    #[tokio::test]
    async fn fsck_de_un_repo_sano_es_ok() {
        let (directorio, _, _) = repo_con_dos_commits().await;
        fsck(directorio.path())
            .await
            .expect("fsck de un repo sano no falla");
    }

    #[tokio::test]
    async fn fsck_de_un_repo_corrupto_devuelve_error() {
        let (directorio, sha_uno, _) = repo_con_dos_commits().await;

        // Corrompe el objeto del primer commit machacando su contenido. Git guarda los
        // objetos en solo lectura, así que hay que devolverles el permiso de escritura
        // antes de poder machacarlos.
        use std::os::unix::fs::PermissionsExt;
        let ruta_objeto = directorio
            .path()
            .join(".git/objects")
            .join(&sha_uno[..2])
            .join(&sha_uno[2..]);
        let mut permisos = std::fs::metadata(&ruta_objeto)
            .expect("metadata del objeto")
            .permissions();
        permisos.set_mode(0o644);
        std::fs::set_permissions(&ruta_objeto, permisos).expect("permitir escritura en el objeto");
        std::fs::write(&ruta_objeto, b"esto ya no es un objeto git valido")
            .expect("corromper objeto");

        let resultado = fsck(directorio.path()).await;
        assert!(
            resultado.is_err(),
            "fsck debería fallar con un objeto corrupto"
        );
    }

    #[tokio::test]
    async fn refs_lista_las_referencias_del_bare() {
        let (directorio, _, sha_dos) = repo_con_dos_commits().await;
        let bare = tempfile::tempdir().expect("directorio temporal");
        // El propio directorio del bare aún no existe como repo git, así que se clona
        // en el directorio temporal (que `git clone` puede usar si está vacío).
        ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                directorio.path().to_str().expect("utf8"),
                ".",
            ],
            &Opciones {
                directorio: Some(bare.path().to_path_buf()),
                ..Opciones::default()
            },
        )
        .await
        .expect("clonar en bare no falla");

        let listado = refs(bare.path()).await.expect("refs no falla");
        assert!(
            listado
                .iter()
                .any(|(nombre, sha)| nombre == "refs/heads/main" && sha == &sha_dos),
            "{listado:?}"
        );
    }

    #[tokio::test]
    async fn commits_de_mas_cuenta_los_commits_no_compartidos() {
        let (directorio, sha_uno, _) = repo_con_dos_commits().await;
        // «sha_uno..HEAD» tiene un único commit de más (el segundo).
        let de_mas = commits_de_mas(directorio.path(), "HEAD", &sha_uno)
            .await
            .expect("commits_de_mas no falla");
        assert_eq!(de_mas, 1);
    }

    #[tokio::test]
    async fn commits_de_mas_es_cero_si_estan_al_dia() {
        let (directorio, _, sha_dos) = repo_con_dos_commits().await;
        let de_mas = commits_de_mas(directorio.path(), "HEAD", &sha_dos)
            .await
            .expect("commits_de_mas no falla");
        assert_eq!(de_mas, 0);
    }

    #[tokio::test]
    async fn commits_de_mas_rechaza_referencias_invalidas() {
        let directorio = PathBuf::from("/no-existe");
        let resultado = commits_de_mas(&directorio, "--upload-pack=x", "HEAD").await;
        assert!(matches!(resultado, Err(ErrorGit::EntradaInvalida(_))));
    }

    #[tokio::test]
    async fn es_ancestro_es_true_para_un_commit_anterior() {
        let (directorio, sha_uno, sha_dos) = repo_con_dos_commits().await;
        assert!(
            es_ancestro(directorio.path(), &sha_uno, &sha_dos)
                .await
                .expect("es_ancestro no falla")
        );
    }

    #[tokio::test]
    async fn es_ancestro_es_false_al_reves() {
        let (directorio, sha_uno, sha_dos) = repo_con_dos_commits().await;
        assert!(
            !es_ancestro(directorio.path(), &sha_dos, &sha_uno)
                .await
                .expect("es_ancestro no falla")
        );
    }

    #[tokio::test]
    async fn es_ancestro_rechaza_referencias_invalidas() {
        let directorio = PathBuf::from("/no-existe");
        let resultado = es_ancestro(&directorio, "a..b", "HEAD").await;
        assert!(matches!(resultado, Err(ErrorGit::EntradaInvalida(_))));
    }
}
