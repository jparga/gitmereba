//! Error del módulo `config`.

use std::path::PathBuf;

/// Error de configuración: rutas, lectura/escritura de ficheros y validación de alta.
#[derive(Debug, thiserror::Error)]
pub enum ErrorConfig {
    #[error("no se encontró el directorio personal del usuario")]
    SinDirectorioHome,
    #[error("error de E/S: {0}")]
    Io(String),
    #[error("error al leer o escribir TOML: {0}")]
    Toml(String),
    #[error("la carpeta de destino debe ser una ruta absoluta")]
    CarpetaNoAbsoluta,
    #[error("la carpeta de destino no puede ser la raíz del sistema de ficheros")]
    CarpetaRaiz,
    #[error("la carpeta de destino no puede ser el directorio personal del usuario")]
    CarpetaHome,
    #[error("la carpeta de destino no puede estar dentro del directorio de datos de la app")]
    CarpetaDentroDeDatos,
    #[error("la carpeta de destino debe estar vacía o no existir")]
    CarpetaNoVacia,
    #[error("el login «{0}» ya está en uso por otra cuenta")]
    LoginDuplicado(String),
    #[error("la carpeta «{0}» ya está en uso por otra cuenta")]
    CarpetaDuplicada(PathBuf),
    #[error("el puerto debe estar entre 1024 y 65535")]
    PuertoFueraDeRango,
    #[error("el puerto {0} ya está en uso por otra cuenta")]
    PuertoDuplicado(u16),
    #[error("el intervalo mínimo entre sincronizaciones es de 10 minutos")]
    IntervaloDemasiadoCorto,
}
