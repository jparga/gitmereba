//! Error del módulo `snapshots`. Ningún mensaje incluye secretos ni rutas sensibles.

/// Error de una operación de snapshots: capturar, listar, rotar, restaurar...
#[derive(Debug, thiserror::Error)]
pub enum ErrorSnapshots {
    #[error("git: {0}")]
    Git(#[from] crate::git::ErrorGit),
    #[error("error de E/S: {0}")]
    Io(String),
    #[error("error al leer o escribir el manifiesto: {0}")]
    Json(String),
    #[error("el repositorio de origen no existe en «gitea/repositories/»")]
    OrigenNoExiste,
    #[error("el repositorio de origen queda fuera de «gitea/repositories/» tras resolver enlaces")]
    OrigenFueraDeRepositorios,
    #[error("no hay capturas para este repositorio")]
    SinCapturas,
    #[error("el snapshot queda fuera de «snapshots/» tras resolver enlaces")]
    DestinoFueraDeSnapshots,
    #[error("el destino de la restauración ya existe")]
    DestinoExiste,
    #[error("el destino de la restauración no es una ruta válida")]
    DestinoInvalido,
    #[error("el destino no puede estar dentro de «gitea/» ni de «snapshots/»")]
    DestinoDentroDeGiteaOSnapshots,
    #[error("la marca de tiempo «{0}» no tiene el formato AAAAMMDDTHHMMSSZ")]
    MarcaInvalida(String),
    #[error("no se encontró la captura «{0}»")]
    CapturaNoEncontrada(String),
    #[error("el manifiesto ocupa {0} bytes, más del máximo permitido")]
    ManifiestoDemasiadoGrande(usize),
    #[error("el manifiesto contiene un SHA con un formato no válido")]
    ShaInvalido,
    #[error("una ruta contiene bytes que no son UTF-8 válido")]
    RutaNoUtf8,
    #[error("las referencias tras restaurar no coinciden con el manifiesto")]
    RestauracionInconsistente,
}
