//! Punto de partida de una contingencia activada (tabla `contingencia_puntos`).
//!
//! `contingencia::activar` calcula el [`PuntoDePartida`] de un repo al activarlo, pero
//! ese módulo no tiene almacén propio (ver su documentación: «el llamador, fuera de
//! este módulo, es quien guarda `PuntoDePartida`»). Quien lo active debe persistirlo
//! aquí para que `contingencia::reconciliar` (y la vista de Contingencia) puedan volver
//! a leerlo en una llamada posterior.

use rusqlite::{OptionalExtension, params};

use crate::contingencia::PuntoDePartida;
use crate::modelo::{IdRepo, Nombre};

use super::{Almacen, ErrorAlmacen};

impl Almacen {
    /// Guarda (o sustituye) el punto de partida de `id` para `cuenta`.
    pub fn guardar_punto_de_partida(
        &self,
        cuenta: &Nombre,
        id: &IdRepo,
        punto: &PuntoDePartida,
    ) -> Result<(), ErrorAlmacen> {
        let json = serde_json::to_string(punto)
            .map_err(|error| ErrorAlmacen::Sqlite(error.to_string()))?;
        let conexion = self.conn()?;
        conexion.execute(
            "INSERT INTO contingencia_puntos (cuenta, dueno, nombre, punto_json) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (cuenta, dueno, nombre) DO UPDATE SET punto_json = excluded.punto_json",
            params![cuenta.as_str(), id.dueno.as_str(), id.nombre.as_str(), json],
        )?;
        Ok(())
    }

    /// Punto de partida guardado de `id` para `cuenta`, o `None` si su contingencia no
    /// se ha activado (o ya se cerró) desde esta app.
    pub fn punto_de_partida(
        &self,
        cuenta: &Nombre,
        id: &IdRepo,
    ) -> Result<Option<PuntoDePartida>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let json: Option<String> = conexion
            .query_row(
                "SELECT punto_json FROM contingencia_puntos \
                 WHERE cuenta = ?1 AND dueno = ?2 AND nombre = ?3",
                params![cuenta.as_str(), id.dueno.as_str(), id.nombre.as_str()],
                |fila| fila.get(0),
            )
            .optional()?;
        match json {
            Some(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|error| ErrorAlmacen::Sqlite(error.to_string())),
            None => Ok(None),
        }
    }

    /// Repos de `cuenta` con una contingencia activa (los originales, no sus copias
    /// hermanas), ordenados por dueño y nombre.
    pub fn contingencias_de(&self, cuenta: &Nombre) -> Result<Vec<IdRepo>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut consulta = conexion.prepare(
            "SELECT dueno, nombre FROM contingencia_puntos WHERE cuenta = ?1 \
             ORDER BY dueno, nombre",
        )?;
        let filas = consulta.query_map(params![cuenta.as_str()], |fila| {
            Ok((fila.get::<_, String>(0)?, fila.get::<_, String>(1)?))
        })?;
        let mut ids = Vec::new();
        for fila in filas {
            let (dueno, nombre) = fila?;
            let dueno = Nombre::nuevo(dueno).map_err(|e| ErrorAlmacen::Sqlite(e.to_string()))?;
            let nombre = Nombre::nuevo(nombre).map_err(|e| ErrorAlmacen::Sqlite(e.to_string()))?;
            ids.push(IdRepo { dueno, nombre });
        }
        Ok(ids)
    }

    /// Borra el punto de partida guardado de `id` para `cuenta` (al cerrar del todo su
    /// contingencia). Borrar algo que no existe no es un error.
    pub fn borrar_punto_de_partida(
        &self,
        cuenta: &Nombre,
        id: &IdRepo,
    ) -> Result<(), ErrorAlmacen> {
        let conexion = self.conn()?;
        conexion.execute(
            "DELETE FROM contingencia_puntos WHERE cuenta = ?1 AND dueno = ?2 AND nombre = ?3",
            params![cuenta.as_str(), id.dueno.as_str(), id.nombre.as_str()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use std::collections::BTreeMap;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    fn punto(refs: &[(&str, &str)]) -> PuntoDePartida {
        PuntoDePartida {
            refs: refs
                .iter()
                .map(|(r, sha)| (r.to_string(), sha.to_string()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn guardar_y_leer_ida_y_vuelta() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");
        let repo = id("jparga", "incidente");
        let punto = punto(&[("refs/heads/main", "aaa")]);

        almacen
            .guardar_punto_de_partida(&cuenta, &repo, &punto)
            .expect("guardar");

        let leido = almacen
            .punto_de_partida(&cuenta, &repo)
            .expect("leer")
            .expect("hay punto de partida guardado");
        assert_eq!(leido, punto);
    }

    #[test]
    fn leer_sin_guardar_devuelve_none() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");
        let repo = id("jparga", "incidente");

        assert!(
            almacen
                .punto_de_partida(&cuenta, &repo)
                .expect("leer")
                .is_none()
        );
    }

    #[test]
    fn guardar_dos_veces_es_un_upsert() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");
        let repo = id("jparga", "incidente");

        almacen
            .guardar_punto_de_partida(&cuenta, &repo, &punto(&[("refs/heads/main", "aaa")]))
            .expect("primer guardado");
        almacen
            .guardar_punto_de_partida(&cuenta, &repo, &punto(&[("refs/heads/main", "bbb")]))
            .expect("segundo guardado");

        let leido = almacen
            .punto_de_partida(&cuenta, &repo)
            .expect("leer")
            .expect("hay punto de partida guardado");
        assert_eq!(leido.sha_de("refs/heads/main"), Some("bbb"));
    }

    #[test]
    fn borrar_quita_el_punto_de_partida() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");
        let repo = id("jparga", "incidente");
        almacen
            .guardar_punto_de_partida(&cuenta, &repo, &punto(&[("refs/heads/main", "aaa")]))
            .expect("guardar");

        almacen
            .borrar_punto_de_partida(&cuenta, &repo)
            .expect("borrar");

        assert!(
            almacen
                .punto_de_partida(&cuenta, &repo)
                .expect("leer")
                .is_none()
        );
    }

    #[test]
    fn borrar_sin_guardar_no_es_un_error() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");
        let repo = id("jparga", "incidente");
        almacen
            .borrar_punto_de_partida(&cuenta, &repo)
            .expect("borrar");
    }

    #[test]
    fn contingencias_de_lista_solo_las_de_la_cuenta() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let vacio = PuntoDePartida::default();
        almacen
            .guardar_punto_de_partida(&nombre("jparga"), &id("jparga", "b"), &vacio)
            .expect("guardar");
        almacen
            .guardar_punto_de_partida(&nombre("jparga"), &id("jparga", "a"), &vacio)
            .expect("guardar");
        almacen
            .guardar_punto_de_partida(&nombre("otra"), &id("otra", "z"), &vacio)
            .expect("guardar");

        let ids = almacen.contingencias_de(&nombre("jparga")).expect("listar");

        assert_eq!(ids, vec![id("jparga", "a"), id("jparga", "b")]);
    }
}
