//! Módulo `verificacion`: salud de los mirrors (SHA frente a GitHub, `git fsck`,
//! obsolescencia) y avisos operativos (caducidad del token, límite de peticiones a
//! GitHub).
//!
//! [`evaluar`] es la evaluación pura (sin E/S) de un repo ya recogido, y la parte más
//! probada del módulo. [`verificar_cuenta`] es quien recoge los datos (SHA de GitHub,
//! `refs` y `fsck` locales) y llama a [`evaluar`] por cada mirror de la cuenta.

mod aviso;
mod caducidad;
mod espacio;
mod evaluar;
mod informe;
mod motivo;
mod recogida;
mod umbrales;

#[cfg(test)]
mod dobles;

pub use aviso::Aviso;
pub use caducidad::{DIAS_AVISO_POR_DEFECTO, aviso_caducidad};
pub use espacio::{ErrorEspacio, espacio_de};
pub use evaluar::{Diagnostico, EntradaRepo, ResultadoFsck, evaluar};
pub use informe::InformeVerificacion;
pub use motivo::Motivo;
pub use recogida::{ContextoVerificacion, OpcionesVerificacion, verificar_cuenta};
pub use umbrales::{GRACIA_INICIAL_POR_DEFECTO, OBSOLETO_MINIMO, Umbrales};
