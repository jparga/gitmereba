//! `reconciliar`: envía a GitHub los commits de más del repo de contingencia, rama a
//! rama, sin forzar nunca nada. Es la operación más delicada del módulo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::RutasCuenta;
use crate::git::{self, Credencial, ErrorGit, Opciones, validar_ref};
use crate::idioma::{Idioma, Localizable};
use crate::modelo::IdRepo;
use crate::secretos::Secreto;

use super::decidir::{
    Decision, EntradaDecision, argumentos_push, decidir, refspec_rama, refspec_tag,
};
use super::error::ErrorContingencia;
use super::modelo::PuntoDePartida;
use super::nombres::org_contingencia;
use super::rutas::ruta_bare_validada;
use super::temporal::DirTemporal;

const PREFIJO_TEMPORAL: &str = "gitmereba-reconciliar";
const USUARIO_GITHUB: &str = "x-access-token";
const ORIGEN_GITHUB: &str = "https://github.com/";
const NAMESPACE_REMOTO_HEADS: &str = "refs/mereba-remoto/heads/";
const NAMESPACE_REMOTO_TAGS: &str = "refs/mereba-remoto/tags/";

/// De dónde viene / a dónde va la reconciliación.
///
/// En producción siempre [`OrigenReconciliacion::Github`] (debe empezar por
/// `https://github.com/`, si no `reconciliar` devuelve
/// [`ErrorContingencia::OrigenNoGithub`]); [`OrigenReconciliacion::LocalParaPruebas`]
/// solo existe para las pruebas de integración de este módulo, donde un bare local hace
/// de «GitHub».
#[derive(Debug, Clone)]
pub enum OrigenReconciliacion {
    Github(String),
    LocalParaPruebas(PathBuf),
}

/// Qué pasó con una rama al reconciliar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultadoRama {
    Enviada {
        rama: String,
    },
    Creada {
        rama: String,
    },
    SinCambios {
        rama: String,
    },
    Divergente {
        rama: String,
        locales: u64,
        remotos: u64,
    },
    /// Borrada en el repo de contingencia: nunca se borra en el remoto por esto, solo se
    /// informa.
    BorradaLocalmente {
        rama: String,
    },
    Fallida {
        rama: String,
        motivo: String,
    },
}

/// Qué pasó con un tag al reconciliar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultadoTag {
    Enviado {
        tag: String,
    },
    /// Ya existe en el remoto con otro SHA: no se toca.
    ConflictoNoTocado {
        tag: String,
    },
}

/// Resultado de [`reconciliar`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformeReconciliacion {
    pub ramas: Vec<ResultadoRama>,
    pub tags: Vec<ResultadoTag>,
    /// Solo `true` si no quedó ninguna rama divergente ni fallida.
    pub completa: bool,
}

impl InformeReconciliacion {
    /// Resumen en español: es el texto que se guarda en la auditoría; para mostrarlo al
    /// usuario, [`Localizable::localizar`].
    pub fn resumen(&self) -> String {
        self.localizar(Idioma::Es)
    }

    /// Las cifras y nombres de la reconciliación, que los catálogos de idioma convierten
    /// en texto.
    pub(crate) fn cifras(&self) -> CifrasReconciliacion<'_> {
        CifrasReconciliacion {
            enviadas: self
                .ramas
                .iter()
                .filter(|r| {
                    matches!(
                        r,
                        ResultadoRama::Enviada { .. } | ResultadoRama::Creada { .. }
                    )
                })
                .count(),
            divergentes: self
                .ramas
                .iter()
                .filter_map(|r| match r {
                    ResultadoRama::Divergente { rama, .. } => Some(rama.as_str()),
                    _ => None,
                })
                .collect(),
            fallidas: self
                .ramas
                .iter()
                .filter_map(|r| match r {
                    ResultadoRama::Fallida { rama, .. } => Some(rama.as_str()),
                    _ => None,
                })
                .collect(),
            borradas: self
                .ramas
                .iter()
                .filter_map(|r| match r {
                    ResultadoRama::BorradaLocalmente { rama } => Some(rama.as_str()),
                    _ => None,
                })
                .collect(),
            tags_enviados: self
                .tags
                .iter()
                .filter(|t| matches!(t, ResultadoTag::Enviado { .. }))
                .count(),
            tags_conflicto: self
                .tags
                .iter()
                .filter_map(|t| match t {
                    ResultadoTag::ConflictoNoTocado { tag } => Some(tag.as_str()),
                    _ => None,
                })
                .collect(),
            completa: self.completa,
        }
    }
}

