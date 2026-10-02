//! El manifiesto de una captura: qué refs tenía el repo y cuándo. Se trata como no
//! fiable al leerlo del disco (tamaño máximo, formato de marca y de SHA validados).

use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::error::ErrorSnapshots;
use super::fichero::escribir_privado;

/// Subcarpeta del bare de snapshots donde viven los manifiestos JSON.
pub(super) const DIR_MANIFIESTOS: &str = "gitmereba-snapshots";
/// Tamaño máximo admitido de un manifiesto en disco: no fiarse de lo que hay ahí.
const TAMANO_MAXIMO_MANIFIESTO: u64 = 5 * 1024 * 1024;

/// Manifiesto de una captura: las refs del repo en el momento de capturarla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifiesto {
    /// Instante de la captura, en UTC.
    #[serde(with = "time::serde::rfc3339")]
    pub momento: OffsetDateTime,
    /// Marca de tiempo con la que se nombra el espacio `refs/snapshots/<id>/...`.
    pub id: String,
    /// Refs capturadas tal cual estaban en el origen: `(nombre completo, sha)`.
    pub refs: Vec<(String, String)>,
    /// Si `true`, `rotar` nunca la borra hasta que el usuario llame a `liberar`.
    #[serde(default)]
    pub protegida: bool,
}

/// Valida que `marca` tiene la forma fija `AAAAMMDDTHHMMSSZ` (16 caracteres ASCII).
///
/// Al exigir esta forma exacta se descartan de paso separadores de ruta, `..` y
/// cualquier otro intento de inyección: solo dígitos, una `T` y una `Z` en posiciones
/// fijas pueden pasar.
pub(super) fn validar_marca(marca: &str) -> Result<(), ErrorSnapshots> {
    let bytes = marca.as_bytes();
    let formato_valido = bytes.len() == 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'Z';
    if formato_valido {
        Ok(())
    } else {
        Err(ErrorSnapshots::MarcaInvalida(marca.to_string()))
    }
}

/// Genera la marca de tiempo de `momento` (convertido a UTC) en formato fijo.
pub(super) fn generar_marca(momento: OffsetDateTime) -> String {
    let momento = momento.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        momento.year(),
        u8::from(momento.month()),
        momento.day(),
        momento.hour(),
        momento.minute(),
        momento.second()
    )
}

/// Genera una marca para `ahora` que no choque con ninguna de `existentes`.
///
/// La marca solo tiene resolución de segundo: dos capturas en el mismo segundo (dos
/// sincronizaciones muy seguidas, o simplemente unas pruebas rápidas) generarían la
/// misma marca. Como el fetch de una captura usa refspecs con `+` sobre un espacio de
/// refs que se asume propio y nuevo, reutilizar una marca ya usada sobrescribiría en
/// silencio la captura anterior con ese nombre —justo lo que este módulo existe para
/// evitar—, así que si hay colisión se prueba con el segundo siguiente hasta encontrar
/// una marca libre.
pub(super) fn generar_marca_unica(existentes: &[Manifiesto], ahora: OffsetDateTime) -> String {
    let usadas: std::collections::BTreeSet<&str> =
        existentes.iter().map(|m| m.id.as_str()).collect();
    let mut candidato = ahora;
    loop {
        let marca = generar_marca(candidato);
        if !usadas.contains(marca.as_str()) {
            return marca;
        }
        candidato += time::Duration::SECOND;
    }
}

/// Valida que `sha` es un SHA-1 (40) o SHA-256 (64) en hexadecimal.
fn validar_sha(sha: &str) -> Result<(), ErrorSnapshots> {
    let longitud_valida = sha.len() == 40 || sha.len() == 64;
    if longitud_valida && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ErrorSnapshots::ShaInvalido)
    }
}

/// Valida un nombre de ref del manifiesto: debe ser una rama o un tag y pasar las
/// mismas reglas que cualquier referencia que vaya a usarse con `git`.
fn validar_nombre_ref(nombre: &str) -> Result<(), ErrorSnapshots> {
    if !nombre.starts_with("refs/heads/") && !nombre.starts_with("refs/tags/") {
        return Err(ErrorSnapshots::MarcaInvalida(nombre.to_string()));
    }
    crate::git::validar_ref(nombre)?;
    Ok(())
}

/// Lee y valida un manifiesto de `ruta`. Nunca confía en el contenido: comprueba el
/// tamaño antes de leerlo entero, y valida la marca, los nombres de ref y los SHA.
pub(super) fn leer_manifiesto(ruta: &Path) -> Result<Manifiesto, ErrorSnapshots> {
    let metadatos =
        std::fs::symlink_metadata(ruta).map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    if metadatos.len() > TAMANO_MAXIMO_MANIFIESTO {
        return Err(ErrorSnapshots::ManifiestoDemasiadoGrande(
            metadatos.len() as usize
        ));
    }
    let contenido = std::fs::read(ruta).map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    let manifiesto: Manifiesto = serde_json::from_slice(&contenido)
        .map_err(|error| ErrorSnapshots::Json(error.to_string()))?;

    validar_marca(&manifiesto.id)?;
    for (nombre, sha) in &manifiesto.refs {
        validar_nombre_ref(nombre)?;
        validar_sha(sha)?;
    }
    Ok(manifiesto)
}

