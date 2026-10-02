//! Estado persistido de avisos de una cuenta: qué ya se ha avisado, para no repetir
//! notificaciones ni perder el hilo entre pasadas.
//!
//! Vive en `avisos-<login>.json`, bajo `Rutas::directorio_datos()`, con permisos 0600 y
//! escritura atómica (fichero temporal + `rename`), igual que el resto de ficheros de
//! configuración de la app (`config::fichero`, `instancia::fichero`).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::modelo::{IdRepo, Nombre};

use super::error::ErrorAvisos;
use super::modelo::TipoFallo;

const MODO_DIRECTORIO: u32 = 0o700;
const MODO_FICHERO: u32 = 0o600;
/// Tamaño máximo, en bytes, que se lee del fichero de estado. Por encima de esto se trata
/// como si estuviera corrupto (se empieza de cero) en vez de intentar `parse` un fichero
/// arbitrariamente grande.
const TAMANO_MAXIMO_FICHERO: u64 = 1_000_000;

/// Un fallo de un repo que ya se conoce, para decidir si reavisar (transcurridas 24 h) o
/// si el repo se ha recuperado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaFallo {
    pub id: IdRepo,
    pub tipo: TipoFallo,
    #[serde(with = "time::serde::rfc3339")]
    pub primera_vez: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub ultimo_aviso: OffsetDateTime,
}

/// Estado persistido de los avisos de una cuenta.
///
/// Los fallos se guardan como `Vec`, no como `BTreeMap<IdRepo, _>`: una clave compuesta
/// por [`IdRepo`] no serializa como clave de objeto JSON (`serde_json` exige que las
/// claves de un mapa serialicen a una cadena). El número de repos por cuenta es pequeño,
/// así que el coste de una búsqueda lineal en [`super::decidir`] es insignificante.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EstadoAvisos {
    #[serde(default)]
    pub fallos: Vec<EntradaFallo>,
    /// Último momento en que se avisó de la caducidad (próxima o ya ocurrida) del token.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub ultimo_aviso_token: Option<OffsetDateTime>,
    /// Último momento en que se avisó de que Gitea no respondía.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub ultimo_aviso_gitea_parado: Option<OffsetDateTime>,
}

/// Nombre del fichero de estado de avisos de `login`.
pub fn nombre_fichero_estado(login: &Nombre) -> String {
    format!("avisos-{login}.json")
}

/// Ruta del fichero de estado de avisos de `login`, bajo `directorio_datos`
/// (`Rutas::directorio_datos()`).
pub fn ruta_estado(directorio_datos: &Path, login: &Nombre) -> PathBuf {
    directorio_datos.join(nombre_fichero_estado(login))
}

/// Carga el estado de avisos de `ruta`. Nunca falla: si el fichero no existe, supera el
/// tamaño máximo o no se puede parsear (corrupto), se empieza de cero.
pub fn cargar_estado(ruta: &Path) -> EstadoAvisos {
    let metadata = match fs::metadata(ruta) {
        Ok(metadata) => metadata,
        Err(_) => return EstadoAvisos::default(),
    };
    if metadata.len() > TAMANO_MAXIMO_FICHERO {
        return EstadoAvisos::default();
    }
    match fs::read_to_string(ruta) {
        Ok(contenido) => serde_json::from_str(&contenido).unwrap_or_default(),
        Err(_) => EstadoAvisos::default(),
    }
}

/// Guarda `estado` en `ruta` de forma atómica (fichero temporal en el mismo directorio +
/// `rename`) con permisos 0600. Crea el directorio padre (0700) si falta.
pub fn guardar_estado(ruta: &Path, estado: &EstadoAvisos) -> Result<(), ErrorAvisos> {
    let contenido =
        serde_json::to_vec_pretty(estado).map_err(|error| ErrorAvisos::Io(error.to_string()))?;

    if let Some(directorio) = ruta.parent()
        && !directorio.as_os_str().is_empty()
        && !directorio.exists()
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(MODO_DIRECTORIO)
            .create(directorio)
            .map_err(|error| ErrorAvisos::Io(error.to_string()))?;
    }

    let temporal = fichero_temporal(ruta);
    escribir_fichero_nuevo(&temporal, &contenido)?;
    fs::rename(&temporal, ruta).map_err(|error| ErrorAvisos::Io(error.to_string()))?;
    Ok(())
}

fn fichero_temporal(ruta: &Path) -> PathBuf {
    let mut nombre = ruta.as_os_str().to_owned();
    nombre.push(".tmp");
    PathBuf::from(nombre)
}