/// Cifras y nombres de un [`InformeReconciliacion`] para redactar su resumen.
pub(crate) struct CifrasReconciliacion<'a> {
    pub enviadas: usize,
    pub divergentes: Vec<&'a str>,
    pub fallidas: Vec<&'a str>,
    pub borradas: Vec<&'a str>,
    pub tags_enviados: usize,
    pub tags_conflicto: Vec<&'a str>,
    pub completa: bool,
}

/// Envía a GitHub (o, en pruebas, a `origen`) los commits de más del repo de
/// contingencia de `id_original`, rama a rama, sin forzar nunca nada.
///
/// `token_escritura` no se guarda en ningún sitio: solo viaja al proceso `git` como
/// cabecera HTTP (ver `git::ejecutar`), y cualquier aparición literal suya en un mensaje
/// que este módulo construye se sustituye por `***` antes de devolverlo.
///
/// Primero clona (lectura) el bare de contingencia a un clon bare temporal fuera del
/// árbol de Gitea, y hace `git fetch` del estado actual del remoto a un espacio de refs
/// propio dentro de ese clon: nunca se escribe en ningún bare de Gitea.
pub async fn reconciliar(
    rutas: &RutasCuenta,
    id_original: &IdRepo,
    punto_de_partida: &PuntoDePartida,
    origen: &OrigenReconciliacion,
    token_escritura: &Secreto,
) -> Result<InformeReconciliacion, ErrorContingencia> {
    let dueno_contingencia = org_contingencia(&id_original.dueno)?;
    let id_contingencia = IdRepo {
        dueno: dueno_contingencia,
        nombre: id_original.nombre.clone(),
    };
    let bare_contingencia = ruta_bare_validada(&rutas.gitea_repositorios(), &id_contingencia)
        .ok_or_else(|| ErrorContingencia::NoActivada(id_original.clone()))?;

    let (remoto_arg, credencial) = preparar_destino(origen, token_escritura)?;

    let temporal = DirTemporal::nueva(PREFIJO_TEMPORAL)?;
    clonar_local(&bare_contingencia, temporal.ruta()).await?;
    obtener_estado_remoto(temporal.ruta(), &remoto_arg, credencial.as_ref()).await?;

    let refs_temporal = git::refs(temporal.ruta()).await?;
    let locales_heads = filtrar(&refs_temporal, "refs/heads/");
    let locales_tags = filtrar(&refs_temporal, "refs/tags/");
    let remotos_heads = filtrar(&refs_temporal, NAMESPACE_REMOTO_HEADS);
    let remotos_tags = filtrar(&refs_temporal, NAMESPACE_REMOTO_TAGS);

    let mut ramas = Vec::new();
    for (rama, sha_local) in &locales_heads {
        validar_ref(rama)?;
        let referencia_partida = format!("refs/heads/{rama}");
        let sha_partida = punto_de_partida.sha_de(&referencia_partida);
        if sha_partida == Some(sha_local.as_str()) {
            continue; // sin commits de más: nada que reconciliar en esta rama.
        }

        let remoto_sha = remotos_heads.get(rama).cloned();
        let es_rama_nueva = remoto_sha.is_none();
        let entrada = match &remoto_sha {
            Some(sha_remoto) => EntradaDecision {
                remoto: Some(sha_remoto.as_str()),
                partida: sha_partida,
                remoto_es_ancestro_de_local: git::es_ancestro(
                    temporal.ruta(),
                    sha_remoto,
                    sha_local,
                )
                .await?,
                local_es_ancestro_de_remoto: git::es_ancestro(
                    temporal.ruta(),
                    sha_local,
                    sha_remoto,
                )
                .await?,
            },
            None => EntradaDecision {
                remoto: None,
                partida: sha_partida,
                remoto_es_ancestro_de_local: false,
                local_es_ancestro_de_remoto: false,
            },
        };
        let decision = decidir(&entrada);

        let resultado = match decision {
            Decision::RamaNueva | Decision::AvanceRapido => {
                match empujar(
                    temporal.ruta(),
                    &remoto_arg,
                    credencial.as_ref(),
                    &refspec_rama(rama),
                )
                .await
                {
                    Ok(()) if es_rama_nueva => ResultadoRama::Creada { rama: rama.clone() },
                    Ok(()) => ResultadoRama::Enviada { rama: rama.clone() },
                    Err(e) => ResultadoRama::Fallida {
                        rama: rama.clone(),
                        motivo: sanear(e.to_string(), token_escritura),
                    },
                }
            }
            Decision::SinCambios => ResultadoRama::SinCambios { rama: rama.clone() },
            Decision::Divergente => {
                let sha_remoto = remoto_sha.clone().unwrap_or_default();
                let locales = git::commits_de_mas(temporal.ruta(), sha_local, &sha_remoto).await?;
                let remotos = git::commits_de_mas(temporal.ruta(), &sha_remoto, sha_local).await?;
                ResultadoRama::Divergente {
                    rama: rama.clone(),
                    locales,
                    remotos,
                }
            }
        };
        ramas.push(resultado);
    }

    for referencia in punto_de_partida.refs.keys() {
        if let Some(rama) = referencia.strip_prefix("refs/heads/")
            && !locales_heads.contains_key(rama)
        {
            ramas.push(ResultadoRama::BorradaLocalmente {
                rama: rama.to_string(),
            });
        }
    }

    let mut tags = Vec::new();
    for (tag, sha_local) in &locales_tags {
        validar_ref(tag)?;
        match remotos_tags.get(tag) {
            None => match empujar(
                temporal.ruta(),
                &remoto_arg,
                credencial.as_ref(),
                &refspec_tag(tag),
            )
            .await
            {
                Ok(()) => tags.push(ResultadoTag::Enviado { tag: tag.clone() }),
                Err(_) => tags.push(ResultadoTag::ConflictoNoTocado { tag: tag.clone() }),
            },
            Some(sha_remoto) if sha_remoto == sha_local => {}
            Some(_) => tags.push(ResultadoTag::ConflictoNoTocado { tag: tag.clone() }),
        }
    }

    let completa = ramas.iter().all(|r| {
        !matches!(
            r,
            ResultadoRama::Divergente { .. } | ResultadoRama::Fallida { .. }
        )
    });

    Ok(InformeReconciliacion {
        ramas,
        tags,
        completa,
    })
}

