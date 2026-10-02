//! Contexto compartido por todas las operaciones de `cuentas`.

use crate::almacen::Almacen;
use crate::config::Rutas;
use crate::secretos::Llavero;

/// Todo lo que necesita una operación de `cuentas`: dónde vive la app (`rutas`), dónde
/// están los secretos (`llavero`) y el histórico/auditoría (`almacen`).
///
/// Genérico sobre [`Llavero`] para poder probar con [`crate::secretos::LlaveroEnMemoria`]
/// sin tocar el llavero real del sistema.
pub struct Contexto<'a, L: Llavero> {
    pub rutas: &'a Rutas,
    pub llavero: &'a L,
    pub almacen: &'a Almacen,
}

impl<'a, L: Llavero> Contexto<'a, L> {
    pub fn nuevo(rutas: &'a Rutas, llavero: &'a L, almacen: &'a Almacen) -> Self {
        Self {
            rutas,
            llavero,
            almacen,
        }
    }
}
