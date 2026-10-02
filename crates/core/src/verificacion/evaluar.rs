//! Evaluación pura del estado de salud de un mirror: sin E/S.

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::modelo::{EstadoRepo, IdRepo, RepoLocal, RepoOrigen};

use super::motivo::Motivo;
use super::umbrales::Umbrales;

/// Resultado de comprobar la integridad de un bare local con `git fsck`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoFsck {
    Ok,
    /// Mensaje saneado de `git fsck`, sin secretos.
    Corrupto(String),
}

/// Todo lo ya recogido de un repo, listo para que [`evaluar`] decida su estado sin tocar
/// la red ni el disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaRepo {
    /// Lo que informa Gitea del mirror local.
    pub local: RepoLocal,
    /// Lo que informa GitHub, si el repo sigue existiendo allí.
    pub origen: Option<RepoOrigen>,
    /// Ya no está en GitHub (el mirror se conserva, pero se ha pausado).
    pub huerfano: bool,
    /// El usuario lo ha excluido del alcance.
    pub excluido: bool,
    /// Convertido en repo con escritura durante una contingencia activa.
    pub en_contingencia: bool,
    /// SHA de la rama por defecto en GitHub, si se ha podido consultar en esta pasada.
    pub sha_github: Option<String>,
    /// SHA de `refs/heads/<rama_por_defecto>` en el bare local.
    pub sha_local: Option<String>,
    /// Resultado de `git fsck`, si se ha ejecutado en esta pasada.
    pub fsck: Option<ResultadoFsck>,
    /// Error al leer el estado local (refs, fsck...), si lo ha habido.
    pub error_lectura: Option<String>,
    /// Cuánto se sabe que lleva existiendo el mirror sin sincronizar nunca, cuando no hay
    /// otra forma de saberlo (Gitea no informa la fecha de creación de un mirror).
    pub antiguedad_conocida: Option<Duration>,
}

/// Resultado de [`evaluar`] para un repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostico {
    pub id: IdRepo,
    pub estado: EstadoRepo,
    pub motivos: Vec<Motivo>,
    /// Cuánto hace de la última sincronización conocida, si se sabe.
    pub desfase: Option<Duration>,
}

fn sin_motivos(id: IdRepo, estado: EstadoRepo) -> Diagnostico {
    Diagnostico {
        id,
        estado,
        motivos: Vec::new(),
        desfase: None,
    }
}

