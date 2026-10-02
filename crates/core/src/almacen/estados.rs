//! Último estado conocido de cada repo (tabla `estado_repos`).

use rusqlite::{Row, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::modelo::{EstadoRepo, IdRepo, Nombre};

use super::saneado::sanear_y_recortar;
use super::{Almacen, ErrorAlmacen, formatear_fecha, parsear_fecha};

/// Longitud máxima, en caracteres, de `detalle`.
const LIMITE_DETALLE: usize = 2000;

/// Estado guardado de un repo, tal como lo dejó la última sincronización o verificación.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoRepoGuardado {
    pub cuenta: Nombre,
    pub id: IdRepo,
    pub estado: EstadoRepo,
    pub detalle: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub actualizado: OffsetDateTime,
}

fn estado_a_texto(estado: EstadoRepo) -> &'static str {
    match estado {
        EstadoRepo::Fallo => "fallo",
        EstadoRepo::Obsoleto => "obsoleto",
        EstadoRepo::Huerfano => "huerfano",
        EstadoRepo::Contingencia => "contingencia",
        EstadoRepo::Excluido => "excluido",
        EstadoRepo::Ok => "ok",
    }
}

fn estado_desde_texto(texto: &str) -> Result<EstadoRepo, ErrorAlmacen> {
    match texto {
        "fallo" => Ok(EstadoRepo::Fallo),
        "obsoleto" => Ok(EstadoRepo::Obsoleto),
        "huerfano" => Ok(EstadoRepo::Huerfano),
        "contingencia" => Ok(EstadoRepo::Contingencia),
        "excluido" => Ok(EstadoRepo::Excluido),
        "ok" => Ok(EstadoRepo::Ok),
        otro => Err(ErrorAlmacen::Sqlite(format!(
            "estado de repo desconocido en la base de datos: «{otro}»"
        ))),
    }
}

fn fila_a_estado(fila: &Row<'_>) -> rusqlite::Result<EstadoRepoGuardado> {
    let convertir = |campo: &'static str, error: ErrorAlmacen| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("columna «{campo}» inválida: {error}").into(),
        )
    };

    let cuenta_texto: String = fila.get("cuenta")?;
    let dueno_texto: String = fila.get("dueno")?;
    let nombre_texto: String = fila.get("nombre")?;
    let estado_texto: String = fila.get("estado")?;
    let actualizado_texto: String = fila.get("actualizado")?;

    let cuenta = Nombre::nuevo(cuenta_texto)
        .map_err(|error| convertir("cuenta", ErrorAlmacen::Sqlite(error.to_string())))?;
    let dueno = Nombre::nuevo(dueno_texto)
        .map_err(|error| convertir("dueno", ErrorAlmacen::Sqlite(error.to_string())))?;
    let nombre = Nombre::nuevo(nombre_texto)
        .map_err(|error| convertir("nombre", ErrorAlmacen::Sqlite(error.to_string())))?;
    let estado = estado_desde_texto(&estado_texto).map_err(|error| convertir("estado", error))?;
    let actualizado =
        parsear_fecha(&actualizado_texto).map_err(|error| convertir("actualizado", error))?;

    Ok(EstadoRepoGuardado {
        cuenta,
        id: IdRepo { dueno, nombre },
        estado,
        detalle: fila.get("detalle")?,
        actualizado,
    })
}

impl Almacen {
    /// Da de alta o actualiza el estado de `id` para `cuenta` (upsert).
    pub fn guardar_estado_repo(
        &self,
        cuenta: &Nombre,
        id: &IdRepo,
        estado: EstadoRepo,
        detalle: Option<&str>,
    ) -> Result<(), ErrorAlmacen> {
        let detalle = detalle.map(|texto| sanear_y_recortar(texto, LIMITE_DETALLE));
        let actualizado = formatear_fecha(OffsetDateTime::now_utc())?;

        let conexion = self.conn()?;
        conexion.execute(
            "INSERT INTO estado_repos (cuenta, dueno, nombre, estado, detalle, actualizado) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (cuenta, dueno, nombre) DO UPDATE SET \
             estado = excluded.estado, detalle = excluded.detalle, actualizado = excluded.actualizado",
            params![
                cuenta.as_str(),
                id.dueno.as_str(),
                id.nombre.as_str(),
                estado_a_texto(estado),
                detalle,
                actualizado,
            ],
        )?;
        Ok(())
    }

