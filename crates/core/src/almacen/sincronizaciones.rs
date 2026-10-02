//! Histórico de sincronizaciones (tabla `sincronizaciones`).

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::modelo::Nombre;

use super::saneado::sanear_y_recortar;
use super::{Almacen, ErrorAlmacen, formatear_fecha, parsear_fecha};

/// Longitud máxima, en caracteres, de `resumen`.
const LIMITE_RESUMEN: usize = 2000;
/// Tamaño máximo, en bytes, de `detalle_json` (1 MB decimal).
const TAMANO_MAXIMO_DETALLE_JSON: usize = 1_000_000;

/// Resultado global de una sincronización.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResultadoSincronizacion {
    Ok,
    ConFallos,
    Error,
}

impl ResultadoSincronizacion {
    fn como_texto(self) -> &'static str {
        match self {
            ResultadoSincronizacion::Ok => "ok",
            ResultadoSincronizacion::ConFallos => "con-fallos",
            ResultadoSincronizacion::Error => "error",
        }
    }

    fn desde_texto(texto: &str) -> Result<Self, ErrorAlmacen> {
        match texto {
            "ok" => Ok(ResultadoSincronizacion::Ok),
            "con-fallos" => Ok(ResultadoSincronizacion::ConFallos),
            "error" => Ok(ResultadoSincronizacion::Error),
            otro => Err(ErrorAlmacen::Sqlite(format!(
                "resultado de sincronización desconocido en la base de datos: «{otro}»"
            ))),
        }
    }
}

/// Datos de una sincronización recién terminada, a registrar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NuevaSincronizacion {
    pub cuenta: Nombre,
    #[serde(with = "time::serde::rfc3339")]
    pub inicio: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub fin: OffsetDateTime,
    pub resultado: ResultadoSincronizacion,
    pub creados: u32,
    pub huerfanos: u32,
    pub fallos: u32,
    pub resumen: String,
    /// JSON opaco con el detalle; tope de 1 MB una vez saneado.
    pub detalle_json: Option<String>,
}

/// Una sincronización ya registrada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sincronizacion {
    pub id: i64,
    pub cuenta: Nombre,
    #[serde(with = "time::serde::rfc3339")]
    pub inicio: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub fin: OffsetDateTime,
    pub resultado: ResultadoSincronizacion,
    pub creados: u32,
    pub huerfanos: u32,
    pub fallos: u32,
    pub resumen: String,
    pub detalle_json: Option<String>,
}

fn fila_a_sincronizacion(fila: &Row<'_>) -> rusqlite::Result<Sincronizacion> {
    let cuenta_texto: String = fila.get("cuenta")?;
    let inicio_texto: String = fila.get("inicio")?;
    let fin_texto: String = fila.get("fin")?;
    let resultado_texto: String = fila.get("resultado")?;

    let cuenta =
        Nombre::nuevo(cuenta_texto).map_err(|error| conversion_invalida("cuenta", error))?;
    let inicio =
        parsear_fecha(&inicio_texto).map_err(|error| conversion_invalida("inicio", error))?;
    let fin = parsear_fecha(&fin_texto).map_err(|error| conversion_invalida("fin", error))?;
    let resultado = ResultadoSincronizacion::desde_texto(&resultado_texto)
        .map_err(|error| conversion_invalida("resultado", error))?;

    Ok(Sincronizacion {
        id: fila.get("id")?,
        cuenta,
        inicio,
        fin,
        resultado,
        creados: fila.get("creados")?,
        huerfanos: fila.get("huerfanos")?,
        fallos: fila.get("fallos")?,
        resumen: fila.get("resumen")?,
        detalle_json: fila.get("detalle_json")?,
    })
}

/// Envuelve un error propio como fallo de conversión de una columna, para poder
/// devolverlo desde un cierre de `query_map` (que exige `rusqlite::Error`).
fn conversion_invalida(
    campo: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        format!("columna «{campo}» inválida: {error}").into(),
    )
}

const COLUMNAS: &str =
    "id, cuenta, inicio, fin, resultado, creados, huerfanos, fallos, resumen, detalle_json";