fn escribir_fichero_nuevo(ruta: &Path, contenido: &[u8]) -> Result<(), ErrorAvisos> {
    let mut fichero = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(MODO_FICHERO)
        .open(ruta)
        .map_err(|error| ErrorAvisos::Io(error.to_string()))?;
    fichero
        .write_all(contenido)
        .map_err(|error| ErrorAvisos::Io(error.to_string()))?;
    fichero
        .sync_all()
        .map_err(|error| ErrorAvisos::Io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::Nombre;
    use std::os::unix::fs::PermissionsExt;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn fallo(
        id_dueno: &str,
        id_nombre: &str,
        tipo: TipoFallo,
        momento: OffsetDateTime,
    ) -> EntradaFallo {
        EntradaFallo {
            id: IdRepo {
                dueno: nombre(id_dueno),
                nombre: nombre(id_nombre),
            },
            tipo,
            primera_vez: momento,
            ultimo_aviso: momento,
        }
    }

    #[test]
    fn nombre_de_fichero_incluye_el_login() {
        assert_eq!(
            nombre_fichero_estado(&nombre("jparga")),
            "avisos-jparga.json"
        );
    }

    #[test]
    fn cargar_un_fichero_inexistente_devuelve_el_valor_por_defecto() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");
        assert_eq!(cargar_estado(&ruta), EstadoAvisos::default());
    }

    #[test]
    fn ida_y_vuelta_conserva_los_fallos_y_las_fechas() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");
        let momento = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("fecha válida");
        let mut estado = EstadoAvisos::default();
        estado
            .fallos
            .push(fallo("jparga", "repo1", TipoFallo::Sincronizacion, momento));
        estado.ultimo_aviso_token = Some(momento);
        estado.ultimo_aviso_gitea_parado = Some(momento);

        guardar_estado(&ruta, &estado).expect("guardar no falla");
        let releido = cargar_estado(&ruta);

        assert_eq!(releido, estado);
    }

    #[test]
    fn guarda_con_permisos_0600() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");

        guardar_estado(&ruta, &EstadoAvisos::default()).expect("guardar no falla");

        let permisos = fs::metadata(&ruta).expect("metadata").permissions();
        assert_eq!(permisos.mode() & 0o777, 0o600);
    }

    #[test]
    fn crea_el_directorio_padre_con_permisos_0700() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("datos").join("avisos-jparga.json");

        guardar_estado(&ruta, &EstadoAvisos::default()).expect("guardar no falla");

        let permisos = fs::metadata(ruta.parent().expect("padre"))
            .expect("metadata")
            .permissions();
        assert_eq!(permisos.mode() & 0o777, 0o700);
    }

    #[test]
    fn no_deja_ficheros_temporales_tras_guardar() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");

        guardar_estado(&ruta, &EstadoAvisos::default()).expect("guardar no falla");

        let temporales: Vec<_> = fs::read_dir(directorio.path())
            .expect("leer directorio")
            .filter_map(Result::ok)
            .filter(|entrada| entrada.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(temporales.is_empty(), "{temporales:?}");
    }

    #[test]
    fn un_fichero_corrupto_se_trata_como_vacio_sin_fallar() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");
        fs::write(&ruta, b"esto no es json valido {{{").expect("escribir corrupto");

        assert_eq!(cargar_estado(&ruta), EstadoAvisos::default());
    }

    #[test]
    fn un_fichero_demasiado_grande_se_trata_como_vacio_sin_fallar() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");
        // JSON válido pero por encima del tamaño máximo permitido.
        let momento = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("fecha válida");
        let mut estado = EstadoAvisos::default();
        for n in 0..40_000 {
            estado.fallos.push(fallo(
                "jparga",
                &format!("repo-{n}"),
                TipoFallo::Sincronizacion,
                momento,
            ));
        }
        let contenido = serde_json::to_vec(&estado).expect("serializar");
        assert!(contenido.len() as u64 > TAMANO_MAXIMO_FICHERO);
        fs::write(&ruta, &contenido).expect("escribir fichero grande");

        assert_eq!(cargar_estado(&ruta), EstadoAvisos::default());
    }

    #[test]
    fn una_segunda_escritura_reemplaza_el_contenido() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("avisos-jparga.json");
        let momento = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("fecha válida");

        guardar_estado(&ruta, &EstadoAvisos::default()).expect("primera escritura");
        let segundo = EstadoAvisos {
            ultimo_aviso_gitea_parado: Some(momento),
            ..EstadoAvisos::default()
        };
        guardar_estado(&ruta, &segundo).expect("segunda escritura");

        assert_eq!(cargar_estado(&ruta), segundo);
    }
}
