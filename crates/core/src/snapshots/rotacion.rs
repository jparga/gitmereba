//! `rotar`: borra las capturas que sobran de un repo según una política de retención.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use time::{Duration, OffsetDateTime};

use crate::config::RutasCuenta;
use crate::git::{self, Opciones};
use crate::modelo::IdRepo;

use super::error::ErrorSnapshots;
use super::listado::{ResumenCaptura, listar};
use super::manifiesto::{DIR_MANIFIESTOS, Manifiesto, leer_manifiesto, validar_marca};
use super::rutas::ruta_bare_snapshot_existente;

/// Cuántas capturas conservar y durante cuánto tiempo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliticaRetencion {
    /// Las `conservar_n` capturas más recientes se conservan siempre.
    pub conservar_n: usize,
    /// Además, una captura por día de los últimos `conservar_dias` días.
    pub conservar_dias: u32,
}

impl Default for PoliticaRetencion {
    fn default() -> Self {
        Self {
            conservar_n: 10,
            conservar_dias: 30,
        }
    }
}

/// Decide, sin tocar el disco, qué marcas de `capturas` (se asume que vienen ordenadas
/// de más antigua a más reciente, como devuelve [`listar`]) hay que borrar: ni las
/// protegidas, ni las `conservar_n` más recientes, ni una por día dentro de los
/// últimos `conservar_dias` días a partir de `ahora` se borran nunca.
pub(super) fn decidir_borrado(
    capturas: &[ResumenCaptura],
    politica: &PoliticaRetencion,
    ahora: OffsetDateTime,
) -> Vec<String> {
    let mut conservar: BTreeSet<&str> = BTreeSet::new();

    for captura in capturas.iter().rev().take(politica.conservar_n) {
        conservar.insert(&captura.marca);
    }

    let limite = ahora - Duration::days(i64::from(politica.conservar_dias));
    let mut mas_reciente_del_dia: BTreeMap<time::Date, &ResumenCaptura> = BTreeMap::new();
    for captura in capturas {
        if captura.momento < limite {
            continue;
        }
        mas_reciente_del_dia
            .entry(captura.momento.date())
            .and_modify(|actual| {
                if captura.momento > actual.momento {
                    *actual = captura;
                }
            })
            .or_insert(captura);
    }
    for captura in mas_reciente_del_dia.values() {
        conservar.insert(&captura.marca);
    }

    for captura in capturas {
        if captura.protegida {
            conservar.insert(&captura.marca);
        }
    }

    capturas
        .iter()
        .filter(|captura| !conservar.contains(captura.marca.as_str()))
        .map(|captura| captura.marca.clone())
        .collect()
}

/// Borra del bare de snapshots las refs de la captura `manifiesto` (todo lo que hay
/// bajo `refs/snapshots/<marca>/...`).
async fn borrar_refs_de_captura(
    destino: &Path,
    manifiesto: &Manifiesto,
) -> Result<(), ErrorSnapshots> {
    let opciones = Opciones {
        directorio: Some(destino.to_path_buf()),
        ..Opciones::default()
    };
    for (nombre, _) in &manifiesto.refs {
        let namespaced = if let Some(rama) = nombre.strip_prefix("refs/heads/") {
            format!("refs/snapshots/{}/heads/{rama}", manifiesto.id)
        } else if let Some(tag) = nombre.strip_prefix("refs/tags/") {
            format!("refs/snapshots/{}/tags/{tag}", manifiesto.id)
        } else {
            continue;
        };
        crate::git::validar_ref(&namespaced)?;
        git::ejecutar(&["update-ref", "-d", &namespaced], &opciones).await?;
    }
    Ok(())
}

