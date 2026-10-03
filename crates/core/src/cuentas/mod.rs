//! Módulo `cuentas`: orquesta el alta, la sincronización y la baja de una cuenta.
//!
//! Es la capa que usarán tanto la CLI como (en F3) la interfaz Tauri: aquí vive toda la
//! lógica de negocio, siempre sobre los traits de los demás módulos (nunca contra sus
//! implementaciones reales) para poder probarla con dobles.

mod ajustes;
mod alta;
mod aprovisionador;
mod baja;
mod comun;
mod contexto;
mod doctor;
mod doctor_textos;
mod error;
mod estado;
mod lan;
mod lanzador;
mod listar;
mod progreso;
mod proteccion;
mod repos;
mod rotar_token;
mod sincronizar;
mod solicitud;
mod temporizador;
mod usuarios_lan;

#[cfg(test)]
mod dobles;

pub use ajustes::actualizar_ajustes;
pub use alta::alta;
pub use aprovisionador::{Aprovisionador, AprovisionadorReal};
pub use baja::baja;
pub use comun::cliente_gitea_de_cuenta;
pub use contexto::Contexto;
pub use doctor::{Comprobacion, InformeDoctor, NivelComprobacion, comprobar_idioma, doctor};
pub use doctor_textos::{MotivoAppIni, NombreComprobacion, ParteCuenta, TextoDoctor};
pub use error::ErrorCuentas;
pub use estado::{ConteoRepos, EstadoCuenta, estado};
pub use lan::{InformeLan, estado_lan, exponer_lan, ocultar_lan};
pub use lanzador::{Lanzador, LanzadorPrimerPlano, LanzadorSystemd, ParametrosLanzamiento};
pub use listar::listar;
pub use progreso::{EstadoPaso, PasoAlta};
pub use proteccion::{CambioDetectado, InformeProteccion, proteger_cuenta};
pub use repos::excluir_repo;
pub use rotar_token::rotar_token;
pub use sincronizar::{
    ResultadoSincronizacionYVerificacion, SincronizacionDeRepo, sincronizar, sincronizar_con,
    sincronizar_con_progreso, sincronizar_repo, sincronizar_repo_con, sincronizar_y_verificar,
    sincronizar_y_verificar_con, sincronizar_y_verificar_con_progreso,
};
pub use solicitud::{
    Descubrimiento, Previsualizacion, SolicitudAlta, descubrir_repos, previsualizar_alta,
};
pub use temporizador::{reparar_temporizador, temporizador_al_dia};
pub use usuarios_lan::{
    EQUIPO_ESCRITURA, EQUIPO_LECTURA, UsuarioLan, UsuarioLanCreado, crear_usuario_lan,
    eliminar_usuario_lan, listar_usuarios_lan, reconciliar_permisos_lan,
};
