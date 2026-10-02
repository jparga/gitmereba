//! Instancia de Gitea de una cuenta: descarga verificada del binario, `app.ini`
//! endurecido, provisión desatendida, unidad `systemd --user` y, opcionalmente por
//! cuenta, el certificado autofirmado de su acceso LAN por HTTPS.

mod app_ini;
mod binario;
mod certificado;
mod error;
mod fichero;
mod proceso;
mod provision;
mod puerto;
mod servicio;
mod temporizador;
mod usuarios;

pub use app_ini::{
    ParametrosAppIni, ParametrosLanAppIni, SecretosAppIni, escribir_app_ini, generar_app_ini,
    leer_secretos_app_ini, regenerar_app_ini_conservando_secretos,
};
pub use binario::{SHA256_GITEA_1_27_3_LINUX_AMD64, VERSION_GITEA, asegurar_binario};
pub use certificado::{
    ParametrosCertificado, ResultadoCertificado, asegurar_certificado, huella_sha256,
};
pub use error::ErrorInstancia;
pub use provision::{NOMBRE_ADMIN_GITEA, ParametrosProvision, ResultadoProvision, provisionar};
pub use puerto::puerto_libre;
pub use servicio::{
    ParametrosUnidad, ProcesoGitea, arrancar, estado, generar_unidad, instalar_unidad,
    nombre_unidad, parar,
};
pub use temporizador::{
    ParametrosTemporizador, borrar_unidades, deshabilitar_temporizador, generar_servicio_sync,
    generar_timer_sync, habilitar_temporizador, instalar_temporizador, nombre_servicio_sync,
    nombre_timer_sync, reiniciar_temporizador,
};
pub use usuarios::{UsuarioGitea, borrar_usuario, crear_usuario, listar_usuarios};