fn filtrar(refs: &[(String, String)], prefijo: &str) -> BTreeMap<String, String> {
    refs.iter()
        .filter_map(|(nombre, sha)| {
            nombre
                .strip_prefix(prefijo)
                .map(|resto| (resto.to_string(), sha.clone()))
        })
        .collect()
}

fn preparar_destino(
    origen: &OrigenReconciliacion,
    token: &Secreto,
) -> Result<(String, Option<Credencial>), ErrorContingencia> {
    match origen {
        OrigenReconciliacion::Github(url) => {
            if !url.starts_with(ORIGEN_GITHUB) {
                return Err(ErrorContingencia::OrigenNoGithub);
            }
            let credencial = Credencial {
                origen: ORIGEN_GITHUB.to_string(),
                usuario: USUARIO_GITHUB.to_string(),
                token: token.clone(),
            };
            Ok((url.clone(), Some(credencial)))
        }
        OrigenReconciliacion::LocalParaPruebas(ruta) => {
            let texto = ruta.to_str().ok_or_else(|| {
                ErrorContingencia::Io("la ruta de origen de pruebas no es UTF-8".to_string())
            })?;
            Ok((texto.to_string(), None))
        }
    }
}

async fn clonar_local(bare_contingencia: &Path, destino: &Path) -> Result<(), ErrorGit> {
    let origen = bare_contingencia
        .to_str()
        .ok_or_else(|| ErrorGit::EntradaInvalida("la ruta del bare no es UTF-8".to_string()))?;
    let opciones = Opciones {
        directorio: Some(destino.to_path_buf()),
        ..Opciones::default()
    };
    git::ejecutar(&["clone", "--bare", "-q", origen, "."], &opciones).await?;
    Ok(())
}

async fn obtener_estado_remoto(
    temporal: &Path,
    remoto: &str,
    credencial: Option<&Credencial>,
) -> Result<(), ErrorGit> {
    let opciones = Opciones {
        directorio: Some(temporal.to_path_buf()),
        credencial: credencial.cloned(),
        ..Opciones::default()
    };
    let refspec_heads = format!("+refs/heads/*:{NAMESPACE_REMOTO_HEADS}*");
    let refspec_tags = format!("+refs/tags/*:{NAMESPACE_REMOTO_TAGS}*");
    git::ejecutar(
        &["fetch", "--no-tags", remoto, &refspec_heads, &refspec_tags],
        &opciones,
    )
    .await?;
    Ok(())
}

