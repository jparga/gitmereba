//! Cerrojo de fichero por cuenta, entre procesos.
//!
//! La ventana, la CLI y los temporizadores de systemd son procesos distintos: una marca en
//! memoria no basta para que mover la carpeta de una cuenta no coincida con una
//! sincronización suya. Cada cuenta tiene `<directorio_datos>/cerrojos/<login>.lock`
//! (directorio 0700, fichero 0600) y quien la toca toma un bloqueo exclusivo no bloqueante
//! (`File::try_lock`). Si ya está tomado no se espera: [`ErrorCuentas::CuentaOcupada`]. Lo
//! libera el `Drop` (al cerrar el fichero) y el sistema operativo si el proceso muere.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

use crate::config::Rutas;
use crate::modelo::Nombre;

use super::error::ErrorCuentas;

/// Guarda del cerrojo de una cuenta. Mientras exista, nadie más la toma.
#[derive(Debug)]
pub struct CerrojoCuenta {
    _fichero: File,
}

impl CerrojoCuenta {
    /// Toma el cerrojo de `login` sin esperar.
    pub fn tomar(rutas: &Rutas, login: &Nombre) -> Result<Self, ErrorCuentas> {
        let io = |error: std::io::Error| ErrorCuentas::Io(error.to_string());
        let directorio = rutas.directorio_datos().join("cerrojos");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directorio)
            .map_err(io)?;
        std::fs::set_permissions(&directorio, std::fs::Permissions::from_mode(0o700))
            .map_err(io)?;
        let fichero = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(directorio.join(format!("{login}.lock")))
            .map_err(io)?;
        match fichero.try_lock() {
            Ok(()) => Ok(Self { _fichero: fichero }),
            Err(TryLockError::WouldBlock) => Err(ErrorCuentas::CuentaOcupada(login.clone())),
            Err(TryLockError::Error(error)) => Err(io(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    #[test]
    fn la_segunda_guarda_del_mismo_login_falla_y_otro_login_no_choca() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let primera = CerrojoCuenta::tomar(&rutas, &nombre("ana")).expect("primera");
        assert!(matches!(
            CerrojoCuenta::tomar(&rutas, &nombre("ana")),
            Err(ErrorCuentas::CuentaOcupada(_))
        ));
        let _otra = CerrojoCuenta::tomar(&rutas, &nombre("luis")).expect("otro login");
        drop(primera);
        CerrojoCuenta::tomar(&rutas, &nombre("ana")).expect("se vuelve a poder tomar");
    }

    #[test]
    fn el_directorio_es_0700_y_el_fichero_0600() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let _guarda = CerrojoCuenta::tomar(&rutas, &nombre("ana")).expect("tomar");
        let directorio = rutas.directorio_datos().join("cerrojos");
        let modo = |ruta: &std::path::Path| {
            std::fs::metadata(ruta)
                .expect("metadatos")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(modo(&directorio), 0o700);
        assert_eq!(modo(&directorio.join("ana.lock")), 0o600);
    }
}
