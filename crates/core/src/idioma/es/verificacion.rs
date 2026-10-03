//! Textos de los avisos de la verificación, en español.

use crate::verificacion::Aviso;

/// El español es el `Display` del propio aviso.
pub(crate) fn aviso(aviso: &Aviso) -> String {
    aviso.to_string()
}
