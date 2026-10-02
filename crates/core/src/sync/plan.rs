//! Planificación pura de la sincronización: sin E/S, es la parte más importante y más
//! probada del módulo.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::modelo::{Cuenta, IdRepo, Nombre, RepoLocal, RepoOrigen};

const PREFIJO_CONTINGENCIA: &str = "contingencia-";

/// Lo que se conoce de la sincronización anterior: qué mirrors están pausados por ser
/// huérfanos y qué intervalo (minutos) tenía cada mirror activo.
///
/// `sync` no persiste nada (no depende de `almacen`): el llamador reconstruye este valor
/// a partir del [`super::InformeSync`] anterior y lo vuelve a pasar en la siguiente
/// llamada a [`planificar`] (o a [`super::ejecutar`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoPrevio {
    /// Mirrors marcados huérfanos (pausados) en una sincronización anterior.
    pub huerfanos: BTreeSet<IdRepo>,
    /// Último intervalo (minutos) aplicado a cada mirror activo (no huérfano).
    pub intervalos_aplicados: BTreeMap<IdRepo, u32>,
}

/// Una acción a aplicar sobre Gitea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Accion {
    /// Crea el mirror de un repo de GitHub que todavía no existe en local.
    CrearMirror(RepoOrigen),
    /// Pausa (intervalo `"0"`) un mirror que ya no está en GitHub. Nunca se borra.
    MarcarHuerfano(IdRepo),
    /// Reactiva un mirror que había sido marcado huérfano y ha vuelto a aparecer.
    Reanudar(IdRepo),
    /// Cambia el intervalo de un mirror activo porque el de la cuenta ha cambiado.
    AjustarIntervalo { id: IdRepo, minutos: u32 },
}

impl Accion {
    /// El repo al que afecta, para ordenar el plan y para buscarlo en el informe.
    pub fn id(&self) -> &IdRepo {
        match self {
            Accion::CrearMirror(repo) => &repo.id,
            Accion::MarcarHuerfano(id) | Accion::Reanudar(id) => id,
            Accion::AjustarIntervalo { id, .. } => id,
        }
    }
}

/// Por qué un repo de GitHub no se ha incluido en el plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotivoOmision {
    /// Está en `alcance.excluidos`.
    Excluido,
    /// Es un fork y `alcance.incluir_forks` es `false`.
    ForkNoIncluido,
    /// Su dueño no es el login de la cuenta ni está en `alcance.organizaciones`.
    OrganizacionNoIncluida,
    /// Su dueño empieza por `contingencia-`: pertenece al módulo de contingencia.
    EnContingencia,
}

/// Un repo omitido del plan, con el motivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omitido {
    pub id: IdRepo,
    pub motivo: MotivoOmision,
}

/// Aviso de que el plan ha aplicado una salvaguarda y, por precaución, ha dejado de hacer
/// algo que normalmente haría.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlertaPlan {
    /// El listado de GitHub venía vacío, o marcaría huérfanos a más del 50 % de los
    /// mirrors locales (siendo más de 3): puede ser un origen comprometido o una
    /// respuesta vacía errónea. No se ha marcado ningún huérfano en este plan.
    DemasiadosHuerfanos {
        /// Cuántos mirrors se habrían marcado huérfanos.
        candidatos: usize,
        /// Cuántos mirrors locales hay en total (el denominador del porcentaje).
        total_mirrors: usize,
        /// Si, además, el listado de GitHub venía vacío.
        listado_github_vacio: bool,
    },
}

/// Resultado de [`planificar`]: qué hacer, y qué se ha decidido no hacer y por qué.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Ordenadas por [`IdRepo`] (salida determinista).
    pub acciones: Vec<Accion>,
    /// Ordenados por [`IdRepo`].
    pub omitidos: Vec<Omitido>,
    pub alerta: Option<AlertaPlan>,
}

fn empieza_por_contingencia(dueno: &Nombre) -> bool {
    dueno
        .as_str()
        .to_lowercase()
        .starts_with(PREFIJO_CONTINGENCIA)
}

