//! Error del módulo `almacen`.

/// Fallos al abrir o usar el almacén SQLite de la app.
///
/// Ningún mensaje incluye secretos: el módulo nunca recibe un [`crate::secretos`], y el
/// texto libre que sí acepta (`detalle`, `resumen`) se sanea antes de guardarse (ver
/// [`super::saneado`]).
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ErrorAlmacen {
    /// Cualquier fallo devuelto por SQLite (consulta, apertura, migración).
    #[error("error de sqlite: {0}")]
    Sqlite(String),
    /// La base de datos tiene una versión de esquema más reciente que la que conoce
    /// esta versión de la app: no se toca para no arriesgar los datos.
    #[error(
        "la base de datos tiene la versión {encontrada}, más reciente que la conocida por esta versión de la app ({conocida})"
    )]
    VersionFutura { encontrada: i64, conocida: i64 },
    /// `accion` no cumple `[a-z0-9._-]{1,64}`.
    #[error(
        "la acción de auditoría no es válida: solo minúsculas, dígitos, «.», «_» o «-», de 1 a 64 caracteres"
    )]
    AccionInvalida,
    /// `detalle_json` supera el tamaño máximo permitido tras sanear.
    #[error("el detalle supera el tamaño máximo permitido")]
    DetalleDemasiadoGrande,
    /// Error de E/S al crear o inspeccionar el fichero de la base de datos.
    #[error("error de E/S: {0}")]
    Io(String),
}

impl From<rusqlite::Error> for ErrorAlmacen {
    fn from(error: rusqlite::Error) -> Self {
        ErrorAlmacen::Sqlite(error.to_string())
    }
}

impl From<std::io::Error> for ErrorAlmacen {
    fn from(error: std::io::Error) -> Self {
        ErrorAlmacen::Io(error.to_string())
    }
}
