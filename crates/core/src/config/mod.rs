//! Rutas de la app (XDG) y configuración por cuenta.

mod cuenta;
mod error;
mod fichero;
mod rutas;
mod validacion;

pub use cuenta::{
    EntradaCuenta, IndiceCuentas, escribir_cuenta, escribir_indice_cuentas, leer_cuenta,
    leer_indice_cuentas,
};
pub use error::ErrorConfig;
pub use rutas::{Rutas, RutasCuenta};
pub use validacion::{validar_alta, validar_intervalo};