async fn empujar(
    temporal: &Path,
    remoto: &str,
    credencial: Option<&Credencial>,
    refspec: &str,
) -> Result<(), ErrorGit> {
    let opciones = Opciones {
        directorio: Some(temporal.to_path_buf()),
        credencial: credencial.cloned(),
        ..Opciones::default()
    };
    let args = argumentos_push(remoto, refspec);
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    git::ejecutar(&args_str, &opciones).await?;
    Ok(())
}

/// Sustituye cualquier aparición literal de `token` en `mensaje` por `***`: defensa en
/// profundidad además del saneado que ya hace `git::proceso` sobre `stderr`.
fn sanear(mensaje: String, token: &Secreto) -> String {
    if token.esta_vacio() {
        return mensaje;
    }
    mensaje.replace(token.exponer(), "***")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumen_lista_lo_relevante_en_espanol() {
        let informe = InformeReconciliacion {
            ramas: vec![
                ResultadoRama::Enviada {
                    rama: "main".to_string(),
                },
                ResultadoRama::Divergente {
                    rama: "dev".to_string(),
                    locales: 1,
                    remotos: 2,
                },
            ],
            tags: vec![ResultadoTag::Enviado {
                tag: "v1".to_string(),
            }],
            completa: false,
        };
        let resumen = informe.resumen();
        assert!(resumen.contains("reconciliación incompleta"));
        assert!(resumen.contains("dev"));
        assert!(resumen.contains("1 rama(s) enviada(s)"));
        assert!(resumen.contains("1 tag(s) enviado(s)"));
    }

    #[test]
    fn el_resumen_sale_en_el_idioma_pedido() {
        use crate::idioma::{Idioma, Localizable};
        let informe = InformeReconciliacion {
            ramas: vec![
                ResultadoRama::Enviada {
                    rama: "main".to_string(),
                },
                ResultadoRama::Divergente {
                    rama: "dev".to_string(),
                    locales: 1,
                    remotos: 2,
                },
                ResultadoRama::Fallida {
                    rama: "x".to_string(),
                    motivo: "m".to_string(),
                },
                ResultadoRama::BorradaLocalmente {
                    rama: "vieja".to_string(),
                },
            ],
            tags: vec![
                ResultadoTag::Enviado {
                    tag: "v1".to_string(),
                },
                ResultadoTag::ConflictoNoTocado {
                    tag: "v2".to_string(),
                },
            ],
            completa: false,
        };
        assert_eq!(informe.localizar(Idioma::Es), informe.resumen());
        assert_eq!(
            informe.localizar(Idioma::En),
            "incomplete reconciliation: 1 branch(es) sent; 1 diverged branch(es) (dev); \
             1 failed branch(es) (x); 1 branch(es) deleted locally, kept on the remote (vieja); \
             1 tag(s) sent; 1 conflicting tag(s), left untouched (v2)"
        );
    }

    #[test]
    fn preparar_destino_rechaza_un_origen_que_no_es_github() {
        let resultado = preparar_destino(
            &OrigenReconciliacion::Github("https://gitlab.com/a/b.git".to_string()),
            &Secreto::nuevo("t"),
        );
        assert!(matches!(resultado, Err(ErrorContingencia::OrigenNoGithub)));
    }

    #[test]
    fn preparar_destino_local_no_lleva_credencial() {
        let (remoto, credencial) = preparar_destino(
            &OrigenReconciliacion::LocalParaPruebas(PathBuf::from("/tmp/origen.git")),
            &Secreto::nuevo("t"),
        )
        .expect("origen local válido");
        assert_eq!(remoto, "/tmp/origen.git");
        assert!(credencial.is_none());
    }

    #[test]
    fn sanea_el_token_de_un_mensaje_de_error() {
        let token = Secreto::nuevo("ghp_super_secreto");
        let mensaje = sanear("fallo: ghp_super_secreto rechazado".to_string(), &token);
        assert!(!mensaje.contains("ghp_super_secreto"));
        assert!(mensaje.contains("***"));
    }
}