/// Escribe `manifiesto` de forma atómica y con permisos 0600 en `ruta`.
pub(super) fn escribir_manifiesto(
    ruta: &Path,
    manifiesto: &Manifiesto,
) -> Result<(), ErrorSnapshots> {
    let contenido =
        serde_json::to_vec(manifiesto).map_err(|error| ErrorSnapshots::Json(error.to_string()))?;
    escribir_privado(ruta, &contenido)
}

/// Lee todos los manifiestos válidos de `directorio`, ordenados de más antiguo a más
/// reciente (el nombre de fichero, `<marca>.json`, ya ordena así). Si `directorio` no
/// existe se interpreta como «sin capturas todavía», no como un error. No sigue
/// enlaces simbólicos: solo lee ficheros normales.
pub(super) fn leer_todos(directorio: &Path) -> Result<Vec<Manifiesto>, ErrorSnapshots> {
    let entradas = match std::fs::read_dir(directorio) {
        Ok(entradas) => entradas,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ErrorSnapshots::Io(error.to_string())),
    };

    let mut manifiestos = Vec::new();
    for entrada in entradas {
        let entrada = entrada.map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
        let tipo = entrada
            .file_type()
            .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
        if !tipo.is_file() {
            // Ni subdirectorios ni enlaces simbólicos: `file_type()` de `read_dir` no
            // sigue enlaces, así que un símlink nunca pasa `is_file()`.
            continue;
        }
        let ruta = entrada.path();
        if ruta.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        manifiestos.push(leer_manifiesto(&ruta)?);
    }
    manifiestos.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(manifiestos)
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    use super::*;

    /// Construye una fecha-hora UTC de prueba sin depender de la feature `macros` de
    /// `time` (no está habilitada en este workspace).
    fn fecha_utc(
        anio: i32,
        mes: Month,
        dia: u8,
        hora: u8,
        minuto: u8,
        segundo: u8,
    ) -> OffsetDateTime {
        let fecha = Date::from_calendar_date(anio, mes, dia).expect("fecha válida");
        let hora = Time::from_hms(hora, minuto, segundo).expect("hora válida");
        PrimitiveDateTime::new(fecha, hora).assume_utc()
    }

    #[test]
    fn genera_la_marca_en_el_formato_fijo() {
        let momento = fecha_utc(2026, Month::September, 19, 8, 5, 3);
        assert_eq!(generar_marca(momento), "20260919T080503Z");
    }

    #[test]
    fn genera_la_marca_convirtiendo_a_utc() {
        let fecha = Date::from_calendar_date(2026, Month::September, 19).expect("fecha válida");
        let hora = Time::from_hms(10, 5, 3).expect("hora válida");
        let con_offset = PrimitiveDateTime::new(fecha, hora)
            .assume_offset(UtcOffset::from_hms(2, 0, 0).expect("offset válido"));
        assert_eq!(generar_marca(con_offset), "20260919T080503Z");
    }

    #[test]
    fn genera_marca_unica_sin_colision_devuelve_la_marca_del_instante() {
        let momento = fecha_utc(2026, Month::September, 19, 8, 5, 3);
        assert_eq!(generar_marca_unica(&[], momento), "20260919T080503Z");
    }

    #[test]
    fn genera_marca_unica_avanza_un_segundo_si_ya_esta_usada() {
        let momento = fecha_utc(2026, Month::September, 19, 8, 5, 3);
        let existente = Manifiesto {
            id: "20260919T080503Z".to_string(),
            ..manifiesto_de_prueba()
        };
        assert_eq!(
            generar_marca_unica(&[existente], momento),
            "20260919T080504Z"
        );
    }

    #[test]
    fn genera_marca_unica_avanza_hasta_encontrar_hueco() {
        let momento = fecha_utc(2026, Month::September, 19, 8, 5, 3);
        let existentes = vec![
            Manifiesto {
                id: "20260919T080503Z".to_string(),
                ..manifiesto_de_prueba()
            },
            Manifiesto {
                id: "20260919T080504Z".to_string(),
                ..manifiesto_de_prueba()
            },
        ];
        assert_eq!(
            generar_marca_unica(&existentes, momento),
            "20260919T080505Z"
        );
    }

    #[test]
    fn valida_marcas_correctas() {
        assert!(validar_marca("20260919T080503Z").is_ok());
    }

    #[test]
    fn rechaza_marcas_con_forma_incorrecta() {
        for invalida in [
            "",
            "2026-09-19T08:05:03Z",
            "20260919T080503",
            "../../etc/passwd",
            "20260919T08050bZ",
            "20260919T080503Zx",
        ] {
            assert!(validar_marca(invalida).is_err(), "{invalida}");
        }
    }

    #[test]
    fn valida_shas_de_40_y_64_caracteres_hex() {
        assert!(validar_sha(&"a".repeat(40)).is_ok());
        assert!(validar_sha(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn rechaza_shas_con_longitud_o_caracteres_invalidos() {
        assert!(validar_sha(&"a".repeat(39)).is_err());
        assert!(validar_sha(&"g".repeat(40)).is_err());
        assert!(validar_sha("").is_err());
    }

    fn manifiesto_de_prueba() -> Manifiesto {
        Manifiesto {
            momento: fecha_utc(2026, Month::September, 19, 8, 5, 3),
            id: "20260919T080503Z".to_string(),
            refs: vec![("refs/heads/main".to_string(), "a".repeat(40))],
            protegida: false,
        }
    }

    #[test]
    fn escribe_y_relee_un_manifiesto() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("20260919T080503Z.json");
        let manifiesto = manifiesto_de_prueba();

        escribir_manifiesto(&ruta, &manifiesto).expect("escribir_manifiesto no falla");
        let releido = leer_manifiesto(&ruta).expect("leer_manifiesto no falla");

        assert_eq!(releido, manifiesto);
        use std::os::unix::fs::PermissionsExt;
        let permisos = std::fs::metadata(&ruta).expect("metadata").permissions();
        assert_eq!(permisos.mode() & 0o777, 0o600);
    }

    #[test]
    fn rechaza_un_manifiesto_corrupto() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("x.json");
        std::fs::write(&ruta, b"esto no es json").expect("escribir fichero corrupto");

        assert!(matches!(
            leer_manifiesto(&ruta),
            Err(ErrorSnapshots::Json(_))
        ));
    }

    #[test]
    fn rechaza_un_manifiesto_demasiado_grande() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("x.json");
        let enorme = vec![b'a'; (TAMANO_MAXIMO_MANIFIESTO + 1) as usize];
        std::fs::write(&ruta, &enorme).expect("escribir fichero enorme");

        assert!(matches!(
            leer_manifiesto(&ruta),
            Err(ErrorSnapshots::ManifiestoDemasiadoGrande(_))
        ));
    }

    #[test]
    fn rechaza_un_manifiesto_con_marca_invalida() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("x.json");
        let mut manifiesto = manifiesto_de_prueba();
        manifiesto.id = "no-es-una-marca".to_string();
        let contenido = serde_json::to_vec(&manifiesto).expect("serializar");
        std::fs::write(&ruta, contenido).expect("escribir fichero");

        assert!(matches!(
            leer_manifiesto(&ruta),
            Err(ErrorSnapshots::MarcaInvalida(_))
        ));
    }

    #[test]
    fn rechaza_un_manifiesto_con_sha_invalido() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("x.json");
        let mut manifiesto = manifiesto_de_prueba();
        manifiesto.refs = vec![("refs/heads/main".to_string(), "no-es-un-sha".to_string())];
        let contenido = serde_json::to_vec(&manifiesto).expect("serializar");
        std::fs::write(&ruta, contenido).expect("escribir fichero");

        assert!(matches!(
            leer_manifiesto(&ruta),
            Err(ErrorSnapshots::ShaInvalido)
        ));
    }

    #[test]
    fn leer_todos_de_un_directorio_inexistente_da_lista_vacia() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let inexistente = directorio.path().join("no-existe");
        assert_eq!(leer_todos(&inexistente).expect("no falla"), Vec::new());
    }

    #[test]
    fn leer_todos_ordena_por_marca_e_ignora_enlaces_simbolicos() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let primero = Manifiesto {
            id: "20260101T000000Z".to_string(),
            ..manifiesto_de_prueba()
        };
        let segundo = Manifiesto {
            id: "20260919T080503Z".to_string(),
            ..manifiesto_de_prueba()
        };
        escribir_manifiesto(
            &directorio.path().join(format!("{}.json", segundo.id)),
            &segundo,
        )
        .expect("escribir segundo");
        escribir_manifiesto(
            &directorio.path().join(format!("{}.json", primero.id)),
            &primero,
        )
        .expect("escribir primero");

        // Enlace simbólico a un manifiesto válido: debe ignorarse igualmente.
        std::os::unix::fs::symlink(
            directorio.path().join(format!("{}.json", primero.id)),
            directorio.path().join("enlace.json"),
        )
        .expect("crear enlace simbólico");

        let leidos = leer_todos(directorio.path()).expect("leer_todos no falla");
        assert_eq!(leidos, vec![primero, segundo]);
    }
}
