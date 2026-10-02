//! Error del módulo `instancia`.

use thiserror::Error;

use crate::modelo::Nombre;
use crate::secretos::ErrorLlavero;

/// Fallos al asegurar el binario de Gitea, generar `app.ini`, provisionar una
/// instancia o gestionar su unidad `systemd --user`.
///
/// Ningún mensaje incluye secretos (contraseñas, tokens) ni el contenido de la clave
/// GPG. La salida de los procesos `gitea` se recorta y, cuando puede llevar un
/// secreto (p. ej. la contraseña pasada a `admin user create`), se sanea antes de
/// incluirla en el error.
#[derive(Debug, Error)]
pub enum ErrorInstancia {
    #[error("error de E/S: {0}")]
    Io(String),
    #[error("error de red: {0}")]
    Red(String),
    #[error("host de descarga no permitido: {0}")]
    HostNoPermitido(String),
    #[error("la descarga supera el tamaño máximo permitido ({0} MB)")]
    TamanoExcedido(u64),
    #[error("el SHA-256 del binario descargado no coincide con el esperado")]
    Sha256NoCoincide,
    #[error("gpgv no está instalado en el sistema")]
    GpgvNoDisponible,
    #[error("la firma GPG del binario de Gitea no es válida")]
    FirmaInvalida,
    #[error("gitea no está instalado o no se encuentra en la ruta indicada")]
    GiteaNoEncontrado,
    #[error("gitea terminó con código {codigo}: {stderr}")]
    GiteaFallo { codigo: i32, stderr: String },
    #[error("systemctl terminó con código {codigo}: {stderr}")]
    SystemctlFallo { codigo: i32, stderr: String },
    #[error("se superó el tiempo límite esperando a un proceso")]
    Timeout,
    #[error("gitea no respondió a tiempo tras arrancar")]
    ArranqueTimeout,
    #[error("no hay ningún puerto libre en el rango asignado")]
    SinPuertoLibre,
    #[error("entrada inválida: {0}")]
    EntradaInvalida(String),
    /// `gitea admin user create` para un usuario de la LAN que ya existe:
    /// Gitea imprime igualmente una contraseña generada en `stdout`, pero termina con
    /// código de error; se detecta por `stderr` («user already exists»), nunca por el
    /// contenido de `stdout`.
    #[error("el usuario «{0}» ya existe en Gitea")]
    UsuarioGiteaYaExiste(Nombre),
    #[error("error al acceder al llavero: {0}")]
    Llavero(#[from] ErrorLlavero),
}
