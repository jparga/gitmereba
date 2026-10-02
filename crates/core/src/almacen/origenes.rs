//! Datos de GitHub de cada repo que la copia local no conserva (tabla `repos_origen`).
//!
//! El mirror de Gitea es siempre privado y no sabe si el original es un fork o está
//! archivado. La sincronización guarda aquí lo que dijo GitHub en su última pasada, para
//! que la ventana pueda enseñarlo sin volver a llamar a GitHub.

use rusqlite::params;

use crate::modelo::{IdRepo, Nombre, RepoOrigen};

use super::{Almacen, ErrorAlmacen};

/// Lo que GitHub dijo de un repo en la última sincronización que lo listó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosOrigen {
    pub id: IdRepo,
    pub privado: bool,
    pub es_fork: bool,
    pub archivado: bool,
}

impl Almacen {
    /// Guarda (upsert) los datos de `origen` para `cuenta`, en una sola transacción.
    ///
    /// No borra los que falten: si el listado de un dueño falló en esta pasada, sus
    /// repos conservan lo último que se supo de ellos.
    pub fn guardar_origenes(
        &self,
        cuenta: &Nombre,
        origen: &[RepoOrigen],
    ) -> Result<(), ErrorAlmacen> {
        let mut conexion = self.conn()?;
        let transaccion = conexion.transaction()?;
        for repo in origen {
            transaccion.execute(
                "INSERT INTO repos_origen (cuenta, dueno, nombre, privado, es_fork, archivado) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT (cuenta, dueno, nombre) DO UPDATE SET \
                 privado = excluded.privado, es_fork = excluded.es_fork, \
                 archivado = excluded.archivado",
                params![
                    cuenta.as_str(),
                    repo.id.dueno.as_str(),
                    repo.id.nombre.as_str(),
                    repo.privado,
                    repo.es_fork,
                    repo.archivado,
                ],
            )?;
        }
        transaccion.commit()?;
        Ok(())
    }

    /// Datos de origen guardados de `cuenta`, ordenados por dueño y nombre.
    pub fn origenes_de(&self, cuenta: &Nombre) -> Result<Vec<DatosOrigen>, ErrorAlmacen> {
        let conexion = self.conn()?;
        let mut consulta = conexion.prepare(
            "SELECT dueno, nombre, privado, es_fork, archivado FROM repos_origen \
             WHERE cuenta = ?1 ORDER BY dueno, nombre",
        )?;
        let filas = consulta.query_map(params![cuenta.as_str()], |fila| {
            Ok((
                fila.get::<_, String>(0)?,
                fila.get::<_, String>(1)?,
                fila.get::<_, bool>(2)?,
                fila.get::<_, bool>(3)?,
                fila.get::<_, bool>(4)?,
            ))
        })?;
        let mut datos = Vec::new();
        for fila in filas {
            let (dueno, nombre, privado, es_fork, archivado) = fila?;
            let dueno = Nombre::nuevo(dueno).map_err(|e| ErrorAlmacen::Sqlite(e.to_string()))?;
            let nombre = Nombre::nuevo(nombre).map_err(|e| ErrorAlmacen::Sqlite(e.to_string()))?;
            datos.push(DatosOrigen {
                id: IdRepo { dueno, nombre },
                privado,
                es_fork,
                archivado,
            });
        }
        Ok(datos)
    }

    /// Borra los datos de origen de `cuenta` (al darla de baja).
    pub fn borrar_origenes_de(&self, cuenta: &Nombre) -> Result<(), ErrorAlmacen> {
        let conexion = self.conn()?;
        conexion.execute(
            "DELETE FROM repos_origen WHERE cuenta = ?1",
            params![cuenta.as_str()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn origen(nombre_repo: &str, privado: bool, es_fork: bool, archivado: bool) -> RepoOrigen {
        RepoOrigen {
            id: IdRepo {
                dueno: nombre("jparga"),
                nombre: nombre(nombre_repo),
            },
            url_clon: format!("https://github.com/jparga/{nombre_repo}.git"),
            privado,
            es_fork,
            archivado,
            rama_por_defecto: None,
            descripcion: None,
            tamano_kb: 0,
        }
    }

    #[test]
    fn guardar_y_leer_conserva_visibilidad_fork_y_archivado() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");

        almacen
            .guardar_origenes(
                &cuenta,
                &[
                    origen("publico", false, false, false),
                    origen("fork-viejo", false, true, true),
                    origen("secreto", true, false, false),
                ],
            )
            .expect("guardar");

        let leidos = almacen.origenes_de(&cuenta).expect("leer");
        let resumen: Vec<(&str, bool, bool, bool)> = leidos
            .iter()
            .map(|d| (d.id.nombre.as_str(), d.privado, d.es_fork, d.archivado))
            .collect();
        assert_eq!(
            resumen,
            vec![
                ("fork-viejo", false, true, true),
                ("publico", false, false, false),
                ("secreto", true, false, false),
            ]
        );
    }

    #[test]
    fn guardar_de_nuevo_actualiza_y_no_borra_los_que_faltan() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = nombre("jparga");
        almacen
            .guardar_origenes(
                &cuenta,
                &[
                    origen("cambia", false, false, false),
                    origen("sigue", true, false, false),
                ],
            )
            .expect("primer guardado");

        almacen
            .guardar_origenes(&cuenta, &[origen("cambia", true, false, true)])
            .expect("segundo guardado");

        let leidos = almacen.origenes_de(&cuenta).expect("leer");
        assert_eq!(leidos.len(), 2);
        assert!(leidos[0].privado && leidos[0].archivado);
        assert_eq!(leidos[1].id.nombre.as_str(), "sigue");
    }

    #[test]
    fn cada_cuenta_ve_solo_lo_suyo_y_borrar_lo_vacia() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let una = nombre("jparga");
        let otra = nombre("mereba-ci");
        almacen
            .guardar_origenes(&una, &[origen("repo", true, false, false)])
            .expect("guardar");

        assert!(almacen.origenes_de(&otra).expect("leer").is_empty());

        almacen.borrar_origenes_de(&una).expect("borrar");
        assert!(almacen.origenes_de(&una).expect("leer").is_empty());
    }
}
