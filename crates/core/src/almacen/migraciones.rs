//! Migraciones versionadas del esquema, controladas por `PRAGMA user_version`.
//!
//! Cada script se aplica en su propia transacción y sube la versión en uno. Abrir una
//! base de datos con una versión mayor que la conocida por esta lista es un error
//! ([`ErrorAlmacen::VersionFutura`]): no se toca nada.

use rusqlite::Connection;

use super::error::ErrorAlmacen;

/// Primera versión del esquema: tablas de sincronizaciones, estado de repos y
/// auditoría de solo-añadir, con los triggers que la hacen inmutable.
const MIGRACION_1: &str = r#"
CREATE TABLE sincronizaciones (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    cuenta       TEXT NOT NULL,
    inicio       TEXT NOT NULL,
    fin          TEXT NOT NULL,
    resultado    TEXT NOT NULL CHECK (resultado IN ('ok', 'con-fallos', 'error')),
    creados      INTEGER NOT NULL,
    huerfanos    INTEGER NOT NULL,
    fallos       INTEGER NOT NULL,
    resumen      TEXT NOT NULL,
    detalle_json TEXT
);

CREATE INDEX idx_sincronizaciones_cuenta_fin ON sincronizaciones (cuenta, fin);

CREATE TABLE estado_repos (
    cuenta      TEXT NOT NULL,
    dueno       TEXT NOT NULL,
    nombre      TEXT NOT NULL,
    estado      TEXT NOT NULL,
    detalle     TEXT,
    actualizado TEXT NOT NULL,
    PRIMARY KEY (cuenta, dueno, nombre)
);

CREATE TABLE auditoria (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    momento       TEXT NOT NULL,
    cuenta        TEXT,
    accion        TEXT NOT NULL,
    detalle       TEXT NOT NULL,
    hash_anterior TEXT NOT NULL,
    hash          TEXT NOT NULL
);

CREATE TRIGGER auditoria_sin_update
BEFORE UPDATE ON auditoria
BEGIN
    SELECT RAISE(ABORT, 'la auditoría es de solo lectura: no se puede modificar');
END;

CREATE TRIGGER auditoria_sin_delete
BEFORE DELETE ON auditoria
BEGIN
    SELECT RAISE(ABORT, 'la auditoría es de solo lectura: no se puede borrar');
END;
"#;

/// Segunda versión: punto de partida de cada contingencia activada (el propio módulo
/// `contingencia` no tiene almacén: quien la activa debe guardarlo para que
/// `contingencia::reconciliar` y la vista de Contingencia puedan volver a leerlo).
const MIGRACION_2: &str = r#"
CREATE TABLE contingencia_puntos (
    cuenta     TEXT NOT NULL,
    dueno      TEXT NOT NULL,
    nombre     TEXT NOT NULL,
    punto_json TEXT NOT NULL,
    PRIMARY KEY (cuenta, dueno, nombre)
);
"#;

/// Tercera versión: lo que GitHub dice de cada repo y la copia local no conserva
/// (visibilidad real, fork, archivado). El mirror de Gitea es siempre privado.
const MIGRACION_3: &str = r#"
CREATE TABLE repos_origen (
    cuenta    TEXT NOT NULL,
    dueno     TEXT NOT NULL,
    nombre    TEXT NOT NULL,
    privado   INTEGER NOT NULL,
    es_fork   INTEGER NOT NULL,
    archivado INTEGER NOT NULL,
    PRIMARY KEY (cuenta, dueno, nombre)
);
"#;

/// Lista ordenada de migraciones. La versión de cada una es su posición (1-based).
const MIGRACIONES: &[&str] = &[MIGRACION_1, MIGRACION_2, MIGRACION_3];

/// Aplica las migraciones que falten y deja `user_version` al día. Rechaza una base de
/// datos con una versión más reciente que la última migración conocida.
pub(super) fn aplicar(conexion: &mut Connection) -> Result<(), ErrorAlmacen> {
    let version_actual: i64 = conexion.query_row("PRAGMA user_version", [], |fila| fila.get(0))?;
    let version_conocida = MIGRACIONES.len() as i64;

    if version_actual > version_conocida {
        return Err(ErrorAlmacen::VersionFutura {
            encontrada: version_actual,
            conocida: version_conocida,
        });
    }

    for (indice, script) in MIGRACIONES.iter().enumerate() {
        let version = (indice + 1) as i64;
        if version <= version_actual {
            continue;
        }
        let transaccion = conexion.transaction()?;
        transaccion.execute_batch(script)?;
        transaccion.pragma_update(None, "user_version", version)?;
        transaccion.commit()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(conexion: &Connection) -> i64 {
        conexion
            .query_row("PRAGMA user_version", [], |fila| fila.get(0))
            .expect("leer user_version")
    }

    #[test]
    fn migra_desde_cero_hasta_la_ultima_version() {
        let mut conexion = Connection::open_in_memory().expect("abrir en memoria");
        aplicar(&mut conexion).expect("aplicar migraciones");
        assert_eq!(version(&conexion), MIGRACIONES.len() as i64);

        conexion
            .execute("INSERT INTO auditoria (momento, cuenta, accion, detalle, hash_anterior, hash) VALUES ('t', NULL, 'x', 'd', 'a', 'b')", [])
            .expect("la tabla auditoria existe");
    }

    #[test]
    fn reaplicar_es_idempotente() {
        let mut conexion = Connection::open_in_memory().expect("abrir en memoria");
        aplicar(&mut conexion).expect("primera aplicación");
        aplicar(&mut conexion).expect("segunda aplicación no falla");
        assert_eq!(version(&conexion), MIGRACIONES.len() as i64);
    }

    #[test]
    fn version_futura_se_rechaza() {
        let mut conexion = Connection::open_in_memory().expect("abrir en memoria");
        conexion
            .pragma_update(None, "user_version", MIGRACIONES.len() as i64 + 1)
            .expect("fijar versión futura");

        let error = aplicar(&mut conexion).expect_err("debe rechazarse");
        assert_eq!(
            error,
            ErrorAlmacen::VersionFutura {
                encontrada: MIGRACIONES.len() as i64 + 1,
                conocida: MIGRACIONES.len() as i64,
            }
        );
    }
}
