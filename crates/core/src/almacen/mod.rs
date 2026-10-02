//! SQLite local de la app: histórico de sincronizaciones, estado de repos y auditoría
//! de solo-añadir.
//!
//! Vive en `~/.local/share/gitmereba/gitmereba.db` (ver [`crate::config::rutas`]). Este
//! módulo nunca recibe un [`crate::secretos::Secreto`]: el llamador no debe meter
//! secretos en `detalle` ni `resumen`. Como defensa adicional, todo texto libre se sanea
//! antes de guardarse (ver [`saneado`]).
//!
//! La auditoría es de solo-añadir: la API pública no tiene ninguna función que la
//! modifique o borre, y la base de datos lo hace cumplir con triggers `BEFORE
//! UPDATE`/`BEFORE DELETE` que abortan la transacción.

mod auditoria;
mod contingencia;
mod error;
mod estados;
mod migraciones;
mod origenes;
mod saneado;
mod sincronizaciones;

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub use auditoria::{EntradaAuditoria, VerificacionAuditoria};
pub use error::ErrorAlmacen;
pub use estados::EstadoRepoGuardado;
pub use origenes::DatosOrigen;
pub use sincronizaciones::{NuevaSincronizacion, ResultadoSincronizacion, Sincronizacion};

/// Permisos del fichero de la base de datos: solo el dueño puede leerlo o escribirlo.
const MODO_FICHERO: u32 = 0o600;

/// Almacén SQLite de la app. Envuelve la conexión en un `Mutex` para ser `Send + Sync`;
/// los métodos son síncronos y el llamador async debe usar `spawn_blocking`.
pub struct Almacen {
    conexion: Mutex<Connection>,
}

impl Almacen {
    /// Abre (o crea) la base de datos en `ruta`.
    ///
    /// Si el fichero no existe se crea vacío con permisos 0600 antes de que SQLite
    /// escriba nada, para que nunca exista con permisos por defecto ni siquiera un
    /// instante. La base usa `journal_mode=DELETE` en vez de WAL: con WAL, SQLite crea
    /// ficheros auxiliares (`-wal`, `-shm`) cuyo modo depende del `umask` del proceso y
    /// no se puede fijar desde `rusqlite`, así que no se puede garantizar que hereden
    /// permisos restrictivos. Con `DELETE`, el único fichero en disco es `ruta`, cuyos
    /// permisos sí controlamos.
    pub fn abrir(ruta: &Path) -> Result<Self, ErrorAlmacen> {
        crear_fichero_privado_si_falta(ruta)?;
        let mut conexion = Connection::open(ruta)?;
        configurar(&mut conexion)?;
        migraciones::aplicar(&mut conexion)?;
        Ok(Self {
            conexion: Mutex::new(conexion),
        })
    }

    /// Base de datos en memoria, para pruebas.
    pub fn en_memoria() -> Result<Self, ErrorAlmacen> {
        let mut conexion = Connection::open_in_memory()?;
        configurar(&mut conexion)?;
        migraciones::aplicar(&mut conexion)?;
        Ok(Self {
            conexion: Mutex::new(conexion),
        })
    }

    /// Conexión interna, con el mutex ya bloqueado.
    fn conn(&self) -> Result<MutexGuard<'_, Connection>, ErrorAlmacen> {
        self.conexion.lock().map_err(|_| {
            ErrorAlmacen::Sqlite("el mutex interno del almacén está envenenado".to_string())
        })
    }
}

/// Aplica los ajustes de conexión comunes a `abrir` y `en_memoria`.
fn configurar(conexion: &mut Connection) -> Result<(), ErrorAlmacen> {
    conexion.pragma_update(None, "foreign_keys", "ON")?;
    conexion.pragma_update(None, "trusted_schema", "OFF")?;
    // Ver el comentario de `abrir`: DELETE en vez de WAL, para no crear ficheros
    // auxiliares cuyos permisos no podemos garantizar.
    conexion.pragma_update_and_check(None, "journal_mode", "DELETE", |_fila| Ok(()))?;
    // La app y el timer de sincronización pueden abrir la base a la vez.
    conexion.busy_timeout(Duration::from_secs(5))?;
    Ok(())
}

