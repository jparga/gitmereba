//! Trait [`Aprovisionador`]: abstrae `instancia::asegurar_binario`/`instancia::provisionar`
//! para que los unitarios de `alta` no descarguen ni ejecuten ningún binario real.

use std::path::PathBuf;

use crate::config::Rutas;
use crate::instancia::{self, ParametrosProvision, ResultadoProvision};

use super::error::ErrorCuentas;

/// Asegura el binario de Gitea y provisiona una instancia.
///
/// Se define como trait (usado con genéricos, nunca `dyn`, igual que [`crate::github::ApiGithub`]
/// y [`crate::gitea::ApiGitea`]) para poder sustituirlo por un doble en las pruebas.
#[allow(async_fn_in_trait)]
pub trait Aprovisionador {
    /// Ver [`instancia::asegurar_binario`].
    async fn asegurar_binario(&self, rutas: &Rutas) -> Result<PathBuf, ErrorCuentas>;

    /// Ver [`instancia::provisionar`].
    async fn provisionar(
        &self,
        parametros: &ParametrosProvision<'_>,
    ) -> Result<ResultadoProvision, ErrorCuentas>;
}

/// Implementación real: descarga (si hace falta) y provisiona de verdad.
pub struct AprovisionadorReal;

impl Aprovisionador for AprovisionadorReal {
    async fn asegurar_binario(&self, rutas: &Rutas) -> Result<PathBuf, ErrorCuentas> {
        instancia::asegurar_binario(rutas)
            .await
            .map_err(ErrorCuentas::from)
    }

    async fn provisionar(
        &self,
        parametros: &ParametrosProvision<'_>,
    ) -> Result<ResultadoProvision, ErrorCuentas> {
        instancia::provisionar(parametros)
            .await
            .map_err(ErrorCuentas::from)
    }
}
