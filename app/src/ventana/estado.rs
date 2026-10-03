//! Estado compartido por los comandos y ejecución de `core` fuera del hilo de la ventana.

use std::future::Future;

use gitmereba_core::almacen::Almacen;
use gitmereba_core::config::{self, Rutas, RutasCuenta};
use gitmereba_core::cuentas::Contexto;
use gitmereba_core::idioma::{Idioma, idioma_actual};
use gitmereba_core::modelo::Cuenta;
use gitmereba_core::secretos::LlaveroDelSistema;

use super::error::{ErrorUi, texto};
use crate::comandos::abrir_almacen;

/// Estado gestionado por Tauri (`tauri::State<EstadoApp>`). Solo las rutas: el almacén y
/// el llavero se abren por comando, igual que hace la CLI, para no compartir conexiones
/// entre hilos.
#[derive(Debug, Clone)]
pub struct EstadoApp {
    pub rutas: Rutas,
}

impl EstadoApp {
    /// Idioma de la interfaz en este momento. No se cachea: el usuario puede cambiarlo
    /// mientras la ventana sigue abierta.
    pub fn idioma(&self) -> Idioma {
        idioma_actual(&self.rutas)
    }
}

/// Lo que necesita un comando para llamar a `core::cuentas`. Se crea dentro del hilo del
/// comando con [`Recursos::abrir`] y presta un [`Contexto`].
pub struct Recursos {
    pub rutas: Rutas,
    pub idioma: Idioma,
    pub llavero: LlaveroDelSistema,
    pub almacen: Almacen,
}

impl Recursos {
    pub fn abrir(rutas: &Rutas) -> Result<Self, ErrorUi> {
        let idioma = idioma_actual(rutas);
        let llavero = LlaveroDelSistema::nuevo().map_err(|error| ErrorUi::de(&error, idioma))?;
        let almacen = abrir_almacen(rutas).map_err(|error| ErrorUi::de(&error, idioma))?;
        Ok(Self {
            rutas: rutas.clone(),
            idioma,
            llavero,
            almacen,
        })
    }

    pub fn contexto(&self) -> Contexto<'_, LlaveroDelSistema> {
        Contexto::nuevo(&self.rutas, &self.llavero, &self.almacen)
    }

    /// Cuenta dada de alta con ese login, o `cuenta_no_encontrada`.
    pub fn cuenta(&self, login: &str) -> Result<(Cuenta, RutasCuenta), ErrorUi> {
        let idioma = self.idioma;
        let indice = config::leer_indice_cuentas(&self.rutas)
            .map_err(|error| ErrorUi::de(&error, idioma))?;
        let entrada = indice
            .cuentas
            .get(login)
            .ok_or_else(|| ErrorUi::cuenta_no_encontrada(login, idioma))?;
        let rutas_cuenta = RutasCuenta::nueva(entrada.carpeta.clone());
        let cuenta =
            config::leer_cuenta(&rutas_cuenta).map_err(|error| ErrorUi::de(&error, idioma))?;
        Ok((cuenta, rutas_cuenta))
    }
}

/// Ejecuta `tarea` en un hilo propio del sistema con su ejecutor de un solo hilo.
///
/// Los futuros de `core` no garantizan `Send` (traits con `async fn` nativas) y los
/// comandos asíncronos de Tauri lo exigen; además así ninguna operación larga (una
/// sincronización, un alta) ocupa el hilo de la ventana. No se usa `spawn_blocking`: el
/// llavero arranca su propio ejecutor y tokio no admite uno dentro de otro.
pub async fn en_hilo<T, F, Fut>(idioma: Idioma, tarea: F) -> Result<T, ErrorUi>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, ErrorUi>>,
{
    let (emisor, receptor) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("gitmereba-comando".to_string())
        .spawn(move || {
            let resultado = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| ErrorUi::externo("interno", None, &error))
                .and_then(|ejecutor| ejecutor.block_on(tarea()));
            let _ = emisor.send(resultado);
        })
        .map_err(|error| ErrorUi::externo("interno", None, &error))?;
    receptor.await.map_err(|_| {
        ErrorUi::interno(texto(
            idioma,
            "el comando terminó sin dar resultado",
            "the command finished without a result",
        ))
    })?
}