/// Crea `ruta` con permisos 0600 si no existe todavía, sin escribir nada (un fichero
/// vacío es una base de datos SQLite válida y vacía). Si otro proceso lo crea a la vez,
/// no es un error: solo importa que exista con permisos restrictivos.
fn crear_fichero_privado_si_falta(ruta: &Path) -> Result<(), ErrorAlmacen> {
    if let Some(directorio) = ruta.parent()
        && !directorio.as_os_str().is_empty()
        && !directorio.exists()
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directorio)?;
    }
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(MODO_FICHERO)
        .open(ruta)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Formatea una fecha para guardarla: RFC 3339 en UTC, truncada a segundos. La
/// precisión de segundo basta para histórico y auditoría, y garantiza que la
/// representación textual tiene siempre el mismo formato fijo, de modo que ordenar u
/// comparar por texto coincide con ordenar u comparar cronológicamente.
fn formatear_fecha(fecha: OffsetDateTime) -> Result<String, ErrorAlmacen> {
    let fecha = fecha
        .to_offset(time::UtcOffset::UTC)
        .replace_nanosecond(0)
        .map_err(|error| ErrorAlmacen::Sqlite(error.to_string()))?;
    fecha
        .format(&Rfc3339)
        .map_err(|error| ErrorAlmacen::Sqlite(error.to_string()))
}

/// Interpreta una fecha tal como la guardó [`formatear_fecha`].
fn parsear_fecha(texto: &str) -> Result<OffsetDateTime, ErrorAlmacen> {
    OffsetDateTime::parse(texto, &Rfc3339).map_err(|error| ErrorAlmacen::Sqlite(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn crea_el_fichero_con_permisos_0600() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("gitmereba.db");

        let _almacen = Almacen::abrir(&ruta).expect("abrir no falla");

        let permisos = std::fs::metadata(&ruta).expect("metadata").permissions();
        assert_eq!(permisos.mode() & 0o777, 0o600);
    }

    #[test]
    fn no_crea_ficheros_wal_ni_shm() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("gitmereba.db");

        let _almacen = Almacen::abrir(&ruta).expect("abrir no falla");

        assert!(!ruta.with_extension("db-wal").exists());
        assert!(!ruta.with_extension("db-shm").exists());
    }

    #[test]
    fn reabrir_el_mismo_fichero_no_falla() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("gitmereba.db");

        Almacen::abrir(&ruta).expect("primera apertura");
        let permisos_tras_primera = std::fs::metadata(&ruta)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;

        Almacen::abrir(&ruta).expect("segunda apertura");
        let permisos_tras_segunda = std::fs::metadata(&ruta)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(permisos_tras_primera, 0o600);
        assert_eq!(permisos_tras_segunda, 0o600);
    }

    #[test]
    fn dos_almacenes_sobre_el_mismo_fichero_escriben_sin_error() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("gitmereba.db");

        let a1 = Almacen::abrir(&ruta).expect("abrir a1");
        let a2 = Almacen::abrir(&ruta).expect("abrir a2");

        a1.auditar(None, "cuenta.alta", "desde a1")
            .expect("a1 escribe");
        a2.auditar(None, "cuenta.alta", "desde a2")
            .expect("a2 escribe");
        a1.auditar(None, "cuenta.alta", "de nuevo a1")
            .expect("a1 vuelve a escribir");

        assert_eq!(a1.auditoria(10, None).expect("leer auditoria").len(), 3);
    }

    #[test]
    fn en_memoria_aplica_las_migraciones() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        assert!(
            almacen
                .auditoria(1, None)
                .expect("leer auditoria")
                .is_empty()
        );
    }

    #[test]
    fn no_hay_funciones_publicas_para_alterar_la_auditoria() {
        // No incluye este propio fichero: el texto de este test menciona a propósito
        // los nombres prohibidos, así que se comprobaría a sí mismo en falso.
        let ficheros = [
            include_str!("auditoria.rs"),
            include_str!("estados.rs"),
            include_str!("sincronizaciones.rs"),
            include_str!("migraciones.rs"),
            include_str!("saneado.rs"),
            include_str!("error.rs"),
        ];
        let prohibidos =
            ["borrar", "editar", "modificar"].map(|verbo| format!("{verbo}_auditoria"));
        for prohibido in &prohibidos {
            for fichero in ficheros {
                assert!(
                    !fichero.contains(prohibido.as_str()),
                    "no debe existir «{prohibido}»"
                );
            }
        }
    }
}
