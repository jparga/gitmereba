//! Auditoría de solo-añadir (tabla `auditoria`).
//!
//! Cada entrada encadena con la anterior: `hash = SHA-256(hash_anterior || momento ||
//! cuenta || accion || detalle)`. La API pública de este fichero (y de todo el módulo
//! `almacen`) no tiene ninguna función que modifique o borre una entrada; además, la
//! tabla tiene triggers `BEFORE UPDATE`/`BEFORE DELETE` que abortan cualquier intento
//! por SQL. `auditar` usa una transacción `IMMEDIATE`, para que dos procesos (la app y
//! el timer) que escriban a la vez no calculen el mismo `hash_anterior`: SQLite serializa
//! con su propio bloqueo, y `busy_timeout` hace que el segundo espere en vez de fallar.

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::modelo::Nombre;

use super::saneado::sanear_y_recortar;
use super::{Almacen, ErrorAlmacen, formatear_fecha, parsear_fecha};

/// Longitud máxima, en caracteres, de `detalle`.
const LIMITE_DETALLE: usize = 2000;
/// Longitud máxima de `accion`.
const LONGITUD_MAXIMA_ACCION: usize = 64;

/// Una entrada de la auditoría, ya encadenada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntradaAuditoria {
    pub id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub momento: OffsetDateTime,
    pub cuenta: Option<Nombre>,
    pub accion: String,
    pub detalle: String,
    pub hash_anterior: String,
    pub hash: String,
}

/// Resultado de recorrer la cadena de auditoría.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificacionAuditoria {
    /// La cadena es coherente de principio a fin.
    Integra,
    /// La primera entrada rota (hash que no encaja) es la de este id.
    Rota { id: i64 },
}

/// Hash de la entrada anterior a la primera: no hay una entrada real antes de ella.
fn hash_genesis() -> String {
    "0".repeat(64)
}

/// Calcula el hash de una entrada a partir del hash anterior y sus datos.
fn calcular_hash(
    hash_anterior: &str,
    momento: &str,
    cuenta: &str,
    accion: &str,
    detalle: &str,
) -> String {
    // Cada campo va precedido de su longitud: sin ello, mover caracteres de un campo al
    // contiguo («ab»+«c» frente a «a»+«bc») daría el mismo hash.
    let mut hasher = Sha256::new();
    for campo in [hash_anterior, momento, cuenta, accion, detalle] {
        hasher.update((campo.len() as u64).to_be_bytes());
        hasher.update(campo.as_bytes());
    }
    a_hex(&hasher.finalize())
}

/// Codifica `bytes` en hexadecimal en minúsculas.
fn a_hex(bytes: &[u8]) -> String {
    let mut salida = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        salida.push_str(&format!("{byte:02x}"));
    }
    salida
}

/// Valida que `accion` cumple `[a-z0-9._-]{1,64}`.
fn validar_accion(accion: &str) -> Result<(), ErrorAlmacen> {
    if accion.is_empty() || accion.len() > LONGITUD_MAXIMA_ACCION {
        return Err(ErrorAlmacen::AccionInvalida);
    }
    let valida = accion
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'));
    if valida {
        Ok(())
    } else {
        Err(ErrorAlmacen::AccionInvalida)
    }
}

fn conversion_invalida(campo: &'static str, error: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        format!("columna «{campo}» inválida: {error}").into(),
    )
}

fn fila_a_entrada(fila: &Row<'_>) -> rusqlite::Result<EntradaAuditoria> {
    let momento_texto: String = fila.get("momento")?;
    let cuenta_texto: Option<String> = fila.get("cuenta")?;

    let momento =
        parsear_fecha(&momento_texto).map_err(|error| conversion_invalida("momento", error))?;
    let cuenta = cuenta_texto
        .map(Nombre::nuevo)
        .transpose()
        .map_err(|error| conversion_invalida("cuenta", error))?;

    Ok(EntradaAuditoria {
        id: fila.get("id")?,
        momento,
        cuenta,
        accion: fila.get("accion")?,
        detalle: fila.get("detalle")?,
        hash_anterior: fila.get("hash_anterior")?,
        hash: fila.get("hash")?,
    })
}

