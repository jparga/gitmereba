//! Espacio en disco ocupado por una carpeta, para la tarjeta de resumen.

use std::fs;
use std::path::Path;

/// Error al calcular el espacio ocupado por una carpeta.
#[derive(Debug, thiserror::Error)]
pub enum ErrorEspacio {
    #[error("error de E/S al calcular el espacio en disco: {0}")]
    Io(String),
}

/// Suma el tamaño de todos los ficheros bajo `ruta`, recursivamente.
///
/// No sigue enlaces simbólicos (usa [`fs::symlink_metadata`]): un enlace cuenta su propio
/// tamaño (el de la ruta que guarda), nunca se recorre ni se cuenta el tamaño de su
/// destino, así un enlace que apunte fuera del árbol o a algo enorme no infla el resultado
/// ni hace que la función salga de `ruta`.
pub fn espacio_de(ruta: &Path) -> Result<u64, ErrorEspacio> {
    let metadata = fs::symlink_metadata(ruta).map_err(|e| ErrorEspacio::Io(e.to_string()))?;
    if !metadata.is_dir() {
        return Ok(metadata.len());
    }
    let mut total = 0u64;
    for entrada in fs::read_dir(ruta).map_err(|e| ErrorEspacio::Io(e.to_string()))? {
        let entrada = entrada.map_err(|e| ErrorEspacio::Io(e.to_string()))?;
        total += espacio_de(&entrada.path())?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn suma_los_ficheros_de_un_arbol_de_carpetas() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        fs::write(raiz.path().join("uno.txt"), b"12345").expect("escribir fichero");
        fs::create_dir(raiz.path().join("sub")).expect("crear subcarpeta");
        fs::write(raiz.path().join("sub").join("dos.txt"), b"1234567890")
            .expect("escribir fichero");

        let total = espacio_de(raiz.path()).expect("espacio_de no falla");
        assert_eq!(total, 5 + 10);
    }

    #[test]
    fn un_solo_fichero_devuelve_su_propio_tamano() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let fichero = raiz.path().join("solo.txt");
        fs::write(&fichero, b"1234567").expect("escribir fichero");

        assert_eq!(espacio_de(&fichero).expect("espacio_de no falla"), 7);
    }

    #[test]
    fn no_sigue_enlaces_simbolicos() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let objetivo = tempfile::tempdir().expect("directorio temporal para el destino");
        fs::write(objetivo.path().join("grande.bin"), vec![0u8; 100_000])
            .expect("escribir fichero grande");

        fs::write(raiz.path().join("real.txt"), b"12").expect("escribir fichero");
        symlink(objetivo.path(), raiz.path().join("enlace")).expect("crear enlace simbólico");

        let total = espacio_de(raiz.path()).expect("espacio_de no falla");
        // Solo cuenta «real.txt» (2 bytes) más el propio tamaño del enlace (la longitud de
        // la ruta que guarda), nunca los ~100 000 bytes del fichero al que apunta.
        assert!(
            total < 1_000,
            "no debería haber seguido el enlace: total = {total}"
        );
    }

    #[test]
    fn falla_si_la_ruta_no_existe() {
        let resultado = espacio_de(Path::new("/no/existe/de/verdad"));
        assert!(resultado.is_err());
    }
}
