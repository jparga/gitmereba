//! Detección barata de Git LFS en el bare de un mirror, para avisar en [`super::activar`]
//! cuando `git lfs` no está disponible: `push --mirror` no copia los objetos LFS, solo
//! los punteros de git, así que el repo de contingencia se queda sin ellos. Es una
//! advertencia, no un error.

use std::path::Path;

use crate::git::{self, Opciones};

/// `true` si `bare` usa Git LFS: hay al menos un objeto en `lfs/objects/` (comprobación
/// de disco, sin `git`) o el `.gitattributes` de `HEAD` menciona `filter=lfs` (un único
/// `git cat-file`, barato).
async fn usa_lfs(bare: &Path) -> bool {
    if directorio_no_vacio(&bare.join("lfs").join("objects")) {
        return true;
    }
    let opciones = Opciones {
        directorio: Some(bare.to_path_buf()),
        ..Opciones::default()
    };
    match git::ejecutar(&["cat-file", "-p", "HEAD:.gitattributes"], &opciones).await {
        Ok(salida) => salida.stdout.contains("filter=lfs"),
        Err(_) => false,
    }
}

fn directorio_no_vacio(ruta: &Path) -> bool {
    std::fs::read_dir(ruta)
        .map(|mut entradas| entradas.next().is_some())
        .unwrap_or(false)
}

/// `true` si `git lfs` está disponible en esta máquina.
async fn git_lfs_disponible() -> bool {
    git::ejecutar(&["lfs", "version"], &Opciones::default())
        .await
        .is_ok()
}

/// Advertencias sobre `bare` para el resultado de `activar`: una única entrada si usa
/// LFS y `git lfs` no está disponible; ninguna en cualquier otro caso.
pub(crate) async fn detectar_advertencias(bare: &Path) -> Vec<String> {
    if !usa_lfs(bare).await {
        return Vec::new();
    }
    advertencias_lfs(true, git_lfs_disponible().await)
}

/// Decisión pura de [`detectar_advertencias`], separada para probarla sin depender de si
/// la máquina tiene `git lfs` instalado.
fn advertencias_lfs(usa_lfs: bool, git_lfs_disponible: bool) -> Vec<String> {
    let mut advertencias = Vec::new();
    if usa_lfs && !git_lfs_disponible {
        advertencias.push(
            "el mirror usa Git LFS, pero «git lfs» no está disponible en esta máquina: \
             los objetos LFS no se han copiado al repo de contingencia, solo sus punteros"
                .to_string(),
        );
    }
    advertencias
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sin_carpeta_lfs_ni_gitattributes_no_hay_advertencia() {
        let bare = tempfile::tempdir().expect("directorio temporal");
        let advertencias = detectar_advertencias(bare.path()).await;
        assert!(advertencias.is_empty());
    }

    #[tokio::test]
    async fn con_un_objeto_lfs_en_disco_se_detecta_como_usado() {
        let bare = tempfile::tempdir().expect("directorio temporal");
        let objetos = bare.path().join("lfs").join("objects").join("ab");
        std::fs::create_dir_all(&objetos).expect("crear carpeta de objetos lfs");
        std::fs::write(objetos.join("cd1234"), b"contenido").expect("escribir objeto de prueba");

        assert!(usa_lfs(bare.path()).await);
    }

    #[tokio::test]
    async fn directorio_lfs_vacio_no_cuenta_como_usado() {
        let bare = tempfile::tempdir().expect("directorio temporal");
        std::fs::create_dir_all(bare.path().join("lfs").join("objects"))
            .expect("crear carpeta vacía");
        assert!(!usa_lfs(bare.path()).await);
    }

    #[test]
    fn avisa_solo_si_usa_lfs_y_git_lfs_no_esta_disponible() {
        assert_eq!(advertencias_lfs(true, false).len(), 1);
        assert!(advertencias_lfs(true, true).is_empty());
        assert!(advertencias_lfs(false, false).is_empty());
        assert!(advertencias_lfs(false, true).is_empty());
    }
}
