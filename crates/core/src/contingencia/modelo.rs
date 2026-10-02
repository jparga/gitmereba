//! Tipos de datos de `contingencia`, todos serializables: el llamador (fuera de este
//! módulo, que no tiene almacén propio) es quien guarda [`PuntoDePartida`] y el resto de
//! informes entre pasos.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::git::{self, ErrorGit};
use crate::modelo::IdRepo;

/// SHA de cada referencia del mirror en el instante en que se activó la contingencia.
/// Es la base contra la que [`super::estado`] y
/// [`super::reconciliar`] miden los commits de más.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuntoDePartida {
    pub refs: BTreeMap<String, String>,
}

impl PuntoDePartida {
    /// Anota el SHA de todas las referencias del bare en `ruta` (sin red: un `git
    /// for-each-ref` local).
    pub(crate) async fn leer(ruta: &Path) -> Result<Self, ErrorGit> {
        let refs = git::refs(ruta).await?;
        Ok(Self {
            refs: refs.into_iter().collect(),
        })
    }

    /// SHA de `referencia` (p. ej. `refs/heads/main`) en el punto de partida.
    pub fn sha_de(&self, referencia: &str) -> Option<&str> {
        self.refs.get(referencia).map(String::as_str)
    }
}

/// Resultado de [`super::activar`] para un repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoContingencia {
    pub original: IdRepo,
    pub contingencia: IdRepo,
    /// URL de clonado del repo de contingencia, sin credenciales.
    pub url_remoto: String,
    /// Comando listo para pegar en una terminal.
    pub comando_remote: String,
    pub punto_de_partida: PuntoDePartida,
    pub advertencias: Vec<String>,
}

/// Situación de una rama del repo de contingencia frente al punto de partida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoRama {
    pub rama: String,
    pub commits_de_mas: u64,
    pub es_nueva: bool,
}

/// Resultado de [`super::estado`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoContingencia {
    pub ramas: Vec<EstadoRama>,
    /// Ramas que estaban en el punto de partida y ya no existen en el repo de
    /// contingencia (nunca se borran en GitHub por esto: solo se informa).
    pub ramas_borradas: Vec<String>,
}

impl EstadoContingencia {
    pub fn hay_commits_de_mas(&self) -> bool {
        self.ramas.iter().any(|rama| rama.commits_de_mas > 0)
    }
}

/// Resultado de [`super::cerrar`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultadoCierre {
    pub original: IdRepo,
    pub contingencia: IdRepo,
    /// Siempre `true`: `cerrar` nunca borra el repo de contingencia (la API de Gitea no
    /// permite archivarlo); usa [`super::borrar_contingencia`] aparte.
    pub pendiente_de_borrar: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_contingencia_por_defecto_no_tiene_commits_de_mas() {
        assert!(!EstadoContingencia::default().hay_commits_de_mas());
    }

    #[test]
    fn hay_commits_de_mas_es_cierto_si_alguna_rama_tiene_alguno() {
        let estado = EstadoContingencia {
            ramas: vec![
                EstadoRama {
                    rama: "main".to_string(),
                    commits_de_mas: 0,
                    es_nueva: false,
                },
                EstadoRama {
                    rama: "dev".to_string(),
                    commits_de_mas: 2,
                    es_nueva: false,
                },
            ],
            ramas_borradas: vec![],
        };
        assert!(estado.hay_commits_de_mas());
    }

    #[test]
    fn punto_de_partida_sha_de_devuelve_none_si_no_esta() {
        let punto = PuntoDePartida::default();
        assert_eq!(punto.sha_de("refs/heads/main"), None);
    }
}
