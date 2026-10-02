//! Módulo `sync`: reconcilia lo descubierto en GitHub con lo que hay en el Gitea local.
//!
//! No depende de `almacen` (todavía no existe): [`ejecutar`] devuelve un [`InformeSync`]
//! y es quien lo llama el que decide cómo persistirlo. Por el mismo motivo, el estado de
//! la pasada anterior que hace falta para planificar ([`EstadoPrevio`]) se lo tiene que
//! dar el llamador.

mod ejecucion;
mod error;
mod informe;
mod plan;
mod progreso;

#[cfg(test)]
mod dobles;

pub use ejecucion::{CONCURRENCIA_POR_DEFECTO, OpcionesSync, ejecutar, ejecutar_con_progreso};
pub use error::ErrorSync;
pub use informe::{ErrorListado, InformeSync, ResultadoAccion, ResultadoRepo, TipoAccion};
pub use plan::{Accion, AlertaPlan, EstadoPrevio, MotivoOmision, Omitido, Plan, planificar};
pub use progreso::{FaseSync, ProgresoSync};
