//! Directorios de la aplicación, según XDG, y los de cada cuenta.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::config::error::ErrorConfig;

/// Directorios en los que vive la aplicación (no los de una cuenta en concreto).
///
/// `del_sistema` usa las rutas reales de XDG; `con_raiz` ancla todo a un directorio
/// arbitrario para que las pruebas no toquen el `HOME` real.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rutas {
    directorio_home: PathBuf,
    raiz_datos: PathBuf,
    raiz_systemd_usuario: PathBuf,
}

impl Rutas {
    /// Rutas reales del sistema: `~/.local/share/gitmereba/` y
    /// `~/.config/systemd/user/`.
    pub fn del_sistema() -> Result<Self, ErrorConfig> {
        let base = BaseDirs::new().ok_or(ErrorConfig::SinDirectorioHome)?;
        Ok(Self {
            directorio_home: base.home_dir().to_path_buf(),
            raiz_datos: base.data_dir().join("gitmereba"),
            raiz_systemd_usuario: base.config_dir().join("systemd").join("user"),
        })
    }

    /// Rutas ancladas a `raiz`, para pruebas: ningún test debe usar `del_sistema`.
    pub fn con_raiz(raiz: impl Into<PathBuf>) -> Self {
        let raiz = raiz.into();
        Self {
            directorio_home: raiz.clone(),
            raiz_datos: raiz.join("datos"),
            raiz_systemd_usuario: raiz.join("systemd-user"),
        }
    }

    /// Directorio personal del usuario (real o simulado con `con_raiz`).
    pub fn directorio_home(&self) -> &Path {
        &self.directorio_home
    }

    /// Directorio de datos de la app: `~/.local/share/gitmereba/`.
    pub fn directorio_datos(&self) -> &Path {
        &self.raiz_datos
    }

    /// Binarios de Gitea, compartidos entre cuentas.
    pub fn directorio_bin(&self) -> PathBuf {
        self.raiz_datos.join("bin")
    }

    /// SQLite propio de la app.
    pub fn fichero_bd(&self) -> PathBuf {
        self.raiz_datos.join("gitmereba.db")
    }

    /// Índice login → carpeta y puerto.
    pub fn fichero_cuentas(&self) -> PathBuf {
        self.raiz_datos.join("cuentas.toml")
    }

    /// Directorio de unidades `systemd --user`.
    pub fn directorio_systemd_usuario(&self) -> &Path {
        &self.raiz_systemd_usuario
    }
}

/// Rutas derivadas de la carpeta de una cuenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RutasCuenta {
    carpeta: PathBuf,
}

impl RutasCuenta {
    /// Rutas de la cuenta cuya carpeta es `carpeta`.
    pub fn nueva(carpeta: impl Into<PathBuf>) -> Self {
        Self {
            carpeta: carpeta.into(),
        }
    }

    /// Carpeta raíz de la cuenta.
    pub fn carpeta(&self) -> &Path {
        &self.carpeta
    }

    /// `gitmereba.toml`: configuración de la cuenta, sin secretos.
    pub fn fichero_config(&self) -> PathBuf {
        self.carpeta.join("gitmereba.toml")
    }

    /// Carpeta del Gitea de la cuenta.
    pub fn gitea(&self) -> PathBuf {
        self.carpeta.join("gitea")
    }

    /// `gitea/custom/conf/app.ini`.
    pub fn gitea_app_ini(&self) -> PathBuf {
        self.gitea().join("custom").join("conf").join("app.ini")
    }

    /// `gitea/data/`: SQLite de Gitea, LFS, sesiones.
    pub fn gitea_datos(&self) -> PathBuf {
        self.gitea().join("data")
    }

    /// `gitea/repositories/`.
    pub fn gitea_repositorios(&self) -> PathBuf {
        self.gitea().join("repositories")
    }

    /// Copias rotadas de los repos antes de cambios destructivos.
    pub fn snapshots(&self) -> PathBuf {
        self.carpeta.join("snapshots")
    }

    /// `gitea/tls/`: certificado y clave del acceso LAN por HTTPS. Directorio
    /// 0700; el certificado 0644 y la clave 0600 (nunca se lee ni se registra su
    /// contenido).
    pub fn gitea_tls(&self) -> PathBuf {
        self.gitea().join("tls")
    }

    /// Certificado (PEM) del acceso LAN.
    pub fn gitea_tls_cert(&self) -> PathBuf {
        self.gitea_tls().join("cert.pem")
    }

    /// Clave privada (PEM) del acceso LAN. Nunca se lee desde el core: solo se
    /// comprueba que existe y sus permisos.
    pub fn gitea_tls_key(&self) -> PathBuf {
        self.gitea_tls().join("key.pem")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn con_raiz_ancla_todo_bajo_la_raiz_dada() {
        let rutas = Rutas::con_raiz("/tmp/raiz-de-prueba");
        assert!(rutas.directorio_home().starts_with("/tmp/raiz-de-prueba"));
        assert!(rutas.directorio_datos().starts_with("/tmp/raiz-de-prueba"));
        assert!(
            rutas
                .directorio_systemd_usuario()
                .starts_with("/tmp/raiz-de-prueba")
        );
        assert_eq!(rutas.directorio_bin(), rutas.directorio_datos().join("bin"));
        assert_eq!(
            rutas.fichero_bd(),
            rutas.directorio_datos().join("gitmereba.db")
        );
        assert_eq!(
            rutas.fichero_cuentas(),
            rutas.directorio_datos().join("cuentas.toml")
        );
    }

    #[test]
    fn rutas_cuenta_derivan_de_la_carpeta_segun_la_spec() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        assert_eq!(
            rutas.fichero_config(),
            PathBuf::from("/cuentas/jparga/gitmereba.toml")
        );
        assert_eq!(rutas.gitea(), PathBuf::from("/cuentas/jparga/gitea"));
        assert_eq!(
            rutas.gitea_app_ini(),
            PathBuf::from("/cuentas/jparga/gitea/custom/conf/app.ini")
        );
        assert_eq!(
            rutas.gitea_datos(),
            PathBuf::from("/cuentas/jparga/gitea/data")
        );
        assert_eq!(
            rutas.gitea_repositorios(),
            PathBuf::from("/cuentas/jparga/gitea/repositories")
        );
        assert_eq!(
            rutas.snapshots(),
            PathBuf::from("/cuentas/jparga/snapshots")
        );
        assert_eq!(
            rutas.gitea_tls(),
            PathBuf::from("/cuentas/jparga/gitea/tls")
        );
        assert_eq!(
            rutas.gitea_tls_cert(),
            PathBuf::from("/cuentas/jparga/gitea/tls/cert.pem")
        );
        assert_eq!(
            rutas.gitea_tls_key(),
            PathBuf::from("/cuentas/jparga/gitea/tls/key.pem")
        );
    }
}