impl Almacen {
    /// Añade una entrada a la auditoría y devuelve su id. `detalle` se sanea (ver
    /// [`super::saneado`]) y se recorta a 2000 caracteres antes de guardarse ni de
    /// entrar en el cálculo del hash.
    pub fn auditar(
        &self,
        cuenta: Option<&Nombre>,
        accion: &str,
        detalle: &str,
    ) -> Result<i64, ErrorAlmacen> {
        validar_accion(accion)?;
        let detalle = sanear_y_recortar(detalle, LIMITE_DETALLE);
        let momento = formatear_fecha(OffsetDateTime::now_utc())?;
        let cuenta_texto = cuenta.map(Nombre::as_str);

        let mut conexion = self.conn()?;
        let transaccion = conexion.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let hash_anterior: String = transaccion
            .query_row(
                "SELECT hash FROM auditoria ORDER BY id DESC LIMIT 1",
                [],
                |fila| fila.get(0),
            )
            .optional()?
            .unwrap_or_else(hash_genesis);

        let hash = calcular_hash(
            &hash_anterior,
            &momento,
            cuenta_texto.unwrap_or(""),
            accion,
            &detalle,
        );

        transaccion.execute(
            "INSERT INTO auditoria (momento, cuenta, accion, detalle, hash_anterior, hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![momento, cuenta_texto, accion, detalle, hash_anterior, hash],
        )?;
        let id = transaccion.last_insert_rowid();
        transaccion.commit()?;
        Ok(id)
    }

