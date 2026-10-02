//! Módulo `contingencia`: activa un repo hermano con escritura junto a un mirror
//! (Gitea no puede convertir un mirror en repo normal), reconcilia sus cambios con GitHub sin forzar nunca
//! nada, y vuelve a dejar el mirror original en marcha.

mod activar;
mod cerrar;
mod decidir;
mod error;
mod estado;
mod lfs;
mod modelo;
mod nombres;
mod reconciliar;
mod rutas;
mod temporal;

#[cfg(test)]
mod dobles;

pub use activar::{CredencialGitea, activar, activar_cuenta};
pub use cerrar::{borrar_contingencia, cerrar};
pub use decidir::{Decision, EntradaDecision, argumentos_push, decidir, refspec_rama, refspec_tag};
pub use error::{ErrorContingencia, PasoActivar};
pub use estado::{estado, estado_de_mirror};
pub use modelo::{
    EstadoContingencia, EstadoRama, PuntoDePartida, RepoContingencia, ResultadoCierre,
};
pub use nombres::{es_org_contingencia, org_contingencia};
pub use reconciliar::{
    InformeReconciliacion, OrigenReconciliacion, ResultadoRama, ResultadoTag, reconciliar,
};
