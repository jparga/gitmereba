//! Invocación segura de `git`: sin shell, entorno mínimo y argumentos validados.
//!
//! Este módulo no depende de ningún otro del `core`.

mod comandos;
mod proceso;
mod validar;

pub use comandos::{commits_de_mas, es_ancestro, fsck, refs, version};
pub use proceso::{CertificadoCa, Credencial, ErrorGit, Opciones, Salida, ejecutar};
pub use validar::validar_ref;