/// Decide el [`EstadoRepo`] de un mirror a partir de lo ya recogido de él, sin E/S.
///
/// Reglas, por prioridad:
///
/// 1. Un objeto corrupto en el bare local (`fsck` = [`ResultadoFsck::Corrupto`]) siempre
///    gana: da igual que el repo esté excluido, en contingencia o huérfano.
/// 2. `excluido`, `en_contingencia` y `huerfano`, en ese orden.
/// 3. No poder leer el estado local (`error_lectura`) es un fallo.
/// 4. Un mirror no vacío en GitHub que nunca ha sincronizado y ya ha superado la gracia
///    inicial es un fallo.
/// 5. Un SHA distinto entre GitHub y el mirror local es un fallo si la última
///    sincronización ya es antigua, o un aviso informativo (`Ok`) si es reciente. Sin
///    fecha de sincronización fiable se trata como antigua: no hay forma de asumir a
///    quién beneficia la duda (el origen puede estar comprometido).
/// 6. Una sincronización más antigua que el umbral de obsolescencia, sin más.
/// 7. En cualquier otro caso, `Ok`.
pub fn evaluar(entrada: &EntradaRepo, ahora: OffsetDateTime, umbrales: &Umbrales) -> Diagnostico {
    let id = entrada.local.id.clone();

    if let Some(ResultadoFsck::Corrupto(_)) = &entrada.fsck {
        return Diagnostico {
            id,
            estado: EstadoRepo::Fallo,
            motivos: vec![Motivo::Corrupcion],
            desfase: None,
        };
    }

    if entrada.excluido {
        return sin_motivos(id, EstadoRepo::Excluido);
    }
    if entrada.en_contingencia {
        return sin_motivos(id, EstadoRepo::Contingencia);
    }
    if entrada.huerfano {
        return sin_motivos(id, EstadoRepo::Huerfano);
    }

    if entrada.error_lectura.is_some() {
        return Diagnostico {
            id,
            estado: EstadoRepo::Fallo,
            motivos: vec![Motivo::ErrorLocal],
            desfase: None,
        };
    }

    let desfase = entrada.local.ultima_sync.map(|ultima| ahora - ultima);

    let origen_no_vacio = entrada
        .origen
        .as_ref()
        .is_some_and(|o| o.tamano_kb > 0 && o.rama_por_defecto.is_some());
    if origen_no_vacio && entrada.local.vacio && entrada.local.ultima_sync.is_none() {
        let paso_la_gracia = entrada
            .antiguedad_conocida
            .is_some_and(|antiguedad| antiguedad > umbrales.gracia_inicial);
        if paso_la_gracia {
            return Diagnostico {
                id,
                estado: EstadoRepo::Fallo,
                motivos: vec![Motivo::NuncaSincronizado],
                desfase,
            };
        }
    }

    if let (Some(en_github), Some(en_local)) = (&entrada.sha_github, &entrada.sha_local)
        && en_github != en_local
    {
        // Sin una fecha de sincronización fiable no se puede afirmar que la diferencia es
        // reciente: por seguridad se trata como una divergencia antigua, nunca al revés.
        let antigua = desfase.is_none_or(|d| d > umbrales.obsoleto);
        return if antigua {
            Diagnostico {
                id,
                estado: EstadoRepo::Fallo,
                motivos: vec![Motivo::ShaDistinto],
                desfase,
            }
        } else {
            Diagnostico {
                id,
                estado: EstadoRepo::Ok,
                motivos: vec![Motivo::PendienteDeSincronizar],
                desfase,
            }
        };
    }

    if desfase.is_some_and(|d| d > umbrales.obsoleto) {
        return Diagnostico {
            id,
            estado: EstadoRepo::Obsoleto,
            motivos: Vec::new(),
            desfase,
        };
    }

    Diagnostico {
        id,
        estado: EstadoRepo::Ok,
        motivos: Vec::new(),
        desfase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> IdRepo {
        crate::verificacion::dobles::pruebas::id("jparga", "repo")
    }

    fn base() -> EntradaRepo {
        EntradaRepo {
            local: RepoLocal {
                id: id(),
                es_mirror: true,
                vacio: false,
                privado: true,
                tamano_kb: 10,
                ultima_sync: None,
            },
            origen: None,
            huerfano: false,
            excluido: false,
            en_contingencia: false,
            sha_github: None,
            sha_local: None,
            fsck: None,
            error_lectura: None,
            antiguedad_conocida: None,
        }
    }

    fn origen(tamano_kb: u64, rama: Option<&str>) -> RepoOrigen {
        crate::verificacion::dobles::pruebas::repo_origen("jparga", "repo", tamano_kb, rama)
    }

    const AHORA: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn hace(minutos: i64) -> OffsetDateTime {
        AHORA - Duration::minutes(minutos)
    }

    // --- Prioridad 1: excluido, contingencia, huérfano ---------------------------------

    #[test]
    fn excluido_es_excluido() {
        let entrada = EntradaRepo {
            excluido: true,
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Excluido);
        assert!(d.motivos.is_empty());
    }

    #[test]
    fn en_contingencia_es_contingencia() {
        let entrada = EntradaRepo {
            en_contingencia: true,
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Contingencia);
    }

    #[test]
    fn huerfano_es_huerfano() {
        let entrada = EntradaRepo {
            huerfano: true,
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Huerfano);
    }

    // --- La corrupción local siempre gana -----------------------------------------------

    #[test]
    fn huerfano_con_fsck_corrupto_es_fallo_por_corrupcion() {
        let entrada = EntradaRepo {
            huerfano: true,
            fsck: Some(ResultadoFsck::Corrupto("objeto dañado".to_string())),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::Corrupcion]);
    }

    #[test]
    fn excluido_con_fsck_corrupto_es_fallo_por_corrupcion() {
        let entrada = EntradaRepo {
            excluido: true,
            fsck: Some(ResultadoFsck::Corrupto("objeto dañado".to_string())),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Fallo);
    }

    #[test]
    fn en_contingencia_con_fsck_corrupto_es_fallo_por_corrupcion() {
        let entrada = EntradaRepo {
            en_contingencia: true,
            fsck: Some(ResultadoFsck::Corrupto("objeto dañado".to_string())),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Fallo);
    }

    #[test]
    fn fsck_corrupto_sin_mas_flags_es_fallo_con_motivo_corrupcion() {
        let entrada = EntradaRepo {
            fsck: Some(ResultadoFsck::Corrupto("objeto dañado".to_string())),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::Corrupcion]);
    }

    #[test]
    fn fsck_ok_no_provoca_fallo_por_si_solo() {
        let entrada = EntradaRepo {
            fsck: Some(ResultadoFsck::Ok),
            local: RepoLocal {
                ultima_sync: Some(hace(1)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    // --- error_lectura: fallo, pero por debajo de excluido/contingencia/huérfano -------

    #[test]
    fn error_lectura_sin_mas_flags_es_fallo() {
        let entrada = EntradaRepo {
            error_lectura: Some("fallo al listar refs".to_string()),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::ErrorLocal]);
    }

    #[test]
    fn error_lectura_no_se_impone_a_excluido() {
        let entrada = EntradaRepo {
            excluido: true,
            error_lectura: Some("fallo al listar refs".to_string()),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Excluido);
    }

    // --- Nunca sincronizado --------------------------------------------------------------

    #[test]
    fn nunca_sincronizado_tras_la_gracia_es_fallo() {
        let entrada = EntradaRepo {
            origen: Some(origen(100, Some("main"))),
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: Some(Duration::minutes(31)),
            ..base()
        };
        let umbrales = Umbrales {
            gracia_inicial: Duration::minutes(30),
            ..Umbrales::default()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::NuncaSincronizado]);
    }

    #[test]
    fn nunca_sincronizado_dentro_de_la_gracia_es_ok() {
        let entrada = EntradaRepo {
            origen: Some(origen(100, Some("main"))),
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: Some(Duration::minutes(10)),
            ..base()
        };
        let umbrales = Umbrales {
            gracia_inicial: Duration::minutes(30),
            ..Umbrales::default()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn nunca_sincronizado_exactamente_en_el_borde_de_la_gracia_no_es_fallo() {
        // «han pasado más de la gracia»: exactamente la gracia no cuenta como «más de».
        let entrada = EntradaRepo {
            origen: Some(origen(100, Some("main"))),
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: Some(Duration::minutes(30)),
            ..base()
        };
        let umbrales = Umbrales {
            gracia_inicial: Duration::minutes(30),
            ..Umbrales::default()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn nunca_sincronizado_sin_antiguedad_conocida_no_se_puede_afirmar_nada() {
        let entrada = EntradaRepo {
            origen: Some(origen(100, Some("main"))),
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: None,
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn mirror_vacio_es_legitimo_si_el_repo_de_github_tambien_lo_esta() {
        let entrada = EntradaRepo {
            origen: Some(origen(0, Some("main"))),
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: Some(Duration::minutes(1000)),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn mirror_vacio_es_legitimo_si_github_no_tiene_rama_por_defecto() {
        let entrada = EntradaRepo {
            origen: Some(origen(100, None)),
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: Some(Duration::minutes(1000)),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    // --- SHA distinto ----------------------------------------------------------------------

    #[test]
    fn sha_distinto_con_sync_antigua_es_fallo() {
        let umbrales = Umbrales {
            obsoleto: Duration::minutes(60),
            ..Umbrales::default()
        };
        let entrada = EntradaRepo {
            sha_github: Some("aaa".to_string()),
            sha_local: Some("bbb".to_string()),
            local: RepoLocal {
                ultima_sync: Some(hace(61)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::ShaDistinto]);
        assert_eq!(d.desfase, Some(Duration::minutes(61)));
    }

    #[test]
    fn sha_distinto_con_sync_reciente_es_ok_pendiente_de_sincronizar() {
        let umbrales = Umbrales {
            obsoleto: Duration::minutes(60),
            ..Umbrales::default()
        };
        let entrada = EntradaRepo {
            sha_github: Some("aaa".to_string()),
            sha_local: Some("bbb".to_string()),
            local: RepoLocal {
                ultima_sync: Some(hace(1)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Ok);
        assert_eq!(d.motivos, vec![Motivo::PendienteDeSincronizar]);
    }

    #[test]
    fn sha_distinto_exactamente_en_el_umbral_se_trata_como_reciente() {
        let umbrales = Umbrales {
            obsoleto: Duration::minutes(60),
            ..Umbrales::default()
        };
        let entrada = EntradaRepo {
            sha_github: Some("aaa".to_string()),
            sha_local: Some("bbb".to_string()),
            local: RepoLocal {
                ultima_sync: Some(hace(60)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn sha_distinto_sin_ultima_sync_conocida_se_trata_como_antigua_por_seguridad() {
        let entrada = EntradaRepo {
            sha_github: Some("aaa".to_string()),
            sha_local: Some("bbb".to_string()),
            local: RepoLocal {
                ultima_sync: None,
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::ShaDistinto]);
        assert_eq!(d.desfase, None);
    }

    #[test]
    fn sha_iguales_no_generan_motivo() {
        let entrada = EntradaRepo {
            sha_github: Some("aaa".to_string()),
            sha_local: Some("aaa".to_string()),
            local: RepoLocal {
                ultima_sync: Some(hace(1)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
        assert!(d.motivos.is_empty());
    }

    #[test]
    fn sha_distinto_manda_sobre_obsoleto() {
        // Con SHA distinto y sincronización antigua, el motivo es ShaDistinto (Fallo), no
        // simplemente Obsoleto: es más grave y más específico.
        let umbrales = Umbrales {
            obsoleto: Duration::minutes(60),
            ..Umbrales::default()
        };
        let entrada = EntradaRepo {
            sha_github: Some("aaa".to_string()),
            sha_local: Some("bbb".to_string()),
            local: RepoLocal {
                ultima_sync: Some(hace(120)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Fallo);
        assert_eq!(d.motivos, vec![Motivo::ShaDistinto]);
    }

    // --- Obsoleto ----------------------------------------------------------------------

    #[test]
    fn sync_mas_antigua_que_el_umbral_es_obsoleto() {
        let umbrales = Umbrales {
            obsoleto: Duration::minutes(60),
            ..Umbrales::default()
        };
        let entrada = EntradaRepo {
            local: RepoLocal {
                ultima_sync: Some(hace(61)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Obsoleto);
        assert_eq!(d.desfase, Some(Duration::minutes(61)));
    }

    #[test]
    fn sync_exactamente_en_el_umbral_no_es_obsoleto() {
        let umbrales = Umbrales {
            obsoleto: Duration::minutes(60),
            ..Umbrales::default()
        };
        let entrada = EntradaRepo {
            local: RepoLocal {
                ultima_sync: Some(hace(60)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &umbrales);
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn respeta_el_minimo_de_sesenta_minutos_al_decidir_obsoleto() {
        // Cuenta con intervalo muy corto: el umbral de obsolescencia no baja de 60 min.
        let umbrales = Umbrales::para_intervalo(5);
        let entrada = EntradaRepo {
            local: RepoLocal {
                ultima_sync: Some(hace(61)),
                ..base().local
            },
            ..base()
        };
        assert_eq!(
            evaluar(&entrada, AHORA, &umbrales).estado,
            EstadoRepo::Obsoleto
        );

        let entrada = EntradaRepo {
            local: RepoLocal {
                ultima_sync: Some(hace(59)),
                ..base().local
            },
            ..base()
        };
        assert_eq!(evaluar(&entrada, AHORA, &umbrales).estado, EstadoRepo::Ok);
    }

    // --- Ok por defecto ------------------------------------------------------------------

    #[test]
    fn sin_ninguna_senal_es_ok() {
        let entrada = EntradaRepo {
            local: RepoLocal {
                ultima_sync: Some(hace(1)),
                ..base().local
            },
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
        assert!(d.motivos.is_empty());
    }

    #[test]
    fn nunca_sincronizado_pero_sin_datos_de_origen_es_ok() {
        // Sin `origen` no se puede saber si el mirror debería tener contenido: no se
        // afirma nada.
        let entrada = EntradaRepo {
            local: RepoLocal {
                vacio: true,
                ultima_sync: None,
                ..base().local
            },
            antiguedad_conocida: Some(Duration::minutes(1000)),
            ..base()
        };
        let d = evaluar(&entrada, AHORA, &Umbrales::default());
        assert_eq!(d.estado, EstadoRepo::Ok);
    }

    #[test]
    fn el_diagnostico_conserva_el_id_del_repo_local() {
        let d = evaluar(&base(), AHORA, &Umbrales::default());
        assert_eq!(d.id, id());
    }
}
