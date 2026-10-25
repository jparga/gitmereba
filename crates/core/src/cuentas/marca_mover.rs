//! Marca de «movimiento de carpeta en curso».
//!
//! Antes de parar Gitea, `mover_carpeta` escribe `<directorio_datos>/mover-en-curso-<login>.toml`
//! con el origen, el destino y el modo. La borra tras confirmar el movimiento o tras
//! deshacerlo del todo. Si el proceso muere a mitad, la marca queda: `doctor` la avisa con
//! instrucciones y `mover_carpeta` se niega a empezar otro movimiento. No hay recuperación
//! automática: decidir qué carpeta es la buena es cosa de la persona.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Rutas;
use crate::modelo::Nombre;

use super::error::ErrorCuentas;

const PREFIJO: &str = "mover-en-curso-";

/// Contenido de la marca.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarcaMover {
    pub login: String,
    pub origen: PathBuf,
    pub destino: PathBuf,
    /// `"pendiente"` hasta saber si se renombra o se copia; luego `"renombrado"` o `"copia"`.
    pub modo: String,
}

fn ruta(rutas: &Rutas, login: &str) -> PathBuf {
    rutas
        .directorio_datos()
        .join(format!("{PREFIJO}{login}.toml"))
}

fn io(error: &std::io::Error) -> ErrorCuentas {
    ErrorCuentas::Io(error.to_string())
}

/// Escribe (o reescribe) la marca, de forma atómica y con permisos 0600.
pub(super) fn escribir_marca(rutas: &Rutas, marca: &MarcaMover) -> Result<(), ErrorCuentas> {
    let texto =
        toml::to_string_pretty(marca).map_err(|error| ErrorCuentas::Interno(error.to_string()))?;
    std::fs::create_dir_all(rutas.directorio_datos()).map_err(|e| io(&e))?;
    let definitiva = ruta(rutas, &marca.login);
    let temporal = definitiva.with_extension("toml.tmp");
    let mut fichero = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporal)
        .map_err(|e| io(&e))?;
    fichero
        .write_all(texto.as_bytes())
        .and_then(|()| fichero.sync_all())
        .map_err(|e| io(&e))?;
    std::fs::rename(&temporal, &definitiva).map_err(|e| io(&e))
}

/// La marca de `login`, si existe.
pub fn leer_marca(rutas: &Rutas, login: &Nombre) -> Result<Option<MarcaMover>, ErrorCuentas> {
    match std::fs::read_to_string(ruta(rutas, login.as_str())) {
        Ok(texto) => toml::from_str(&texto).map(Some).map_err(|error| {
            ErrorCuentas::Interno(format!("marca de movimiento ilegible: {error}"))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(&error)),
    }
}

/// Borra la marca de `login`. Que no exista no es un error.
pub(super) fn borrar_marca(rutas: &Rutas, login: &Nombre) -> Result<(), ErrorCuentas> {
    match std::fs::remove_file(ruta(rutas, login.as_str())) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io(&error)),
        _ => Ok(()),
    }
}

/// Todas las marcas que hay en el directorio de datos (las de movimientos interrumpidos).
/// Una marca ilegible se devuelve como `Err` en su posición.
pub fn marcas_pendientes(rutas: &Rutas) -> Vec<Result<MarcaMover, ErrorCuentas>> {
    let Ok(entradas) = std::fs::read_dir(rutas.directorio_datos()) else {
        return Vec::new();
    };
    let mut nombres: Vec<String> = entradas
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(PREFIJO) && n.ends_with(".toml"))
        .collect();
    nombres.sort();
    nombres
        .into_iter()
        .map(|nombre| {
            let texto = std::fs::read_to_string(rutas.directorio_datos().join(&nombre))
                .map_err(|e| io(&e))?;
            toml::from_str(&texto).map_err(|error| {
                ErrorCuentas::Interno(format!("marca «{nombre}» ilegible: {error}"))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn marca(login: &str) -> MarcaMover {
        MarcaMover {
            login: login.to_string(),
            origen: PathBuf::from("/a/origen"),
            destino: PathBuf::from("/b/destino"),
            modo: "pendiente".to_string(),
        }
    }

    #[test]
    fn escribir_leer_actualizar_y_borrar() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        assert_eq!(leer_marca(&rutas, &nombre("ana")).expect("leer"), None);

        escribir_marca(&rutas, &marca("ana")).expect("escribir");
        let fichero = rutas.directorio_datos().join("mover-en-curso-ana.toml");
        assert_eq!(
            std::fs::metadata(&fichero)
                .expect("metadatos")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            leer_marca(&rutas, &nombre("ana")).expect("leer"),
            Some(marca("ana"))
        );

        let mut copia = marca("ana");
        copia.modo = "copia".to_string();
        escribir_marca(&rutas, &copia).expect("reescribir");
        assert_eq!(
            leer_marca(&rutas, &nombre("ana")).expect("leer"),
            Some(copia)
        );

        borrar_marca(&rutas, &nombre("ana")).expect("borrar");
        borrar_marca(&rutas, &nombre("ana")).expect("borrar otra vez no falla");
        assert_eq!(leer_marca(&rutas, &nombre("ana")).expect("leer"), None);
    }

    #[test]
    fn pendientes_lista_todas_las_marcas() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        assert!(marcas_pendientes(&rutas).is_empty());
        escribir_marca(&rutas, &marca("luis")).expect("escribir");
        escribir_marca(&rutas, &marca("ana")).expect("escribir");
        let logins: Vec<String> = marcas_pendientes(&rutas)
            .into_iter()
            .map(|m| m.expect("legible").login)
            .collect();
        assert_eq!(logins, ["ana", "luis"]);
    }
}
