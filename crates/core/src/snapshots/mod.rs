//! Copias rotadas de los repos antes de cambios destructivos
//! (por ejemplo, un origen comprometido que reescribe la historia).
//!
//! Un mirror fiel replica también lo destructivo: si la cuenta de GitHub de origen se
//! ve comprometida, o alguien hace un force-push o borra una rama, Gitea reescribe o
//! borra esas refs en el mirror local al sincronizar y los commits antiguos quedan
//! inalcanzables. Este módulo no toca nunca el árbol que gestiona Gitea (no puede
//! escribir ahí): la garantía de recuperación es un bare espejo propio por repo, bajo
//! `<carpeta de la cuenta>/snapshots/<dueño>/<nombre>.git`, al que se hace `git fetch`
//! desde el bare de Gitea guardando cada captura bajo un espacio de refs con fecha
//! (`refs/snapshots/<marca>/heads/*` y `.../tags/*`). Los objetos se comparten entre
//! capturas, así que el coste en disco es aproximadamente el de un repo más lo que se
//! haya reescrito con el tiempo.

mod captura;
mod compactacion;
mod comparacion;
mod deteccion;
mod error;
mod fichero;
mod listado;
mod manifiesto;
mod proteccion;
mod restauracion;
mod rotacion;
mod rutas;

#[cfg(test)]
mod tests_integracion;

pub use captura::{Captura, capturar};
pub use compactacion::compactar;
pub use deteccion::{
    CambioDestructivo, detectar_cambios_destructivos, detectar_desde_captura, detectar_en_bare,
};
pub use error::ErrorSnapshots;
pub use listado::{ResumenCaptura, listar, listar_cuenta};
pub use manifiesto::Manifiesto;
pub use proteccion::{liberar, proteger};
pub use restauracion::restaurar_en;
pub use rotacion::{PoliticaRetencion, rotar};
