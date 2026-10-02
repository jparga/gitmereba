//! Helpers compartidos entre las operaciones de `cuentas`.

use std::path::{Path, PathBuf};

use crate::config::{self, Rutas, RutasCuenta};
use crate::gitea::ClienteGitea;
use crate::instancia;
use crate::modelo::{Cuenta, Nombre};
use crate::secretos::Secreto;

use super::error::ErrorCuentas;

/// Carga la cuenta `login` a partir del índice y de su `gitmereba.toml`.
pub(super) fn cargar_cuenta(rutas: &Rutas, login: &Nombre) -> Result<Cuenta, ErrorCuentas> {
    let indice = config::leer_indice_cuentas(rutas)?;
    let entrada = indice
        .cuentas
        .get(login.as_str())
        .ok_or_else(|| ErrorCuentas::CuentaNoExiste(login.clone()))?;
    let rutas_cuenta = RutasCuenta::nueva(&entrada.carpeta);
    config::leer_cuenta(&rutas_cuenta).map_err(ErrorCuentas::from)
}

/// Construye el [`ClienteGitea`] de `cuenta`: sin acceso LAN, contra
/// `http://127.0.0.1:<puerto>` como siempre; con acceso LAN activo, contra
/// `https://127.0.0.1:<puerto>` confiando solo en el certificado autofirmado de esa
/// cuenta (nunca en las CA del sistema).
pub fn cliente_gitea_de_cuenta(
    cuenta: &Cuenta,
    token: Secreto,
) -> Result<ClienteGitea, ErrorCuentas> {
    match &cuenta.lan {
        None => ClienteGitea::nuevo(&cuenta.url_gitea(), token).map_err(ErrorCuentas::from),
        Some(_) => {
            let ruta_certificado = RutasCuenta::nueva(&cuenta.carpeta).gitea_tls_cert();
            let certificado_pem = std::fs::read(&ruta_certificado)
                .map_err(|error| ErrorCuentas::Io(error.to_string()))?;
            ClienteGitea::nuevo_con_certificado(&cuenta.url_gitea(), token, &certificado_pem)
                .map_err(ErrorCuentas::from)
        }
    }
}

/// Crea `ruta` (y sus padres) con permisos 0700; si ya existe, le fija 0700.
///
/// Copia deliberada del patrón de `config::fichero`/`instancia::fichero`: ambos son
/// `pub(super)` de su propio módulo, así que `cuentas` no puede reutilizarlos.
pub(super) fn crear_directorio_0700(ruta: &Path) -> Result<(), ErrorCuentas> {
    use std::os::unix::fs::DirBuilderExt;

    use std::os::unix::fs::PermissionsExt;

    if ruta.exists() {
        // La carpeta puede venir ya creada (p. ej. desde el diálogo de selección) con
        // los permisos del umask: se cierra igualmente a 0700.
        return std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| ErrorCuentas::Io(error.to_string()));
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(ruta)
        .map_err(|error| ErrorCuentas::Io(error.to_string()))
}

/// Nombre de usuario del sistema que ejecuta el proceso, para `RUN_USER` en `app.ini`.
pub(super) fn usuario_actual() -> String {
    std::env::var("USER").unwrap_or_else(|_| "gitmereba".to_string())
}

/// Ruta al binario de Gitea cacheado por versión (`instancia::asegurar_binario`), común a
/// todas las cuentas. Usado por `lan` y `usuarios_lan` para invocar la
/// CLI de `gitea` sin volver a descargarlo.
pub(super) fn ruta_binario_cacheado(rutas: &Rutas) -> PathBuf {
    rutas
        .directorio_bin()
        .join(format!("gitea-{}", instancia::VERSION_GITEA))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crear_directorio_0700_cierra_los_permisos_de_una_carpeta_que_ya_existia() {
        use std::os::unix::fs::PermissionsExt;
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let ruta = temporal.path().join("elegida-en-el-dialogo");
        std::fs::create_dir(&ruta).expect("crear");
        std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o775)).expect("chmod");

        crear_directorio_0700(&ruta).expect("asegurar");

        let modo = std::fs::metadata(&ruta)
            .expect("metadatos")
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o700);
    }

    #[test]
    fn crear_directorio_0700_es_idempotente() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let carpeta = temporal.path().join("cuenta");

        crear_directorio_0700(&carpeta).expect("primera creación");
        crear_directorio_0700(&carpeta).expect("segunda llamada no falla");

        use std::os::unix::fs::PermissionsExt;
        let permisos = std::fs::metadata(&carpeta).expect("metadata").permissions();
        assert_eq!(permisos.mode() & 0o777, 0o700);
    }

    #[test]
    fn cargar_cuenta_falla_si_no_esta_en_el_indice() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let login = Nombre::nuevo("no-existe").expect("nombre");

        let resultado = cargar_cuenta(&rutas, &login);
        assert!(matches!(resultado, Err(ErrorCuentas::CuentaNoExiste(_))));
    }
}