    /// Lista la auditoría en orden cronológico ascendente, como mucho `limite` filas.
    /// Con `desde_id`, solo las entradas con id mayor (paginación hacia delante).
    pub fn auditoria(
        &self,
        limite: u32,
        desde_id: Option<i64>,
    ) -> Result<Vec<EntradaAuditoria>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut resultado = Vec::new();
        match desde_id {
            Some(desde_id) => {
                let mut consulta = conexion.prepare(
                    "SELECT id, momento, cuenta, accion, detalle, hash_anterior, hash \
                     FROM auditoria WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
                )?;
                let filas = consulta.query_map(params![desde_id, limite], fila_a_entrada)?;
                for fila in filas {
                    resultado.push(fila?);
                }
            }
            None => {
                let mut consulta = conexion.prepare(
                    "SELECT id, momento, cuenta, accion, detalle, hash_anterior, hash \
                     FROM auditoria ORDER BY id ASC LIMIT ?1",
                )?;
                let filas = consulta.query_map(params![limite], fila_a_entrada)?;
                for fila in filas {
                    resultado.push(fila?);
                }
            }
        }
        Ok(resultado)
    }

    /// Recorre la cadena de auditoría de principio a fin y comprueba que cada hash
    /// encaja con el anterior y con los datos de su fila.
    pub fn verificar_auditoria(&self) -> Result<VerificacionAuditoria, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut consulta = conexion.prepare(
            "SELECT id, momento, cuenta, accion, detalle, hash_anterior, hash FROM auditoria ORDER BY id ASC",
        )?;
        let filas = consulta.query_map([], |fila| {
            Ok((
                fila.get::<_, i64>("id")?,
                fila.get::<_, String>("momento")?,
                fila.get::<_, Option<String>>("cuenta")?,
                fila.get::<_, String>("accion")?,
                fila.get::<_, String>("detalle")?,
                fila.get::<_, String>("hash_anterior")?,
                fila.get::<_, String>("hash")?,
            ))
        })?;

        let mut esperado = hash_genesis();
        for fila in filas {
            let (id, momento, cuenta, accion, detalle, hash_anterior, hash) = fila?;
            if hash_anterior != esperado {
                return Ok(VerificacionAuditoria::Rota { id });
            }
            let calculado = calcular_hash(
                &hash_anterior,
                &momento,
                cuenta.as_deref().unwrap_or(""),
                &accion,
                &detalle,
            );
            if calculado != hash {
                return Ok(VerificacionAuditoria::Rota { id });
            }
            esperado = hash;
        }
        Ok(VerificacionAuditoria::Integra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;

    #[test]
    fn la_cadena_verifica_tras_varias_entradas() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("cuenta válida");

        almacen
            .auditar(Some(&cuenta), "cuenta.alta", "alta inicial")
            .expect("auditar 1");
        almacen
            .auditar(Some(&cuenta), "repo.excluir", "excluye x/y")
            .expect("auditar 2");
        almacen
            .auditar(None, "contingencia.activar", "activa z")
            .expect("auditar 3");

        assert_eq!(
            almacen.verificar_auditoria().expect("verificar"),
            VerificacionAuditoria::Integra
        );
        assert_eq!(almacen.auditoria(10, None).expect("listar").len(), 3);
    }

    #[test]
    fn la_primera_entrada_encadena_con_el_genesis() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        almacen.auditar(None, "cuenta.alta", "x").expect("auditar");

        let entradas = almacen.auditoria(1, None).expect("listar");
        assert_eq!(entradas[0].hash_anterior, hash_genesis());
    }

    #[test]
    fn accion_invalida_se_rechaza() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        for accion in ["", "Mayusculas", "con espacio", &"a".repeat(65), "acción-ñ"] {
            let error = almacen
                .auditar(None, accion, "x")
                .expect_err("debe rechazarse");
            assert_eq!(error, ErrorAlmacen::AccionInvalida);
        }
    }

    #[test]
    fn la_inyeccion_sql_en_detalle_se_guarda_literal() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let detalle = "'); DROP TABLE auditoria;--";
        almacen
            .auditar(None, "cuenta.alta", detalle)
            .expect("auditar");

        let entradas = almacen.auditoria(1, None).expect("listar");
        assert_eq!(entradas[0].detalle, detalle);
        // La tabla sigue existiendo: si la inyección hubiera funcionado, esto fallaría.
        assert_eq!(
            almacen.verificar_auditoria().expect("verificar"),
            VerificacionAuditoria::Integra
        );
    }

    #[test]
    fn update_directo_falla_por_el_trigger() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        almacen.auditar(None, "cuenta.alta", "x").expect("auditar");

        let conexion = almacen.conexion.lock().expect("bloquear conexión");
        let resultado = conexion.execute("UPDATE auditoria SET detalle = 'y' WHERE id = 1", []);
        assert!(resultado.is_err());
    }

    #[test]
    fn delete_directo_falla_por_el_trigger() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        almacen.auditar(None, "cuenta.alta", "x").expect("auditar");

        let conexion = almacen.conexion.lock().expect("bloquear conexión");
        let resultado = conexion.execute("DELETE FROM auditoria WHERE id = 1", []);
        assert!(resultado.is_err());
    }

    #[test]
    fn desactivar_el_trigger_y_alterar_una_fila_se_detecta() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        almacen
            .auditar(None, "cuenta.alta", "primera")
            .expect("auditar 1");
        almacen
            .auditar(None, "cuenta.alta", "segunda")
            .expect("auditar 2");

        {
            let conexion = almacen.conexion.lock().expect("bloquear conexión");
            conexion
                .execute_batch("DROP TRIGGER auditoria_sin_update;")
                .expect("quitar el trigger");
            conexion
                .execute("UPDATE auditoria SET detalle = 'alterada' WHERE id = 1", [])
                .expect("alterar la fila sin el trigger");
        }

        let verificacion = almacen.verificar_auditoria().expect("verificar");
        assert_eq!(verificacion, VerificacionAuditoria::Rota { id: 1 });
    }

    #[test]
    fn el_hash_distingue_los_limites_entre_campos() {
        let a = calcular_hash("h", "m", "ab", "c", "d");
        let b = calcular_hash("h", "m", "a", "bc", "d");
        assert_ne!(a, b);
    }
}
