//! Detección de cambios destructivos entre dos capturas de un mismo repo.

use std::collections::BTreeMap;
use std::path::Path;

use crate::git;

use super::error::ErrorSnapshots;
use super::manifiesto::Manifiesto;

/// Un cambio entre dos capturas que pierde historia o hace inalcanzable un commit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tipo", rename_all = "kebab-case")]
pub enum CambioDestructivo {
    /// Una rama que existía ya no está.
    RamaBorrada { rama: String },
    /// Un tag que existía ya no está.
    TagBorrado { tag: String },
    /// Un tag apunta a otro commit (los tags no deberían moverse nunca).
    TagMovido {
        tag: String,
        antes: String,
        ahora: String,
    },
    /// El SHA de una rama cambió y el anterior no es antepasado del nuevo: no es un
    /// avance normal, sino un `reset` o un force-push que reescribe la historia.
    HistoriaReescrita {
        rama: String,
        antes: String,
        ahora: String,
    },
}

/// Compara el manifiesto `anterior` con las refs `actual` (misma forma que devuelve
/// [`crate::git::refs`]) y detecta cambios destructivos.
///
/// Función pura, sin E/S: `es_ancestro(antes, ahora)` decide si el SHA `antes` de una
/// rama es antepasado de su nuevo SHA `ahora`; quien la envuelve (ver
/// [`detectar_en_bare`]) es quien de verdad consulta git.
pub fn detectar_cambios_destructivos(
    anterior: &Manifiesto,
    actual: &[(String, String)],
    es_ancestro: impl Fn(&str, &str) -> bool,
) -> Vec<CambioDestructivo> {
    let actual: BTreeMap<&str, &str> = actual
        .iter()
        .map(|(nombre, sha)| (nombre.as_str(), sha.as_str()))
        .collect();

    let mut cambios = Vec::new();
    for (nombre, sha_antes) in &anterior.refs {
        match actual.get(nombre.as_str()) {
            None => {
                if let Some(rama) = nombre.strip_prefix("refs/heads/") {
                    cambios.push(CambioDestructivo::RamaBorrada {
                        rama: rama.to_string(),
                    });
                } else if let Some(tag) = nombre.strip_prefix("refs/tags/") {
                    cambios.push(CambioDestructivo::TagBorrado {
                        tag: tag.to_string(),
                    });
                }
            }
            Some(sha_ahora) if sha_ahora != sha_antes => {
                if let Some(rama) = nombre.strip_prefix("refs/heads/") {
                    if !es_ancestro(sha_antes, sha_ahora) {
                        cambios.push(CambioDestructivo::HistoriaReescrita {
                            rama: rama.to_string(),
                            antes: sha_antes.clone(),
                            ahora: sha_ahora.to_string(),
                        });
                    }
                } else if let Some(tag) = nombre.strip_prefix("refs/tags/") {
                    cambios.push(CambioDestructivo::TagMovido {
                        tag: tag.to_string(),
                        antes: sha_antes.clone(),
                        ahora: sha_ahora.to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    cambios
}

/// Envoltura async de [`detectar_cambios_destructivos`] que resuelve la ancestría con
/// `git merge-base --is-ancestor` sobre `bare`, un repositorio que debe contener los
/// objetos de ambas capturas (el propio bare de snapshots, que los conserva a todos
/// bajo `refs/snapshots/*`).
pub async fn detectar_en_bare(
    bare: &Path,
    anterior: &Manifiesto,
    actual: &[(String, String)],
) -> Result<Vec<CambioDestructivo>, ErrorSnapshots> {
    let actual_por_nombre: BTreeMap<&str, &str> = actual
        .iter()
        .map(|(nombre, sha)| (nombre.as_str(), sha.as_str()))
        .collect();

    let mut ancestria: BTreeMap<(String, String), bool> = BTreeMap::new();
    for (nombre, sha_antes) in &anterior.refs {
        if !nombre.starts_with("refs/heads/") {
            continue;
        }
        if let Some(sha_ahora) = actual_por_nombre.get(nombre.as_str())
            && sha_ahora != sha_antes
        {
            let clave = (sha_antes.clone(), sha_ahora.to_string());
            if let std::collections::btree_map::Entry::Vacant(entrada) = ancestria.entry(clave) {
                let es = git::es_ancestro(bare, sha_antes, sha_ahora).await?;
                entrada.insert(es);
            }
        }
    }

    Ok(detectar_cambios_destructivos(anterior, actual, |a, b| {
        ancestria
            .get(&(a.to_string(), b.to_string()))
            .copied()
            .unwrap_or(false)
    }))
}

/// Compara la captura `marca_anterior` de `id` con `refs_nuevas`, resolviendo la ruta del
/// bare de snapshots y leyendo el manifiesto con las mismas validaciones que el resto del
/// módulo (ruta canónica dentro de `snapshots/`, marca, tamaño y SHAs).
pub async fn detectar_desde_captura(
    rutas: &crate::config::RutasCuenta,
    id: &crate::modelo::IdRepo,
    marca_anterior: &str,
    refs_nuevas: &[(String, String)],
) -> Result<Vec<CambioDestructivo>, ErrorSnapshots> {
    super::manifiesto::validar_marca(marca_anterior)?;
    let bare = super::rutas::ruta_bare_snapshot_existente(&rutas.snapshots(), id)?;
    let ruta = bare
        .join(super::manifiesto::DIR_MANIFIESTOS)
        .join(format!("{marca_anterior}.json"));
    let anterior = super::manifiesto::leer_manifiesto(&ruta)?;
    detectar_en_bare(&bare, &anterior, refs_nuevas).await
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;

    fn manifiesto(refs: Vec<(&str, &str)>) -> Manifiesto {
        Manifiesto {
            momento: OffsetDateTime::UNIX_EPOCH,
            id: "20260101T000000Z".to_string(),
            refs: refs
                .into_iter()
                .map(|(n, s)| (n.to_string(), s.to_string()))
                .collect(),
            protegida: false,
        }
    }

    #[test]
    fn detecta_una_rama_borrada() {
        let anterior = manifiesto(vec![("refs/heads/main", "a")]);
        let cambios = detectar_cambios_destructivos(&anterior, &[], |_, _| true);
        assert_eq!(
            cambios,
            vec![CambioDestructivo::RamaBorrada {
                rama: "main".to_string()
            }]
        );
    }

    #[test]
    fn detecta_un_tag_borrado() {
        let anterior = manifiesto(vec![("refs/tags/v1", "a")]);
        let cambios = detectar_cambios_destructivos(&anterior, &[], |_, _| true);
        assert_eq!(
            cambios,
            vec![CambioDestructivo::TagBorrado {
                tag: "v1".to_string()
            }]
        );
    }

    #[test]
    fn detecta_un_tag_movido_aunque_sea_ancestro() {
        let anterior = manifiesto(vec![("refs/tags/v1", "a")]);
        let actual = vec![("refs/tags/v1".to_string(), "b".to_string())];
        // Los tags nunca deberían moverse, así que se marca como destructivo incluso
        // si `es_ancestro` diría que sí (avance «normal» según ese criterio).
        let cambios = detectar_cambios_destructivos(&anterior, &actual, |_, _| true);
        assert_eq!(
            cambios,
            vec![CambioDestructivo::TagMovido {
                tag: "v1".to_string(),
                antes: "a".to_string(),
                ahora: "b".to_string(),
            }]
        );
    }

    #[test]
    fn un_avance_normal_de_rama_no_es_destructivo() {
        let anterior = manifiesto(vec![("refs/heads/main", "a")]);
        let actual = vec![("refs/heads/main".to_string(), "b".to_string())];
        let cambios = detectar_cambios_destructivos(&anterior, &actual, |_, _| true);
        assert!(cambios.is_empty());
    }

    #[test]
    fn una_rama_reescrita_es_destructiva() {
        let anterior = manifiesto(vec![("refs/heads/main", "a")]);
        let actual = vec![("refs/heads/main".to_string(), "b".to_string())];
        let cambios = detectar_cambios_destructivos(&anterior, &actual, |_, _| false);
        assert_eq!(
            cambios,
            vec![CambioDestructivo::HistoriaReescrita {
                rama: "main".to_string(),
                antes: "a".to_string(),
                ahora: "b".to_string(),
            }]
        );
    }

    #[test]
    fn una_rama_sin_cambios_no_es_destructiva() {
        let anterior = manifiesto(vec![("refs/heads/main", "a")]);
        let actual = vec![("refs/heads/main".to_string(), "a".to_string())];
        let cambios = detectar_cambios_destructivos(&anterior, &actual, |_, _| false);
        assert!(cambios.is_empty());
    }

    #[test]
    fn varios_cambios_a_la_vez() {
        let anterior = manifiesto(vec![
            ("refs/heads/main", "a"),
            ("refs/heads/dev", "d"),
            ("refs/tags/v1", "t"),
        ]);
        let actual = vec![
            ("refs/heads/main".to_string(), "a2".to_string()),
            ("refs/tags/v1".to_string(), "t".to_string()),
        ];
        let cambios = detectar_cambios_destructivos(&anterior, &actual, |_, _| false);
        assert_eq!(cambios.len(), 2);
        assert!(cambios.contains(&CambioDestructivo::RamaBorrada {
            rama: "dev".to_string()
        }));
        assert!(cambios.contains(&CambioDestructivo::HistoriaReescrita {
            rama: "main".to_string(),
            antes: "a".to_string(),
            ahora: "a2".to_string(),
        }));
    }
}
