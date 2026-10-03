//! Errores del almacén SQLite, en inglés.

use crate::almacen::ErrorAlmacen;

pub(crate) fn texto(e: &ErrorAlmacen) -> String {
    match e {
        ErrorAlmacen::Sqlite(detalle) => format!("sqlite error: {detalle}"),
        ErrorAlmacen::VersionFutura {
            encontrada,
            conocida,
        } => format!(
            "the database is at version {encontrada}, newer than the one known to this \
             version of the app ({conocida})"
        ),
        ErrorAlmacen::AccionInvalida => "the audit action is not valid: only lowercase letters, \
                                         digits, “.”, “_” or “-”, from 1 to 64 characters"
            .to_string(),
        ErrorAlmacen::DetalleDemasiadoGrande => {
            "the detail exceeds the maximum allowed size".to_string()
        }
        ErrorAlmacen::Io(detalle) => format!("I/O error: {detalle}"),
    }
}