impl Almacen {
    /// Registra una sincronización ya terminada y devuelve su id.
    pub fn registrar_sincronizacion(
        &self,
        nueva: &NuevaSincronizacion,
    ) -> Result<i64, ErrorAlmacen> {
        let resumen = sanear_y_recortar(&nueva.resumen, LIMITE_RESUMEN);
        let detalle_json = match &nueva.detalle_json {
            Some(detalle) => {
                let saneado = super::saneado::sanear_detalle(detalle);
                if saneado.len() > TAMANO_MAXIMO_DETALLE_JSON {
                    return Err(ErrorAlmacen::DetalleDemasiadoGrande);
                }
                Some(saneado)
            }
            None => None,
        };
        let inicio = formatear_fecha(nueva.inicio)?;
        let fin = formatear_fecha(nueva.fin)?;

        let conexion = self.conn()?;
        conexion.execute(
            "INSERT INTO sincronizaciones \
             (cuenta, inicio, fin, resultado, creados, huerfanos, fallos, resumen, detalle_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                nueva.cuenta.as_str(),
                inicio,
                fin,
                nueva.resultado.como_texto(),
                nueva.creados,
                nueva.huerfanos,
                nueva.fallos,
                resumen,
                detalle_json,
            ],
        )?;
        Ok(conexion.last_insert_rowid())
    }

    /// Últimas sincronizaciones, de más a menos reciente, de `cuenta` o de todas si es
    /// `None`. Como mucho `limite` filas.
    pub fn ultimas_sincronizaciones(
        &self,
        cuenta: Option<&Nombre>,
        limite: u32,
    ) -> Result<Vec<Sincronizacion>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut resultado = Vec::new();
        match cuenta {
            Some(cuenta) => {
                let mut consulta = conexion.prepare(&format!(
                    "SELECT {COLUMNAS} FROM sincronizaciones WHERE cuenta = ?1 ORDER BY id DESC LIMIT ?2"
                ))?;
                let filas =
                    consulta.query_map(params![cuenta.as_str(), limite], fila_a_sincronizacion)?;
                for fila in filas {
                    resultado.push(fila?);
                }
            }
            None => {
                let mut consulta = conexion.prepare(&format!(
                    "SELECT {COLUMNAS} FROM sincronizaciones ORDER BY id DESC LIMIT ?1"
                ))?;
                let filas = consulta.query_map(params![limite], fila_a_sincronizacion)?;
                for fila in filas {
                    resultado.push(fila?);
                }
            }
        }
        Ok(resultado)
    }

    /// Última sincronización cuyo resultado fue `ok`, si la hay.
    pub fn ultima_correcta(&self, cuenta: &Nombre) -> Result<Option<Sincronizacion>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut consulta = conexion.prepare(&format!(
            "SELECT {COLUMNAS} FROM sincronizaciones \
             WHERE cuenta = ?1 AND resultado = 'ok' ORDER BY id DESC LIMIT 1"
        ))?;
        consulta
            .query_row(params![cuenta.as_str()], fila_a_sincronizacion)
            .optional()
            .map_err(ErrorAlmacen::from)
    }

    /// Primera sincronización registrada de `cuenta` (la más antigua), sea cual sea su
    /// resultado. Se usa para saber desde cuándo existe la cuenta cuando no hay otra
    /// forma de saberlo (regla «nunca sincronizado»).
    pub fn primera_sincronizacion(
        &self,
        cuenta: &Nombre,
    ) -> Result<Option<Sincronizacion>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut consulta = conexion.prepare(&format!(
            "SELECT {COLUMNAS} FROM sincronizaciones WHERE cuenta = ?1 ORDER BY id ASC LIMIT 1"
        ))?;
        consulta
            .query_row(params![cuenta.as_str()], fila_a_sincronizacion)
            .optional()
            .map_err(ErrorAlmacen::from)
    }

    /// Borra las sincronizaciones cuyo fin sea anterior a `conservar_dias` atrás.
    /// Devuelve cuántas filas se han borrado. La auditoría no se purga nunca.
    pub fn purgar_sincronizaciones(&self, conservar_dias: u32) -> Result<u64, ErrorAlmacen> {
        let corte = OffsetDateTime::now_utc() - time::Duration::days(i64::from(conservar_dias));
        let corte_texto = formatear_fecha(corte)?;
        let conexion = self.conn()?;
        let borradas = conexion.execute(
            "DELETE FROM sincronizaciones WHERE fin < ?1",
            params![corte_texto],
        )?;
        Ok(borradas as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;

    fn nueva(
        cuenta: &str,
        fin: OffsetDateTime,
        resultado: ResultadoSincronizacion,
    ) -> NuevaSincronizacion {
        NuevaSincronizacion {
            cuenta: Nombre::nuevo(cuenta).expect("nombre válido"),
            inicio: fin - time::Duration::minutes(5),
            fin,
            resultado,
            creados: 1,
            huerfanos: 0,
            fallos: 0,
            resumen: "resumen de prueba".to_string(),
            detalle_json: Some("{\"detalle\":true}".to_string()),
        }
    }

    #[test]
    fn ida_y_vuelta_con_orden_descendente_y_limite() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let base = OffsetDateTime::now_utc();
        for i in 0..5 {
            let dato = nueva(
                "jparga",
                base + time::Duration::minutes(i),
                ResultadoSincronizacion::Ok,
            );
            almacen.registrar_sincronizacion(&dato).expect("registrar");
        }

        let listado = almacen
            .ultimas_sincronizaciones(Some(&Nombre::nuevo("jparga").expect("nombre")), 3)
            .expect("listar");

        assert_eq!(listado.len(), 3);
        assert!(listado[0].id > listado[1].id);
        assert!(listado[1].id > listado[2].id);
    }

    #[test]
    fn ultima_correcta_ignora_las_fallidas() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("nombre");
        let base = OffsetDateTime::now_utc();

        almacen
            .registrar_sincronizacion(&nueva("jparga", base, ResultadoSincronizacion::Ok))
            .expect("registrar ok");
        almacen
            .registrar_sincronizacion(&nueva(
                "jparga",
                base + time::Duration::minutes(10),
                ResultadoSincronizacion::Error,
            ))
            .expect("registrar error");

        let ultima = almacen
            .ultima_correcta(&cuenta)
            .expect("consultar")
            .expect("hay una sincronización correcta");
        assert_eq!(ultima.resultado, ResultadoSincronizacion::Ok);
    }

    #[test]
    fn ultima_correcta_devuelve_none_si_no_hay_ninguna() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("nombre");
        assert!(
            almacen
                .ultima_correcta(&cuenta)
                .expect("consultar")
                .is_none()
        );
    }

    #[test]
    fn primera_sincronizacion_devuelve_la_mas_antigua() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("nombre");
        let base = OffsetDateTime::now_utc();
        for i in 0..3 {
            almacen
                .registrar_sincronizacion(&nueva(
                    "jparga",
                    base + time::Duration::minutes(i),
                    ResultadoSincronizacion::Ok,
                ))
                .expect("registrar");
        }

        let primera = almacen
            .primera_sincronizacion(&cuenta)
            .expect("consultar")
            .expect("hay una primera sincronización");

        // `formatear_fecha` trunca a segundos al guardar: se compara con la misma
        // precisión para no depender de los nanosegundos de `OffsetDateTime::now_utc()`.
        assert_eq!(
            primera.fin,
            base.replace_nanosecond(0).expect("truncar a segundos")
        );
    }

    #[test]
    fn primera_sincronizacion_devuelve_none_si_no_hay_ninguna() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("nombre");
        assert!(
            almacen
                .primera_sincronizacion(&cuenta)
                .expect("consultar")
                .is_none()
        );
    }

    #[test]
    fn primera_sincronizacion_no_mezcla_cuentas() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let base = OffsetDateTime::now_utc();
        almacen
            .registrar_sincronizacion(&nueva("otra", base, ResultadoSincronizacion::Ok))
            .expect("registrar otra cuenta");
        almacen
            .registrar_sincronizacion(&nueva(
                "jparga",
                base + time::Duration::minutes(5),
                ResultadoSincronizacion::Ok,
            ))
            .expect("registrar jparga");

        let primera = almacen
            .primera_sincronizacion(&Nombre::nuevo("jparga").expect("nombre"))
            .expect("consultar")
            .expect("hay una primera sincronización de jparga");

        assert_eq!(
            primera.fin,
            (base + time::Duration::minutes(5))
                .replace_nanosecond(0)
                .expect("truncar a segundos")
        );
    }

    #[test]
    fn purgar_respeta_la_auditoria() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let antigua = OffsetDateTime::now_utc() - time::Duration::days(40);
        almacen
            .registrar_sincronizacion(&nueva("jparga", antigua, ResultadoSincronizacion::Ok))
            .expect("registrar antigua");
        almacen
            .registrar_sincronizacion(&nueva(
                "jparga",
                OffsetDateTime::now_utc(),
                ResultadoSincronizacion::Ok,
            ))
            .expect("registrar reciente");
        almacen
            .auditar(None, "sincronizacion.ok", "detalle")
            .expect("auditar");

        let borradas = almacen.purgar_sincronizaciones(30).expect("purgar");

        assert_eq!(borradas, 1);
        assert_eq!(
            almacen
                .ultimas_sincronizaciones(None, 10)
                .expect("listar")
                .len(),
            1
        );
        assert_eq!(
            almacen.auditoria(10, None).expect("leer auditoria").len(),
            1
        );
    }

    #[test]
    fn detalle_json_demasiado_grande_es_un_error() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let mut dato = nueva(
            "jparga",
            OffsetDateTime::now_utc(),
            ResultadoSincronizacion::Ok,
        );
        dato.detalle_json = Some("a".repeat(TAMANO_MAXIMO_DETALLE_JSON + 1));

        let error = almacen
            .registrar_sincronizacion(&dato)
            .expect_err("debe rechazarse");
        assert_eq!(error, ErrorAlmacen::DetalleDemasiadoGrande);
    }

    #[test]
    fn el_resumen_se_recorta_a_dos_mil_caracteres() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let mut dato = nueva(
            "jparga",
            OffsetDateTime::now_utc(),
            ResultadoSincronizacion::Ok,
        );
        dato.resumen = "x".repeat(5000);

        almacen.registrar_sincronizacion(&dato).expect("registrar");
        let listado = almacen.ultimas_sincronizaciones(None, 1).expect("listar");
        assert_eq!(listado[0].resumen.chars().count(), LIMITE_RESUMEN);
    }
}