fn clave(dueno: &Nombre, nombre: &Nombre) -> (String, String) {
    (
        dueno.as_str().to_lowercase(),
        nombre.as_str().to_lowercase(),
    )
}

fn clave_id(id: &IdRepo) -> (String, String) {
    clave(&id.dueno, &id.nombre)
}

/// Decide qué hacer para poner el Gitea local al día con GitHub, sin tocar la red ni el
/// disco. La comparación de nombres es insensible a mayúsculas (GitHub y Gitea lo son),
/// pero las acciones conservan la grafía de GitHub siempre que hay un repo de origen con
/// el que emparejar.
pub fn planificar(
    cuenta: &Cuenta,
    origen: &[RepoOrigen],
    local: &[RepoLocal],
    estado_previo: &EstadoPrevio,
) -> Plan {
    let duenos_permitidos: BTreeSet<String> = std::iter::once(cuenta.login.as_str().to_lowercase())
        .chain(
            cuenta
                .alcance
                .organizaciones
                .iter()
                .map(|n| n.as_str().to_lowercase()),
        )
        .collect();
    let excluidos: BTreeSet<(String, String)> =
        cuenta.alcance.excluidos.iter().map(clave_id).collect();

    let mut omitidos: BTreeMap<IdRepo, MotivoOmision> = BTreeMap::new();
    let mut origen_valido: BTreeMap<(String, String), &RepoOrigen> = BTreeMap::new();

    for repo in origen {
        let k = clave_id(&repo.id);
        if empieza_por_contingencia(&repo.id.dueno) {
            omitidos.insert(repo.id.clone(), MotivoOmision::EnContingencia);
            continue;
        }
        if !duenos_permitidos.contains(&k.0) {
            omitidos.insert(repo.id.clone(), MotivoOmision::OrganizacionNoIncluida);
            continue;
        }
        if repo.es_fork && !cuenta.alcance.incluir_forks {
            omitidos.insert(repo.id.clone(), MotivoOmision::ForkNoIncluido);
            continue;
        }
        if excluidos.contains(&k) {
            omitidos.insert(repo.id.clone(), MotivoOmision::Excluido);
            continue;
        }
        origen_valido.insert(k, repo);
    }

    let mut acciones: Vec<Accion> = Vec::new();
    let mut candidatos_huerfano: Vec<IdRepo> = Vec::new();
    let mut total_mirrors = 0usize;

    for repo in local {
        // Un repo local que no es mirror nunca se marca huérfano ni se toca.
        if !repo.es_mirror {
            continue;
        }
        // Las organizaciones `contingencia-*` pertenecen a otro módulo: se ignoran del
        // todo, incluso si el llamador las hubiera colado aquí por error.
        if empieza_por_contingencia(&repo.id.dueno) {
            continue;
        }
        let k = clave_id(&repo.id);
        if excluidos.contains(&k) {
            // Un excluido que ya existe en local no se borra ni se toca; solo se informa.
            omitidos.insert(repo.id.clone(), MotivoOmision::Excluido);
            continue;
        }
        total_mirrors += 1;
        match origen_valido.remove(&k) {
            Some(remoto) => {
                if estado_previo.huerfanos.contains(&remoto.id) {
                    acciones.push(Accion::Reanudar(remoto.id.clone()));
                } else if let Some(&ultimo_intervalo) =
                    estado_previo.intervalos_aplicados.get(&remoto.id)
                {
                    // Solo se reajusta si sabemos con certeza (por un pase anterior)
                    // que el intervalo aplicado ya no coincide con el de la cuenta. Sin
                    // ese historial (primera vez que se ve este mirror en un
                    // `EstadoPrevio` nuevo) no se toca: no hay forma de conocer el
                    // intervalo real sin consultarlo a Gitea, y `RepoLocal` no lo trae.
                    if ultimo_intervalo != cuenta.intervalo_minutos {
                        acciones.push(Accion::AjustarIntervalo {
                            id: remoto.id.clone(),
                            minutos: cuenta.intervalo_minutos,
                        });
                    }
                }
            }
            None if estado_previo.huerfanos.contains(&repo.id) => {
                // Ya estaba huérfano y sigue sin aparecer en GitHub: no hay nada que
                // hacer, ya está pausado.
            }
            None => candidatos_huerfano.push(repo.id.clone()),
        }
    }

    // Lo que queda en `origen_valido` son altas nuevas: no había ningún mirror local con
    // ese dueño/nombre.
    for repo in origen_valido.into_values() {
        acciones.push(Accion::CrearMirror(repo.clone()));
    }

    let listado_github_vacio = origen.is_empty();
    let habria_huerfanos = !candidatos_huerfano.is_empty();
    let salvaguarda = habria_huerfanos
        && (listado_github_vacio
            || (candidatos_huerfano.len() * 2 > total_mirrors && total_mirrors > 3));

    let alerta = if salvaguarda {
        Some(AlertaPlan::DemasiadosHuerfanos {
            candidatos: candidatos_huerfano.len(),
            total_mirrors,
            listado_github_vacio,
        })
    } else {
        for id in candidatos_huerfano {
            acciones.push(Accion::MarcarHuerfano(id));
        }
        None
    };

    acciones.sort_by(|a, b| a.id().cmp(b.id()));
    let omitidos = omitidos
        .into_iter()
        .map(|(id, motivo)| Omitido { id, motivo })
        .collect();

    Plan {
        acciones,
        omitidos,
        alerta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::dobles::pruebas::{cuenta, id, repo_local, repo_local_no_mirror, repo_origen};

    #[test]
    fn crea_mirrors_para_repos_nuevos() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let origen = vec![repo_origen("jparga", "uno", false, false)];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        assert_eq!(plan.acciones, vec![Accion::CrearMirror(origen[0].clone())]);
        assert!(plan.omitidos.is_empty());
        assert!(plan.alerta.is_none());
    }

    #[test]
    fn excluidos_nunca_se_crean_y_se_informan() {
        let excluido = id("jparga", "secreto");
        let c = cuenta("jparga", false, &[], std::slice::from_ref(&excluido), 30);
        let origen = vec![
            repo_origen("jparga", "secreto", false, false),
            repo_origen("jparga", "publico", false, false),
        ];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        assert_eq!(plan.acciones, vec![Accion::CrearMirror(origen[1].clone())]);
        assert_eq!(
            plan.omitidos,
            vec![Omitido {
                id: excluido,
                motivo: MotivoOmision::Excluido
            }]
        );
    }

    #[test]
    fn excluido_ya_existente_en_local_no_se_toca() {
        let excluido = id("jparga", "secreto");
        let c = cuenta("jparga", false, &[], std::slice::from_ref(&excluido), 30);
        // Ya no está en GitHub, pero como está excluido no debe marcarse huérfano.
        let local = vec![repo_local("jparga", "secreto", true)];
        let plan = planificar(&c, &[], &local, &EstadoPrevio::default());
        assert!(plan.acciones.is_empty());
        assert_eq!(
            plan.omitidos,
            vec![Omitido {
                id: excluido,
                motivo: MotivoOmision::Excluido
            }]
        );
    }

    #[test]
    fn forks_no_incluidos_por_defecto() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let origen = vec![repo_origen("jparga", "fork1", false, true)];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        assert!(plan.acciones.is_empty());
        assert_eq!(
            plan.omitidos,
            vec![Omitido {
                id: id("jparga", "fork1"),
                motivo: MotivoOmision::ForkNoIncluido
            }]
        );
    }

    #[test]
    fn forks_incluidos_si_el_alcance_lo_permite() {
        let c = cuenta("jparga", true, &[], &[], 30);
        let origen = vec![repo_origen("jparga", "fork1", false, true)];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        assert_eq!(plan.acciones, vec![Accion::CrearMirror(origen[0].clone())]);
        assert!(plan.omitidos.is_empty());
    }

    #[test]
    fn organizacion_incluida_se_considera() {
        let c = cuenta("jparga", false, &["acme"], &[], 30);
        let origen = vec![repo_origen("acme", "repo1", false, false)];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        assert_eq!(plan.acciones, vec![Accion::CrearMirror(origen[0].clone())]);
    }

    #[test]
    fn organizacion_no_incluida_se_omite() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let origen = vec![repo_origen("otraorg", "repo1", false, false)];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        assert!(plan.acciones.is_empty());
        assert_eq!(
            plan.omitidos,
            vec![Omitido {
                id: id("otraorg", "repo1"),
                motivo: MotivoOmision::OrganizacionNoIncluida
            }]
        );
    }

    #[test]
    fn huerfano_se_marca_y_pausa() {
        let c = cuenta("jparga", false, &[], &[], 30);
        // Un repo que sigue existiendo, para que el listado de GitHub no esté vacío y no
        // salte la salvaguarda (que se prueba aparte).
        let origen = vec![repo_origen("jparga", "sigue", false, false)];
        let local = vec![
            repo_local("jparga", "sigue", true),
            repo_local("jparga", "desaparecido", true),
        ];
        let plan = planificar(&c, &origen, &local, &EstadoPrevio::default());
        assert_eq!(
            plan.acciones,
            vec![Accion::MarcarHuerfano(id("jparga", "desaparecido"))]
        );
    }

    #[test]
    fn huerfano_que_reaparece_se_reanuda() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let origen = vec![repo_origen("jparga", "vuelve", false, false)];
        let local = vec![repo_local("jparga", "vuelve", true)];
        let mut previo = EstadoPrevio::default();
        previo.huerfanos.insert(id("jparga", "vuelve"));
        let plan = planificar(&c, &origen, &local, &previo);
        assert_eq!(
            plan.acciones,
            vec![Accion::Reanudar(id("jparga", "vuelve"))]
        );
    }

    #[test]
    fn huerfano_ya_marcado_que_sigue_ausente_no_repite_accion() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let local = vec![repo_local("jparga", "ausente", true)];
        let mut previo = EstadoPrevio::default();
        previo.huerfanos.insert(id("jparga", "ausente"));
        let plan = planificar(&c, &[], &local, &previo);
        assert!(plan.acciones.is_empty());
    }

    #[test]
    fn repo_local_no_mirror_nunca_se_toca() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let local = vec![repo_local_no_mirror("jparga", "manual")];
        let plan = planificar(&c, &[], &local, &EstadoPrevio::default());
        assert!(plan.acciones.is_empty());
        assert!(plan.omitidos.is_empty());
    }

    #[test]
    fn organizaciones_de_contingencia_se_ignoran_por_completo() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let origen = vec![repo_origen("contingencia-jparga", "repo1", false, false)];
        let local = vec![repo_local("contingencia-jparga", "repo2", true)];
        let plan = planificar(&c, &origen, &local, &EstadoPrevio::default());
        assert!(plan.acciones.is_empty());
        assert_eq!(
            plan.omitidos,
            vec![Omitido {
                id: id("contingencia-jparga", "repo1"),
                motivo: MotivoOmision::EnContingencia
            }]
        );
    }

    #[test]
    fn salvaguarda_por_listado_vacio_no_marca_huerfanos() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let local = vec![repo_local("jparga", "uno", true)];
        let plan = planificar(&c, &[], &local, &EstadoPrevio::default());
        assert!(plan.acciones.is_empty());
        assert_eq!(
            plan.alerta,
            Some(AlertaPlan::DemasiadosHuerfanos {
                candidatos: 1,
                total_mirrors: 1,
                listado_github_vacio: true,
            })
        );
    }

    #[test]
    fn salvaguarda_por_mas_del_50_por_ciento() {
        let c = cuenta("jparga", false, &[], &[], 30);
        // 4 mirrors locales, ninguno en GitHub salvo uno: se marcarían 3/4 (75 %).
        let local = vec![
            repo_local("jparga", "sigue", true),
            repo_local("jparga", "uno", true),
            repo_local("jparga", "dos", true),
            repo_local("jparga", "tres", true),
        ];
        let origen = vec![repo_origen("jparga", "sigue", false, false)];
        let plan = planificar(&c, &origen, &local, &EstadoPrevio::default());
        assert!(
            plan.acciones
                .iter()
                .all(|a| !matches!(a, Accion::MarcarHuerfano(_)))
        );
        assert_eq!(
            plan.alerta,
            Some(AlertaPlan::DemasiadosHuerfanos {
                candidatos: 3,
                total_mirrors: 4,
                listado_github_vacio: false,
            })
        );
    }

    #[test]
    fn no_salta_la_salvaguarda_con_pocos_mirrors() {
        // Con 3 mirrors o menos, el porcentaje no activa la salvaguarda aunque se
        // marquen todos huérfanos.
        let c = cuenta("jparga", false, &[], &[], 30);
        let local = vec![
            repo_local("jparga", "uno", true),
            repo_local("jparga", "dos", true),
            repo_local("jparga", "tres", true),
        ];
        let origen = vec![repo_origen(
            "jparga",
            "algoqueseguirimportando",
            false,
            false,
        )];
        let plan = planificar(&c, &origen, &local, &EstadoPrevio::default());
        assert!(plan.alerta.is_none());
        let huerfanos: Vec<_> = plan
            .acciones
            .iter()
            .filter(|a| matches!(a, Accion::MarcarHuerfano(_)))
            .collect();
        assert_eq!(huerfanos.len(), 3);
    }

    #[test]
    fn mayusculas_y_minusculas_se_tratan_igual_conservando_grafia_de_github() {
        let c = cuenta("JParga", false, &[], &[], 30);
        // Gitea guarda el dueño en minúsculas; GitHub informa "JParga".
        let local = vec![repo_local("jparga", "Repo", true)];
        let origen = vec![repo_origen("JParga", "REPO", false, false)];
        let plan = planificar(&c, &origen, &local, &EstadoPrevio::default());
        // No hay alta (ya existe) ni huérfano: coinciden insensible a mayúsculas.
        assert!(plan.acciones.is_empty());
    }

    #[test]
    fn ajusta_el_intervalo_cuando_cambia_el_de_la_cuenta() {
        let c = cuenta("jparga", false, &[], &[], 60);
        let origen = vec![repo_origen("jparga", "repo1", false, false)];
        let local = vec![repo_local("jparga", "repo1", true)];
        let mut previo = EstadoPrevio::default();
        previo
            .intervalos_aplicados
            .insert(id("jparga", "repo1"), 30);
        let plan = planificar(&c, &origen, &local, &previo);
        assert_eq!(
            plan.acciones,
            vec![Accion::AjustarIntervalo {
                id: id("jparga", "repo1"),
                minutos: 60
            }]
        );
    }

    #[test]
    fn no_ajusta_el_intervalo_si_no_ha_cambiado() {
        let c = cuenta("jparga", false, &[], &[], 30);
        let origen = vec![repo_origen("jparga", "repo1", false, false)];
        let local = vec![repo_local("jparga", "repo1", true)];
        let mut previo = EstadoPrevio::default();
        previo
            .intervalos_aplicados
            .insert(id("jparga", "repo1"), 30);
        let plan = planificar(&c, &origen, &local, &previo);
        assert!(plan.acciones.is_empty());
    }

    #[test]
    fn el_orden_de_las_acciones_es_deterministico() {
        let c = cuenta("jparga", false, &["acme"], &[], 30);
        let origen = vec![
            repo_origen("acme", "zzz", false, false),
            repo_origen("jparga", "aaa", false, false),
            repo_origen("jparga", "mmm", false, false),
        ];
        let plan = planificar(&c, &origen, &[], &EstadoPrevio::default());
        let ids: Vec<String> = plan.acciones.iter().map(|a| a.id().to_string()).collect();
        let mut esperado = ids.clone();
        esperado.sort();
        assert_eq!(ids, esperado);
    }
}
