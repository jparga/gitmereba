//! Directorio temporal propio, 0700 y fuera del árbol de Gitea: para el clon bare
//! temporal de [`super::reconciliar`] y para el directorio de trabajo neutral desde el
//! que [`super::activar`] empuja con `--git-dir` (nunca la propia carpeta del bare, ver
//! el comentario de `super::activar::empujar_mirror`).
//!
//! No se usa el crate `tempfile`: solo está declarado en `dev-dependencies` y este
//! código se ejecuta en producción, no en pruebas.

use std::path::{Path, PathBuf};

use super::error::ErrorContingencia;

pub(crate) struct DirTemporal(PathBuf);

impl DirTemporal {
    /// Crea `<temporal del sistema>/<prefijo>-<aleatorio>` con permisos 0700.
    pub(crate) fn nueva(prefijo: &str) -> Result<Self, ErrorContingencia> {
        use std::os::unix::fs::DirBuilderExt;

        let sufijo = getrandom::u64().map_err(|e| ErrorContingencia::Io(e.to_string()))?;
        let ruta = std::env::temp_dir().join(format!("{prefijo}-{sufijo:016x}"));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&ruta)
            .map_err(|e| ErrorContingencia::Io(e.to_string()))?;
        Ok(Self(ruta))
    }

    pub(crate) fn ruta(&self) -> &Path {
        &self.0
    }
}

impl Drop for DirTemporal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_crea_con_permisos_0700_y_se_borra_al_soltarse() {
        use std::os::unix::fs::PermissionsExt;

        let ruta;
        {
            let temporal =
                DirTemporal::nueva("gitmereba-test").expect("crear el directorio temporal");
            ruta = temporal.ruta().to_path_buf();
            assert!(ruta.exists());
            let permisos = std::fs::metadata(&ruta).expect("metadata").permissions();
            assert_eq!(permisos.mode() & 0o777, 0o700);
        }
        assert!(!ruta.exists(), "debe borrarse al soltar el guard");
    }

    #[test]
    fn dos_temporales_seguidos_no_colisionan() {
        let a = DirTemporal::nueva("gitmereba-test").expect("crear el primero");
        let b = DirTemporal::nueva("gitmereba-test").expect("crear el segundo");
        assert_ne!(a.ruta(), b.ruta());
    }
}
