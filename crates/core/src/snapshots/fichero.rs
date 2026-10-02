//! Escritura atómica de ficheros privados, con permisos fijados a 0600/0700.
//!
//! Mismo patrón que `crate::config::fichero` (que es `pub(super)` y no se puede
//! reutilizar desde aquí): fichero temporal + `rename` atómico en el mismo directorio,
//! permisos fijados en la propia creación, nunca tras el hecho.

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

use super::error::ErrorSnapshots;

/// Permisos de los directorios que crea este módulo.
const MODO_DIRECTORIO: u32 = 0o700;
/// Permisos de los ficheros que crea este módulo (los manifiestos).
const MODO_FICHERO: u32 = 0o600;

/// Escribe `contenido` en `ruta` de forma atómica y con permisos 0600. Crea el
/// directorio padre (0700) si falta.
pub(super) fn escribir_privado(ruta: &Path, contenido: &[u8]) -> Result<(), ErrorSnapshots> {
    let directorio = ruta.parent().ok_or_else(|| {
        ErrorSnapshots::Io(format!("«{}» no tiene directorio padre", ruta.display()))
    })?;
    crear_directorio_privado(directorio)?;

    let temporal = fichero_temporal(ruta);
    escribir_fichero_nuevo(&temporal, contenido)?;
    fs::rename(&temporal, ruta).map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    Ok(())
}

/// Crea `directorio` (y los que falten) con permisos 0700 si no existe ya.
pub(super) fn crear_directorio_privado(directorio: &Path) -> Result<(), ErrorSnapshots> {
    if directorio.exists() {
        return Ok(());
    }
    DirBuilder::new()
        .recursive(true)
        .mode(MODO_DIRECTORIO)
        .create(directorio)
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))
}

/// Ruta del fichero temporal usado como paso intermedio de la escritura atómica.
fn fichero_temporal(ruta: &Path) -> std::path::PathBuf {
    let mut nombre = ruta.as_os_str().to_owned();
    nombre.push(".tmp");
    std::path::PathBuf::from(nombre)
}

/// Crea `ruta` con permisos 0600 fijados en la propia creación y escribe `contenido`.
fn escribir_fichero_nuevo(ruta: &Path, contenido: &[u8]) -> Result<(), ErrorSnapshots> {
    let mut fichero = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(MODO_FICHERO)
        .open(ruta)
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    fichero
        .write_all(contenido)
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    fichero
        .sync_all()
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn escribe_y_deja_permisos_0600_en_el_fichero() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("sub").join("f.json");

        escribir_privado(&ruta, b"hola").expect("escribir_privado no falla");

        assert_eq!(fs::read(&ruta).expect("leer fichero"), b"hola");
        let permisos = fs::metadata(&ruta).expect("metadata").permissions();
        assert_eq!(permisos.mode() & 0o777, 0o600);
    }

    #[test]
    fn crea_el_directorio_padre_con_permisos_0700() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("cuenta").join("f.json");

        escribir_privado(&ruta, b"x").expect("escribir_privado no falla");

        let permisos = fs::metadata(ruta.parent().expect("padre"))
            .expect("metadata")
            .permissions();
        assert_eq!(permisos.mode() & 0o777, 0o700);
    }

    #[test]
    fn no_deja_ficheros_temporales_tras_escribir() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("f.json");

        escribir_privado(&ruta, b"x").expect("escribir_privado no falla");

        let temporales: Vec<_> = fs::read_dir(directorio.path())
            .expect("leer directorio")
            .filter_map(Result::ok)
            .filter(|entrada| entrada.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(temporales.is_empty(), "{temporales:?}");
    }

    #[test]
    fn una_segunda_escritura_reemplaza_el_contenido() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("f.json");

        escribir_privado(&ruta, b"primero").expect("escribir_privado no falla");
        escribir_privado(&ruta, b"segundo").expect("escribir_privado no falla");

        assert_eq!(fs::read(&ruta).expect("leer fichero"), b"segundo");
    }
}