    /// Todos los estados guardados de `cuenta`.
    pub fn estados_de(&self, cuenta: &Nombre) -> Result<Vec<EstadoRepoGuardado>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut consulta = conexion.prepare(
            "SELECT cuenta, dueno, nombre, estado, detalle, actualizado \
             FROM estado_repos WHERE cuenta = ?1 ORDER BY dueno, nombre",
        )?;
        let filas = consulta.query_map(params![cuenta.as_str()], fila_a_estado)?;
        let mut resultado = Vec::new();
        for fila in filas {
            resultado.push(fila?);
        }
        Ok(resultado)
    }

    /// Borra el estado guardado de `id` (su mirror se ha descartado para volver a
    /// clonarlo). Borrar algo que no existe no es un error.
    pub fn borrar_estado_repo(&self, cuenta: &Nombre, id: &IdRepo) -> Result<(), ErrorAlmacen> {
        let conexion = self.conn()?;
        conexion.execute(
            "DELETE FROM estado_repos WHERE cuenta = ?1 AND dueno = ?2 AND nombre = ?3",
            params![cuenta.as_str(), id.dueno.as_str(), id.nombre.as_str()],
        )?;
        Ok(())
    }

    /// Borra todos los estados guardados de `cuenta` (al dar de baja la cuenta). No es
    /// auditoría: se permite borrar.
    pub fn borrar_estados_de(&self, cuenta: &Nombre) -> Result<(), ErrorAlmacen> {
        let conexion = self.conn()?;
        conexion.execute(
            "DELETE FROM estado_repos WHERE cuenta = ?1",
            params![cuenta.as_str()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;

    fn id(dueno: &str, nombre: &str) -> IdRepo {
        IdRepo {
            dueno: Nombre::nuevo(dueno).expect("dueno válido"),
            nombre: Nombre::nuevo(nombre).expect("nombre válido"),
        }
    }

    #[test]
    fn guardar_y_leer_el_estado_de_un_repo() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("cuenta válida");
        let repo = id("jparga", "gitmereba");

        almacen
            .guardar_estado_repo(&cuenta, &repo, EstadoRepo::Ok, None)
            .expect("guardar");

        let estados = almacen.estados_de(&cuenta).expect("leer estados");
        assert_eq!(estados.len(), 1);
        assert_eq!(estados[0].id, repo);
        assert_eq!(estados[0].estado, EstadoRepo::Ok);
    }

    #[test]
    fn guardar_dos_veces_es_un_upsert() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta = Nombre::nuevo("jparga").expect("cuenta válida");
        let repo = id("jparga", "gitmereba");

        almacen
            .guardar_estado_repo(&cuenta, &repo, EstadoRepo::Ok, None)
            .expect("primer guardado");
        almacen
            .guardar_estado_repo(&cuenta, &repo, EstadoRepo::Fallo, Some("se rompió"))
            .expect("segundo guardado");

        let estados = almacen.estados_de(&cuenta).expect("leer estados");
        assert_eq!(estados.len(), 1);
        assert_eq!(estados[0].estado, EstadoRepo::Fallo);
        assert_eq!(estados[0].detalle.as_deref(), Some("se rompió"));
    }

    #[test]
    fn borrar_estados_de_solo_afecta_a_esa_cuenta() {
        let almacen = Almacen::en_memoria().expect("abrir en memoria");
        let cuenta_a = Nombre::nuevo("jparga").expect("cuenta válida");
        let cuenta_b = Nombre::nuevo("otra").expect("cuenta válida");

        almacen
            .guardar_estado_repo(&cuenta_a, &id("jparga", "uno"), EstadoRepo::Ok, None)
            .expect("guardar a");
        almacen
            .guardar_estado_repo(&cuenta_b, &id("otra", "dos"), EstadoRepo::Ok, None)
            .expect("guardar b");

        almacen.borrar_estados_de(&cuenta_a).expect("borrar a");

        assert!(almacen.estados_de(&cuenta_a).expect("leer a").is_empty());
        assert_eq!(almacen.estados_de(&cuenta_b).expect("leer b").len(), 1);
    }
}
