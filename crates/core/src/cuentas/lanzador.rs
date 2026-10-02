//! Trait [`Lanzador`]: cómo se arranca y se para el Gitea de una cuenta.
//!
//! Dos implementaciones: [`LanzadorSystemd`], para producción, e
//! [`LanzadorPrimerPlano`], que usa [`ProcesoGitea`] y sirve para pruebas y para
//! `gitmereba cuenta add --primer-plano`/`doctor`.

use std::path::Path;
use std::sync::Mutex;

use crate::config::{self, Rutas, RutasCuenta};
use crate::instancia::{
    self, ParametrosTemporizador, ParametrosUnidad, ProcesoGitea, generar_servicio_sync,
    generar_timer_sync, generar_unidad, instalar_unidad,
};
use crate::modelo::Nombre;

use super::error::ErrorCuentas;

/// Datos para arrancar el Gitea de una cuenta, comunes a ambas implementaciones.
pub struct ParametrosLanzamiento<'a> {
    pub login: &'a Nombre,
    pub binario_gitea: &'a Path,
    pub app_ini: &'a Path,
    /// `GITEA_WORK_DIR`: la carpeta `gitea/` de la cuenta.
    pub directorio_trabajo: &'a Path,
    /// Carpeta raíz de la cuenta, para `ReadWritePaths` en la unidad systemd.
    pub carpeta_cuenta: &'a Path,
    pub puerto: u16,
    /// URL local para la sonda de arranque de [`LanzadorPrimerPlano`]:
    /// `cuenta.url_gitea()` (`http://127.0.0.1:<puerto>`, o `https://127.0.0.1:<puerto>`
    /// con acceso LAN activo). `LanzadorSystemd` no la usa: no hace ninguna sonda.
    pub url_local: String,
    /// Certificado (PEM) en el que confía la sonda cuando `url_local` es HTTPS. `None`
    /// si es HTTP.
    pub certificado_pem: Option<Vec<u8>>,
}

/// Arranca y para el Gitea de una cuenta.
///
/// Trait (usado con genéricos, nunca `dyn`) para poder sustituirlo por un doble en las
/// pruebas de `alta`, que no deben arrancar ningún proceso real.
#[allow(async_fn_in_trait)]
pub trait Lanzador {
    async fn arrancar(
        &self,
        rutas: &Rutas,
        parametros: &ParametrosLanzamiento<'_>,
    ) -> Result<(), ErrorCuentas>;

    async fn parar(&self, rutas: &Rutas, login: &Nombre) -> Result<(), ErrorCuentas>;
}

/// Instala la unidad `systemd --user` de la cuenta y la arranca, junto con el timer de
/// sincronización periódica. Para producción.
pub struct LanzadorSystemd;

impl Lanzador for LanzadorSystemd {
    async fn arrancar(
        &self,
        rutas: &Rutas,
        parametros: &ParametrosLanzamiento<'_>,
    ) -> Result<(), ErrorCuentas> {
        let parametros_unidad = ParametrosUnidad {
            login: parametros.login,
            binario_gitea: parametros.binario_gitea,
            app_ini: parametros.app_ini,
            directorio_trabajo: parametros.directorio_trabajo,
            carpeta_cuenta: parametros.carpeta_cuenta,
        };
        let texto = generar_unidad(&parametros_unidad);
        instalar_unidad(rutas, parametros.login, &texto)
            .await
            .map_err(ErrorCuentas::from)?;
        // `instancia::arrancar` hace `enable --now`: arranca ya y con cada inicio de sesión.
        instancia::arrancar(parametros.login)
            .await
            .map_err(ErrorCuentas::from)?;

        self.instalar_y_habilitar_temporizador(rutas, parametros)
            .await
    }

    async fn parar(&self, _rutas: &Rutas, login: &Nombre) -> Result<(), ErrorCuentas> {
        instancia::parar(login).await.map_err(ErrorCuentas::from)?;
        // Que no exista el timer (cuenta dada de alta antes de que existiera el timer) no es un error.
        instancia::deshabilitar_temporizador(login)
            .await
            .map_err(ErrorCuentas::from)
    }
}

impl LanzadorSystemd {
    /// Instala y habilita `gitmereba-sync-<login>.{service,timer}`.
    ///
    /// `ParametrosLanzamiento` no lleva `intervalo_minutos` (no se puede añadir sin tocar
    /// `alta.rs`, fuera del alcance de esta tarea: ver el informe): se relee de
    /// `gitmereba.toml`, que `cuentas::alta` ya ha escrito en `parametros.carpeta_cuenta`
    /// antes de llamar a `arrancar`.
    async fn instalar_y_habilitar_temporizador(
        &self,
        rutas: &Rutas,
        parametros: &ParametrosLanzamiento<'_>,
    ) -> Result<(), ErrorCuentas> {
        let rutas_cuenta = RutasCuenta::nueva(parametros.carpeta_cuenta);
        let cuenta = config::leer_cuenta(&rutas_cuenta)?;
        let ruta_ejecutable =
            std::env::current_exe().map_err(|error| ErrorCuentas::Io(error.to_string()))?;

        let parametros_temporizador = ParametrosTemporizador {
            login: parametros.login,
            ruta_ejecutable: &ruta_ejecutable,
            carpeta_cuenta: parametros.carpeta_cuenta,
            directorio_datos: rutas.directorio_datos(),
            intervalo_minutos: cuenta.intervalo_minutos,
        };
        let texto_service = generar_servicio_sync(&parametros_temporizador);
        let texto_timer = generar_timer_sync(&parametros_temporizador);
        instancia::instalar_temporizador(rutas, parametros.login, &texto_service, &texto_timer)
            .await
            .map_err(ErrorCuentas::from)?;
        instancia::habilitar_temporizador(parametros.login)
            .await
            .map_err(ErrorCuentas::from)
    }
}

/// Arranca `gitea web` como proceso hijo en primer plano (vía [`ProcesoGitea`]), sin
/// pasar por systemd. Para pruebas de integración y para `doctor`/`--primer-plano`.
#[derive(Default)]
pub struct LanzadorPrimerPlano {
    proceso: Mutex<Option<ProcesoGitea>>,
}

impl LanzadorPrimerPlano {
    pub fn nuevo() -> Self {
        Self::default()
    }
}

impl Lanzador for LanzadorPrimerPlano {
    async fn arrancar(
        &self,
        _rutas: &Rutas,
        parametros: &ParametrosLanzamiento<'_>,
    ) -> Result<(), ErrorCuentas> {
        let proceso = ProcesoGitea::arrancar_en_primer_plano(
            parametros.binario_gitea,
            parametros.app_ini,
            parametros.directorio_trabajo,
            &parametros.url_local,
            parametros.certificado_pem.as_deref(),
        )
        .await
        .map_err(ErrorCuentas::from)?;

        let mut guardia = self
            .proceso
            .lock()
            .map_err(|_| ErrorCuentas::Interno("mutex de LanzadorPrimerPlano envenenado".into()))?;
        *guardia = Some(proceso);
        Ok(())
    }

    async fn parar(&self, _rutas: &Rutas, _login: &Nombre) -> Result<(), ErrorCuentas> {
        let proceso = {
            let mut guardia = self.proceso.lock().map_err(|_| {
                ErrorCuentas::Interno("mutex de LanzadorPrimerPlano envenenado".into())
            })?;
            guardia.take()
        };
        if let Some(proceso) = proceso {
            proceso.parar().await.map_err(ErrorCuentas::from)?;
        }
        Ok(())
    }
}