/// Aplica `politica` a las capturas de `id`: borra las que sobran (refs + manifiesto)
/// y devuelve las marcas borradas. No ejecuta ningún `gc`: eso es una decisión
/// explícita aparte, ver [`super::compactar`].
pub async fn rotar(
    rutas: &RutasCuenta,
    id: &IdRepo,
    politica: &PoliticaRetencion,
    ahora: OffsetDateTime,
) -> Result<Vec<String>, ErrorSnapshots> {
    let capturas = listar(rutas, id)?;
    let borrar = decidir_borrado(&capturas, politica, ahora);
    if borrar.is_empty() {
        return Ok(Vec::new());
    }

    let destino = ruta_bare_snapshot_existente(&rutas.snapshots(), id)?;
    let manifiestos_dir = destino.join(DIR_MANIFIESTOS);
    for marca in &borrar {
        validar_marca(marca)?;
        let ruta_manifiesto = manifiestos_dir.join(format!("{marca}.json"));
        let manifiesto = leer_manifiesto(&ruta_manifiesto)?;
        borrar_refs_de_captura(&destino, &manifiesto).await?;
        std::fs::remove_file(&ruta_manifiesto)
            .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    }
    Ok(borrar)
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::*;
    use crate::modelo::Nombre;

    fn momento(dia: u8, hora: u8) -> OffsetDateTime {
        let fecha = Date::from_calendar_date(2026, Month::September, dia).expect("fecha válida");
        let hora = Time::from_hms(hora, 0, 0).expect("hora válida");
        PrimitiveDateTime::new(fecha, hora).assume_utc()
    }

    fn resumen(marca: &str, dia: u8, hora: u8, protegida: bool) -> ResumenCaptura {
        ResumenCaptura {
            id: IdRepo {
                dueno: Nombre::nuevo("jparga").expect("válido"),
                nombre: Nombre::nuevo("repo1").expect("válido"),
            },
            marca: marca.to_string(),
            momento: momento(dia, hora),
            protegida,
            num_refs: 1,
        }
    }

    #[test]
    fn conserva_las_n_mas_recientes() {
        let capturas = vec![
            resumen("a", 1, 0, false),
            resumen("b", 2, 0, false),
            resumen("c", 3, 0, false),
        ];
        let politica = PoliticaRetencion {
            conservar_n: 2,
            conservar_dias: 0,
        };
        let borrado = decidir_borrado(&capturas, &politica, momento(3, 12));
        assert_eq!(borrado, vec!["a".to_string()]);
    }

    #[test]
    fn conserva_una_por_dia_dentro_de_la_ventana() {
        // Dos capturas el mismo día 1 (se queda con la más reciente de ese día) y una
        // en el día 2; con `conservar_n = 1` solo protegería la última por número, pero
        // la ventana de días añade la más reciente del día 1 también.
        let capturas = vec![
            resumen("a-mañana", 1, 8, false),
            resumen("b-tarde", 1, 20, false),
            resumen("c", 2, 8, false),
        ];
        let politica = PoliticaRetencion {
            conservar_n: 1,
            conservar_dias: 30,
        };
        let borrado = decidir_borrado(&capturas, &politica, momento(2, 12));
        assert_eq!(borrado, vec!["a-mañana".to_string()]);
    }

    #[test]
    fn una_captura_fuera_de_la_ventana_de_dias_se_borra_si_no_esta_entre_las_n_mas_recientes() {
        let capturas = vec![
            resumen("vieja", 1, 0, false),
            resumen("nueva", 20, 0, false),
        ];
        let politica = PoliticaRetencion {
            conservar_n: 1,
            conservar_dias: 5,
        };
        // `ahora` muy posterior: "vieja" queda fuera de la ventana de 5 días y no es
        // una de las `n=1` más recientes.
        let borrado = decidir_borrado(&capturas, &politica, momento(25, 0));
        assert_eq!(borrado, vec!["vieja".to_string()]);
    }

    #[test]
    fn nunca_borra_una_captura_protegida() {
        let capturas = vec![
            resumen("protegida", 1, 0, true),
            resumen("nueva", 20, 0, false),
        ];
        let politica = PoliticaRetencion {
            conservar_n: 1,
            conservar_dias: 0,
        };
        let borrado = decidir_borrado(&capturas, &politica, momento(25, 0));
        assert!(borrado.is_empty(), "{borrado:?}");
    }

    #[test]
    fn politica_por_defecto_es_10_y_30() {
        let politica = PoliticaRetencion::default();
        assert_eq!(politica.conservar_n, 10);
        assert_eq!(politica.conservar_dias, 30);
    }

    #[test]
    fn sin_capturas_no_hay_nada_que_borrar() {
        let politica = PoliticaRetencion::default();
        assert!(decidir_borrado(&[], &politica, momento(1, 0)).is_empty());
    }
}
