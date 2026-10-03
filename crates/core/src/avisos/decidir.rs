//! [`decidir`]: la función pura del módulo `avisos`. Sin E/S: a partir de lo que ha
//! pasado en una pasada y de lo que ya se
//! sabía, decide qué notificar y cómo queda el estado para la próxima vez.

use std::collections::{BTreeMap, BTreeSet};

use time::{Duration, OffsetDateTime};

use crate::cuentas::CambioDetectado;
use crate::modelo::{EstadoRepo, IdRepo};
use crate::snapshots::CambioDestructivo;
use crate::sync::{AlertaPlan, ResultadoAccion};
use crate::verificacion::{Aviso as AvisoVerificacion, DIAS_AVISO_POR_DEFECTO, aviso_caducidad};

use super::estado::{EntradaFallo, EstadoAvisos};
use super::modelo::{CambioAviso, EntradaAvisos, Notificacion, TextoAviso, TipoFallo, Urgencia};
use super::saneado::sanear_texto;

/// Tras cuánto tiempo sin avisar se repite el aviso de un fallo de repo que persiste.
const UMBRAL_REAVISO_FALLO: Duration = Duration::hours(24);
/// Cuánto se espera entre avisos de caducidad del token (próxima o ya ocurrida).
const UMBRAL_AVISO_TOKEN: Duration = Duration::hours(24);
/// Cuánto se espera entre avisos de que Gitea no responde.
const UMBRAL_AVISO_GITEA_PARADO: Duration = Duration::hours(6);
/// Por encima de cuántos repos afectados se agrega en una sola notificación en vez de
/// mandar una por repo.
const UMBRAL_AGREGACION: usize = 3;
/// Longitud máxima de un fragmento de texto libre (nombre de repo, mensaje de error)
/// dentro del cuerpo de una notificación.
const LONGITUD_MAXIMA_FRAGMENTO: usize = 200;

/// Estado de la caducidad del token, ya evaluado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EstadoToken {
    Caducado,
    CaducaPronto { dias: u32 },
}

/// Decide qué [`Notificacion`] mandar a partir de `entrada` y de lo que ya se sabía por
/// `previo`, y devuelve el nuevo [`EstadoAvisos`] a persistir.
///
/// Reglas:
/// - Un fallo nuevo de un repo (que no estuviera ya en `previo`) se notifica. El mismo
///   fallo del mismo repo no se repite en cada pasada; se reavisa si persiste más de 24 h
///   desde el último aviso.
/// - Un repo que fallaba y ha dejado de aparecer entre los fallos actuales se notifica
///   como recuperado, una sola vez (no vuelve a aparecer en el estado devuelto).
/// - Si sync o verificación no se han ejecutado en esta pasada (`None`), los fallos ya
///   conocidos de ese origen se conservan tal cual: ni se notifican de nuevo ni se dan
///   por recuperados sin datos frescos.
/// - Con más de tres repos afectados (fallo nuevo/reaviso, o recuperación) en la misma
///   pasada, se manda una única notificación agregada en vez de una por repo.
/// - La alerta del plan (`AlertaPlan::DemasiadosHuerfanos`) siempre se notifica, con
///   urgencia crítica, sin dedupe.
/// - El token que caduca en pocos días avisa como mucho una vez al día; ya caducado es
///   crítico (mismo límite de una vez al día, para no inundar mientras nadie lo renueva).
/// - Gitea parado es crítico, como mucho una vez cada 6 horas.
///
/// Los nombres de repositorio y los mensajes de error incluidos en el cuerpo de una
/// notificación pasan por [`sanear_texto`]: vienen de GitHub/Gitea y no son de confianza.
pub fn decidir(
    entrada: &EntradaAvisos,
    previo: &EstadoAvisos,
    ahora: OffsetDateTime,
) -> (Vec<Notificacion>, EstadoAvisos) {
    let login = sanear_texto(entrada.login.as_str(), LONGITUD_MAXIMA_FRAGMENTO);
    let mut notificaciones = Vec::new();

    // --- Alerta del plan: demasiados huérfanos. Siempre, sin dedupe. -----------------
    if let Some(alerta) = entrada.sync.as_ref().and_then(|s| s.alerta.clone()) {
        notificaciones.push(notificacion_huerfanos(&login, &alerta));
    }

    // --- Cambios destructivos detectados en los snapshots: siempre, crítico, sin ------
    // throttle. No hace falta dedupe explícito: cada pasada solo detecta lo que cambia
    // frente a la captura inmediatamente anterior (ver `cuentas::proteccion`), así que
    // el mismo cambio no vuelve a aparecer en `proteccion.cambios` en pasadas siguientes.
    if let Some(proteccion) = &entrada.proteccion {
        for cambio in &proteccion.cambios {
            notificaciones.push(notificacion_cambio_destructivo(&login, cambio));
        }
    }

    // --- Gitea parado: crítico, como mucho cada 6 h. ----------------------------------
    let ultimo_aviso_gitea_parado = if entrada.gitea_parado {
        if throttle_vencido(
            previo.ultimo_aviso_gitea_parado,
            ahora,
            UMBRAL_AVISO_GITEA_PARADO,
        ) {
            notificaciones.push(notificacion_gitea_parado(&login));
            Some(ahora)
        } else {
            previo.ultimo_aviso_gitea_parado
        }
    } else {
        // Se sabe que Gitea responde: se resetea para que, si vuelve a pararse, avise
        // de inmediato en vez de arrastrar un aviso antiguo.
        None
    };

    // --- Caducidad del token: caducado (crítico) o próxima (normal), 1 vez al día. ----
    let ultimo_aviso_token = match calcular_estado_token(entrada, ahora) {
        None => previo.ultimo_aviso_token, // sin datos frescos esta pasada: no se toca
        Some(None) => None,                // comprobado: sin problema, se resetea
        Some(Some(estado)) => {
            if throttle_vencido(previo.ultimo_aviso_token, ahora, UMBRAL_AVISO_TOKEN) {
                notificaciones.push(notificacion_token(&login, estado));
                Some(ahora)
            } else {
                previo.ultimo_aviso_token
            }
        }
    };

    // --- Fallos de repos: dedupe, reaviso a 24 h, recuperación, agregación. -----------
    let mensajes_error = mensajes_error_por_repo(entrada);
    let mut fallos_salida = Vec::new();
    let mut afectados: BTreeSet<IdRepo> = BTreeSet::new();
    let mut recuperados: BTreeSet<IdRepo> = BTreeSet::new();

    for tipo in [TipoFallo::Sincronizacion, TipoFallo::Verificacion] {
        let actuales = repos_en_fallo(entrada, tipo);
        procesar_tipo(
            tipo,
            actuales.as_ref(),
            &previo.fallos,
            ahora,
            &mut fallos_salida,
            &mut afectados,
            &mut recuperados,
        );
    }

    if !afectados.is_empty() {
        if afectados.len() > UMBRAL_AGREGACION {
            notificaciones.push(notificacion_fallos_agregados(&login, afectados.len()));
        } else {
            for id in &afectados {
                notificaciones.push(notificacion_fallo(&login, id, &mensajes_error));
            }
        }
    }
    if !recuperados.is_empty() {
        if recuperados.len() > UMBRAL_AGREGACION {
            notificaciones.push(notificacion_recuperados_agregados(
                &login,
                recuperados.len(),
            ));
        } else {
            for id in &recuperados {
                notificaciones.push(notificacion_recuperado(&login, id));
            }
        }
    }

    let nuevo_estado = EstadoAvisos {
        fallos: fallos_salida,
        ultimo_aviso_token,
        ultimo_aviso_gitea_parado,
    };

    (notificaciones, nuevo_estado)
}

/// `true` si ha pasado más de `minimo` desde `ultimo` (o si nunca se avisó).
fn throttle_vencido(
    ultimo: Option<OffsetDateTime>,
    ahora: OffsetDateTime,
    minimo: Duration,
) -> bool {
    ultimo.is_none_or(|u| ahora - u > minimo)
}

/// Calcula el estado de la caducidad del token.
///
/// - `None`: sin datos frescos esta pasada (ni `verificacion` ni `sync`); no se debe
///   tocar el estado ya guardado.
/// - `Some(None)`: se ha comprobado y no hay ningún problema con el token.
/// - `Some(Some(_))`: se ha comprobado y el token caduca pronto o ya ha caducado.
fn calcular_estado_token(
    entrada: &EntradaAvisos,
    ahora: OffsetDateTime,
) -> Option<Option<EstadoToken>> {
    if let Some(verificacion) = &entrada.verificacion {
        for aviso in &verificacion.avisos {
            match aviso {
                AvisoVerificacion::TokenCaducado => return Some(Some(EstadoToken::Caducado)),
                AvisoVerificacion::TokenCaducaPronto { dias } => {
                    return Some(Some(EstadoToken::CaducaPronto { dias: *dias }));
                }
                _ => {}
            }
        }
        return Some(None);
    }
    let sync = entrada.sync.as_ref()?;
    let estado = match aviso_caducidad(sync.caduca_token, ahora, DIAS_AVISO_POR_DEFECTO) {
        Some(AvisoVerificacion::TokenCaducado) => Some(EstadoToken::Caducado),
        Some(AvisoVerificacion::TokenCaducaPronto { dias }) => {
            Some(EstadoToken::CaducaPronto { dias })
        }
        _ => None,
    };
    Some(estado)
}

/// Repos en fallo de `tipo` en esta pasada, o `None` si no hay datos frescos de ese tipo.
fn repos_en_fallo(entrada: &EntradaAvisos, tipo: TipoFallo) -> Option<BTreeSet<IdRepo>> {
    match tipo {
        TipoFallo::Sincronizacion => entrada.sync.as_ref().map(|informe| {
            informe
                .resultados
                .iter()
                .filter(|r| matches!(r.resultado, ResultadoAccion::Error(_)))
                .map(|r| r.id.clone())
                .collect()
        }),
        TipoFallo::Verificacion => entrada.verificacion.as_ref().map(|informe| {
            informe
                .diagnosticos
                .iter()
                .filter(|d| d.estado == EstadoRepo::Fallo)
                .map(|d| d.id.clone())
                .collect()
        }),
    }
}

/// Aplica las reglas de dedupe/reaviso/recuperación de `tipo`, añadiendo lo que
/// corresponda a `salida` (estado a conservar), `afectados` (a notificar como fallo) y
/// `recuperados` (a notificar como recuperación).
#[allow(clippy::too_many_arguments)]
fn procesar_tipo(
    tipo: TipoFallo,
    actuales: Option<&BTreeSet<IdRepo>>,
    previos: &[EntradaFallo],
    ahora: OffsetDateTime,
    salida: &mut Vec<EntradaFallo>,
    afectados: &mut BTreeSet<IdRepo>,
    recuperados: &mut BTreeSet<IdRepo>,
) {
    let previos_de_este_tipo: Vec<&EntradaFallo> =
        previos.iter().filter(|e| e.tipo == tipo).collect();

    let Some(actuales) = actuales else {
        // Sin datos frescos de este tipo en esta pasada: se conserva tal cual, sin
        // notificar ni fallo ni recuperación.
        salida.extend(previos_de_este_tipo.into_iter().cloned());
        return;
    };

    let mut ya_conocidos: BTreeSet<IdRepo> = BTreeSet::new();
    for previo in previos_de_este_tipo {
        ya_conocidos.insert(previo.id.clone());
        if actuales.contains(&previo.id) {
            if ahora - previo.ultimo_aviso > UMBRAL_REAVISO_FALLO {
                afectados.insert(previo.id.clone());
                salida.push(EntradaFallo {
                    id: previo.id.clone(),
                    tipo,
                    primera_vez: previo.primera_vez,
                    ultimo_aviso: ahora,
                });
            } else {
                salida.push(previo.clone());
            }
        } else {
            recuperados.insert(previo.id.clone());
        }
    }

    for id in actuales {
        if !ya_conocidos.contains(id) {
            afectados.insert(id.clone());
            salida.push(EntradaFallo {
                id: id.clone(),
                tipo,
                primera_vez: ahora,
                ultimo_aviso: ahora,
            });
        }
    }
}

/// Mensajes de error puntuales por repo que trae la verificación
/// ([`AvisoVerificacion::ErrorRepo`]), para enriquecer la notificación individual de un
/// fallo (nunca la agregada). Es la vía por la que texto libre y no fiable llega al
/// cuerpo de una notificación, así que siempre pasa por [`sanear_texto`] al usarse.
fn mensajes_error_por_repo(entrada: &EntradaAvisos) -> BTreeMap<IdRepo, String> {
    let mut mapa = BTreeMap::new();
    if let Some(verificacion) = &entrada.verificacion {
        for aviso in &verificacion.avisos {
            if let AvisoVerificacion::ErrorRepo { id, mensaje } = aviso {
                mapa.insert(id.clone(), mensaje.clone());
            }
        }
    }
    mapa
}

fn notificacion_fallo(
    login: &str,
    id: &IdRepo,
    mensajes: &BTreeMap<IdRepo, String>,
) -> Notificacion {
    let id_saneado = sanear_texto(&id.to_string(), LONGITUD_MAXIMA_FRAGMENTO);
    let detalle = mensajes
        .get(id)
        .map(|mensaje| sanear_texto(mensaje, LONGITUD_MAXIMA_FRAGMENTO))
        .filter(|detalle| !detalle.is_empty());
    Notificacion {
        titulo: TextoAviso::TituloFalloSincronizacion,
        cuerpo: TextoAviso::FalloRepositorio {
            login: login.to_string(),
            repo: id_saneado.clone(),
            detalle,
        },
        urgencia: Urgencia::Normal,
        clave_dedupe: format!("fallo:{login}:{id_saneado}"),
    }
}

fn notificacion_fallos_agregados(login: &str, n: usize) -> Notificacion {
    Notificacion {
        titulo: TextoAviso::TituloVariosFallos,
        cuerpo: TextoAviso::FallosAgregados {
            login: login.to_string(),
            n,
        },
        urgencia: Urgencia::Normal,
        clave_dedupe: format!("fallos-agregados:{login}"),
    }
}

fn notificacion_recuperado(login: &str, id: &IdRepo) -> Notificacion {
    let id_saneado = sanear_texto(&id.to_string(), LONGITUD_MAXIMA_FRAGMENTO);
    Notificacion {
        titulo: TextoAviso::TituloRepositorioRecuperado,
        cuerpo: TextoAviso::RepositorioRecuperado {
            login: login.to_string(),
            repo: id_saneado.clone(),
        },
        urgencia: Urgencia::Baja,
        clave_dedupe: format!("recuperado:{login}:{id_saneado}"),
    }
}

fn notificacion_recuperados_agregados(login: &str, n: usize) -> Notificacion {
    Notificacion {
        titulo: TextoAviso::TituloVariosRecuperados,
        cuerpo: TextoAviso::RecuperadosAgregados {
            login: login.to_string(),
            n,
        },
        urgencia: Urgencia::Baja,
        clave_dedupe: format!("recuperados-agregados:{login}"),
    }
}

fn notificacion_huerfanos(login: &str, alerta: &AlertaPlan) -> Notificacion {
    let AlertaPlan::DemasiadosHuerfanos {
        candidatos,
        total_mirrors,
        listado_github_vacio,
    } = alerta;
    let cuerpo = if *listado_github_vacio {
        TextoAviso::HuerfanosListaVacia {
            login: login.to_string(),
        }
    } else {
        TextoAviso::HuerfanosCandidatos {
            login: login.to_string(),
            candidatos: *candidatos,
            total_mirrors: *total_mirrors,
        }
    };
    Notificacion {
        titulo: TextoAviso::TituloHuerfanos,
        cuerpo,
        urgencia: Urgencia::Critica,
        clave_dedupe: format!("plan-huerfanos:{login}"),
    }
}

/// Notificación crítica de un cambio destructivo ya detectado y protegido en los
/// snapshots. Nunca se throttla: cada una representa un cambio
/// real y distinto (ver el comentario en [`decidir`]).
fn notificacion_cambio_destructivo(login: &str, cambio: &CambioDetectado) -> Notificacion {
    let id_saneado = sanear_texto(&cambio.id.to_string(), LONGITUD_MAXIMA_FRAGMENTO);
    let marca_saneada = sanear_texto(&cambio.marca_protegida, LONGITUD_MAXIMA_FRAGMENTO);
    let cambios = cambio.cambios.iter().map(cambio_aviso).collect();
    Notificacion {
        titulo: TextoAviso::TituloHistoriaReescrita,
        cuerpo: TextoAviso::CambioDestructivo {
            login: login.to_string(),
            repo: id_saneado.clone(),
            cambios,
        },
        urgencia: Urgencia::Critica,
        clave_dedupe: format!("snapshots-cambio:{login}:{id_saneado}:{marca_saneada}"),
    }
}

fn cambio_aviso(cambio: &CambioDestructivo) -> CambioAviso {
    let sanear = |texto: &str| sanear_texto(texto, LONGITUD_MAXIMA_FRAGMENTO);
    match cambio {
        CambioDestructivo::HistoriaReescrita { rama, .. } => {
            CambioAviso::HistoriaReescrita { rama: sanear(rama) }
        }
        CambioDestructivo::RamaBorrada { rama } => CambioAviso::RamaBorrada { rama: sanear(rama) },
        CambioDestructivo::TagBorrado { tag } => CambioAviso::TagBorrado { tag: sanear(tag) },
        CambioDestructivo::TagMovido { tag, .. } => CambioAviso::TagMovido { tag: sanear(tag) },
    }
}

fn notificacion_gitea_parado(login: &str) -> Notificacion {
    Notificacion {
        titulo: TextoAviso::TituloGiteaParado,
        cuerpo: TextoAviso::GiteaParado {
            login: login.to_string(),
        },
        urgencia: Urgencia::Critica,
        clave_dedupe: format!("gitea-parado:{login}"),
    }
}

fn notificacion_token(login: &str, estado: EstadoToken) -> Notificacion {
    let login_texto = login.to_string();
    match estado {
        EstadoToken::Caducado => Notificacion {
            titulo: TextoAviso::TituloTokenCaducado,
            cuerpo: TextoAviso::TokenCaducado { login: login_texto },
            urgencia: Urgencia::Critica,
            clave_dedupe: format!("token-caducado:{login}"),
        },
        EstadoToken::CaducaPronto { dias } => Notificacion {
            titulo: TextoAviso::TituloTokenCaducaPronto,
            cuerpo: TextoAviso::TokenCaducaPronto {
                login: login_texto,
                dias,
            },
            urgencia: Urgencia::Normal,
            clave_dedupe: format!("token-caduca-pronto:{login}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::cuentas::InformeProteccion;
    use crate::idioma::{Idioma, Localizable};
    use crate::modelo::{Alcance, Cuenta, Nombre};
    use crate::sync::{InformeSync, Plan, ResultadoRepo, TipoAccion};
    use crate::verificacion::{Diagnostico, InformeVerificacion};

    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    fn cuenta(login: &str) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: PathBuf::from("/tmp/gitmereba-test-avisos"),
            puerto: 33000,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    const AHORA: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn hace(horas: i64) -> OffsetDateTime {
        AHORA - Duration::hours(horas)
    }

    fn sync_vacio(login: &str) -> InformeSync {
        InformeSync {
            cuenta: cuenta(login),
            inicio: AHORA,
            fin: AHORA,
            caduca_token: None,
            plan: Plan::default(),
            resultados: Vec::new(),
            errores_de_listado: Vec::new(),
            alerta: None,
            origen: Vec::new(),
        }
    }

    fn con_fallo_sync(mut s: InformeSync, repo: &str) -> InformeSync {
        s.resultados.push(ResultadoRepo {
            id: id(&s.cuenta.login.to_string(), repo),
            accion: TipoAccion::CrearMirror,
            resultado: ResultadoAccion::Error("boom".to_string()),
        });
        s
    }

    fn verificacion_vacia(login: &str) -> InformeVerificacion {
        InformeVerificacion {
            cuenta: cuenta(login),
            momento: AHORA,
            diagnosticos: Vec::new(),
            consultas_sha: 0,
            fsck_hechos: 0,
            limite_api_alcanzado: false,
            avisos: Vec::new(),
        }
    }

    fn con_fallo_verificacion(mut v: InformeVerificacion, repo: &str) -> InformeVerificacion {
        let login = v.cuenta.login.to_string();
        v.diagnosticos.push(Diagnostico {
            id: id(&login, repo),
            estado: EstadoRepo::Fallo,
            motivos: Vec::new(),
            desfase: None,
        });
        v
    }

    fn entrada_avisos(
        login: &str,
        sync: Option<InformeSync>,
        verificacion: Option<InformeVerificacion>,
        gitea_parado: bool,
    ) -> EntradaAvisos {
        EntradaAvisos {
            login: nombre(login),
            sync,
            verificacion,
            gitea_parado,
            proteccion: None,
        }
    }

    fn previo_con_token(momento: OffsetDateTime) -> EstadoAvisos {
        EstadoAvisos {
            ultimo_aviso_token: Some(momento),
            ..EstadoAvisos::default()
        }
    }

    fn previo_con_gitea_parado(momento: OffsetDateTime) -> EstadoAvisos {
        EstadoAvisos {
            ultimo_aviso_gitea_parado: Some(momento),
            ..EstadoAvisos::default()
        }
    }

    fn entrada_fallo(
        repo: &str,
        tipo: TipoFallo,
        primera_vez: OffsetDateTime,
        ultimo_aviso: OffsetDateTime,
    ) -> EntradaFallo {
        EntradaFallo {
            id: id("jparga", repo),
            tipo,
            primera_vez,
            ultimo_aviso,
        }
    }

    // --- Caso base: nada que avisar -----------------------------------------------------

    #[test]
    fn sin_nada_que_avisar_no_hay_notificaciones_ni_estado() {
        let entrada = entrada_avisos(
            "jparga",
            Some(sync_vacio("jparga")),
            Some(verificacion_vacia("jparga")),
            false,
        );
        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);
        assert!(notificaciones.is_empty());
        assert_eq!(estado, EstadoAvisos::default());
    }

    // --- Fallo nuevo: dedupe y reaviso a 24 h -------------------------------------------

    #[test]
    fn un_fallo_nuevo_de_sincronizacion_se_notifica() {
        let sync = con_fallo_sync(sync_vacio("jparga"), "repo1");
        let entrada = entrada_avisos("jparga", Some(sync), None, false);
        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Normal);
        assert_eq!(
            notificaciones[0].cuerpo,
            TextoAviso::FalloRepositorio {
                login: "jparga".to_string(),
                repo: "jparga/repo1".to_string(),
                detalle: None,
            }
        );
        assert_eq!(estado.fallos.len(), 1);
        assert_eq!(estado.fallos[0].id, id("jparga", "repo1"));
        assert_eq!(estado.fallos[0].tipo, TipoFallo::Sincronizacion);
        assert_eq!(estado.fallos[0].primera_vez, AHORA);
        assert_eq!(estado.fallos[0].ultimo_aviso, AHORA);
    }

    #[test]
    fn un_fallo_nuevo_de_verificacion_se_notifica() {
        let verificacion = con_fallo_verificacion(verificacion_vacia("jparga"), "repo1");
        let entrada = entrada_avisos("jparga", None, Some(verificacion), false);
        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(estado.fallos[0].tipo, TipoFallo::Verificacion);
    }

    #[test]
    fn el_mismo_fallo_dentro_de_24h_no_se_repite() {
        let sync = con_fallo_sync(sync_vacio("jparga"), "repo1");
        let entrada = entrada_avisos("jparga", Some(sync), None, false);
        let mut previo = EstadoAvisos::default();
        previo.fallos.push(entrada_fallo(
            "repo1",
            TipoFallo::Sincronizacion,
            hace(10),
            AHORA - Duration::hours(2),
        ));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert!(notificaciones.is_empty(), "{notificaciones:?}");
        // El estado se conserva exactamente igual (mismo `ultimo_aviso`).
        assert_eq!(estado.fallos, previo.fallos);
    }

    #[test]
    fn el_mismo_fallo_exactamente_a_las_24h_no_reavisa_todavia() {
        let sync = con_fallo_sync(sync_vacio("jparga"), "repo1");
        let entrada = entrada_avisos("jparga", Some(sync), None, false);
        let mut previo = EstadoAvisos::default();
        previo.fallos.push(entrada_fallo(
            "repo1",
            TipoFallo::Sincronizacion,
            hace(30),
            AHORA - UMBRAL_REAVISO_FALLO,
        ));

        let (notificaciones, _estado) = decidir(&entrada, &previo, AHORA);

        assert!(notificaciones.is_empty());
    }

    #[test]
    fn el_mismo_fallo_tras_mas_de_24h_reavisa_y_conserva_la_primera_vez() {
        let sync = con_fallo_sync(sync_vacio("jparga"), "repo1");
        let entrada = entrada_avisos("jparga", Some(sync), None, false);
        let primera_vez = hace(48);
        let mut previo = EstadoAvisos::default();
        previo.fallos.push(entrada_fallo(
            "repo1",
            TipoFallo::Sincronizacion,
            primera_vez,
            AHORA - UMBRAL_REAVISO_FALLO - Duration::minutes(1),
        ));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(estado.fallos[0].primera_vez, primera_vez);
        assert_eq!(estado.fallos[0].ultimo_aviso, AHORA);
    }

    // --- Recuperación --------------------------------------------------------------------

    #[test]
    fn un_repo_que_deja_de_fallar_se_notifica_como_recuperado_y_desaparece_del_estado() {
        let entrada = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        let mut previo = EstadoAvisos::default();
        previo.fallos.push(entrada_fallo(
            "repo1",
            TipoFallo::Sincronizacion,
            hace(10),
            hace(10),
        ));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Baja);
        assert_eq!(
            notificaciones[0].titulo,
            TextoAviso::TituloRepositorioRecuperado
        );
        assert!(estado.fallos.is_empty());
    }

    #[test]
    fn la_recuperacion_no_se_repite_en_la_pasada_siguiente() {
        let entrada = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        // Ya no hay ningún fallo en `previo` (se recuperó en la pasada anterior).
        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);
        assert!(notificaciones.is_empty());
        assert!(estado.fallos.is_empty());
    }

    // --- Agregación ------------------------------------------------------------------------

    #[test]
    fn mas_de_tres_fallos_nuevos_se_agregan_en_una_sola_notificacion() {
        let mut sync = sync_vacio("jparga");
        for n in 1..=4 {
            sync = con_fallo_sync(sync, &format!("repo{n}"));
        }
        let entrada = entrada_avisos("jparga", Some(sync), None, false);

        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1, "{notificaciones:?}");
        assert_eq!(
            notificaciones[0].cuerpo,
            TextoAviso::FallosAgregados {
                login: "jparga".to_string(),
                n: 4
            }
        );
        // El estado, en cambio, registra los cuatro fallos individualmente.
        assert_eq!(estado.fallos.len(), 4);
    }

    #[test]
    fn tres_fallos_o_menos_no_se_agregan() {
        let mut sync = sync_vacio("jparga");
        for n in 1..=3 {
            sync = con_fallo_sync(sync, &format!("repo{n}"));
        }
        let entrada = entrada_avisos("jparga", Some(sync), None, false);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 3);
    }

    // --- Alerta del plan: siempre, urgencia crítica ----------------------------------------

    #[test]
    fn la_alerta_de_demasiados_huerfanos_siempre_se_notifica_como_critica() {
        let mut sync = sync_vacio("jparga");
        sync.alerta = Some(AlertaPlan::DemasiadosHuerfanos {
            candidatos: 5,
            total_mirrors: 6,
            listado_github_vacio: false,
        });
        let entrada = entrada_avisos("jparga", Some(sync), None, false);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Critica);
        assert!(matches!(
            notificaciones[0].cuerpo,
            TextoAviso::HuerfanosCandidatos { candidatos: 5, .. }
        ));
    }

    #[test]
    fn la_alerta_de_huerfanos_se_repite_en_pasadas_consecutivas() {
        let mut sync = sync_vacio("jparga");
        sync.alerta = Some(AlertaPlan::DemasiadosHuerfanos {
            candidatos: 5,
            total_mirrors: 6,
            listado_github_vacio: true,
        });
        let entrada = entrada_avisos("jparga", Some(sync), None, false);

        let (primera, estado_tras_primera) = decidir(&entrada, &EstadoAvisos::default(), AHORA);
        let (segunda, _) = decidir(&entrada, &estado_tras_primera, AHORA);

        assert_eq!(primera.len(), 1);
        assert_eq!(segunda.len(), 1);
        assert_eq!(
            segunda[0].cuerpo,
            TextoAviso::HuerfanosListaVacia {
                login: "jparga".to_string()
            }
        );
    }

    // --- Cambios destructivos en snapshots: siempre crítico, no se repite entre pasadas ---

    fn cambio_detectado(repo: &str, marca: &str) -> CambioDetectado {
        CambioDetectado {
            id: id("jparga", repo),
            marca_protegida: marca.to_string(),
            cambios: vec![CambioDestructivo::HistoriaReescrita {
                rama: "main".to_string(),
                antes: "a".repeat(40),
                ahora: "b".repeat(40),
            }],
        }
    }

    #[test]
    fn un_cambio_destructivo_se_notifica_como_critico() {
        let proteccion = InformeProteccion {
            capturas_nuevas: 1,
            cambios: vec![cambio_detectado("repo1", "20260101T000000Z")],
            errores: Vec::new(),
        };
        let mut entrada = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        entrada.proteccion = Some(proteccion);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1, "{notificaciones:?}");
        assert_eq!(notificaciones[0].urgencia, Urgencia::Critica);
        assert_eq!(
            notificaciones[0].cuerpo,
            TextoAviso::CambioDestructivo {
                login: "jparga".to_string(),
                repo: "jparga/repo1".to_string(),
                cambios: vec![CambioAviso::HistoriaReescrita {
                    rama: "main".to_string()
                }],
            }
        );
    }

    #[test]
    fn el_cambio_destructivo_no_se_repite_si_la_pasada_siguiente_no_trae_ninguno() {
        // `proteccion.cambios` solo contiene lo detectado EN ESA pasada (comparado con la
        // captura inmediatamente anterior): una pasada siguiente sin cambios no debe
        // volver a avisar de lo mismo, aunque el estado de avisos no lleve ningún rastro
        // de él (no hace falta: la propia construcción de `InformeProteccion` ya lo
        // garantiza).
        let proteccion = InformeProteccion {
            capturas_nuevas: 1,
            cambios: vec![cambio_detectado("repo1", "20260101T000000Z")],
            errores: Vec::new(),
        };
        let mut primera = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        primera.proteccion = Some(proteccion);
        let (notificaciones_primera, estado) = decidir(&primera, &EstadoAvisos::default(), AHORA);
        assert_eq!(notificaciones_primera.len(), 1);

        let mut segunda = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        segunda.proteccion = Some(InformeProteccion::default());
        let (notificaciones_segunda, _) = decidir(&segunda, &estado, AHORA);

        assert!(
            notificaciones_segunda.is_empty(),
            "{notificaciones_segunda:?}"
        );
    }

    #[test]
    fn varios_cambios_destructivos_en_la_misma_pasada_generan_una_notificacion_cada_uno() {
        let proteccion = InformeProteccion {
            capturas_nuevas: 2,
            cambios: vec![
                cambio_detectado("repo1", "20260101T000000Z"),
                cambio_detectado("repo2", "20260101T000000Z"),
            ],
            errores: Vec::new(),
        };
        let mut entrada = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        entrada.proteccion = Some(proteccion);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 2, "{notificaciones:?}");
        assert!(
            notificaciones
                .iter()
                .all(|n| n.urgencia == Urgencia::Critica)
        );
    }

    // --- Token: caduca pronto (1/día), caducado (crítico), reset al resolverse ------------

    #[test]
    fn token_caduca_pronto_notifica_con_urgencia_normal() {
        let mut verificacion = verificacion_vacia("jparga");
        verificacion
            .avisos
            .push(AvisoVerificacion::TokenCaducaPronto { dias: 3 });
        let entrada = entrada_avisos("jparga", None, Some(verificacion), false);

        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Normal);
        assert_eq!(
            notificaciones[0].cuerpo,
            TextoAviso::TokenCaducaPronto {
                login: "jparga".to_string(),
                dias: 3
            }
        );
        assert_eq!(estado.ultimo_aviso_token, Some(AHORA));
    }

    #[test]
    fn token_caduca_pronto_no_repite_dentro_de_24h() {
        let mut verificacion = verificacion_vacia("jparga");
        verificacion
            .avisos
            .push(AvisoVerificacion::TokenCaducaPronto { dias: 3 });
        let entrada = entrada_avisos("jparga", None, Some(verificacion), false);
        let previo = previo_con_token(hace(2));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert!(notificaciones.is_empty());
        assert_eq!(estado.ultimo_aviso_token, previo.ultimo_aviso_token);
    }

    #[test]
    fn token_caduca_pronto_repite_tras_mas_de_24h() {
        let mut verificacion = verificacion_vacia("jparga");
        verificacion
            .avisos
            .push(AvisoVerificacion::TokenCaducaPronto { dias: 1 });
        let entrada = entrada_avisos("jparga", None, Some(verificacion), false);
        let previo = previo_con_token(hace(25));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(estado.ultimo_aviso_token, Some(AHORA));
    }

    #[test]
    fn token_caducado_es_critico() {
        let mut verificacion = verificacion_vacia("jparga");
        verificacion.avisos.push(AvisoVerificacion::TokenCaducado);
        let entrada = entrada_avisos("jparga", None, Some(verificacion), false);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Critica);
    }

    #[test]
    fn el_token_resuelto_resetea_el_estado() {
        // Verificación sin ningún aviso de token: el problema se ha resuelto (renovado).
        let entrada = entrada_avisos("jparga", None, Some(verificacion_vacia("jparga")), false);
        let previo = previo_con_token(hace(1));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert!(notificaciones.is_empty());
        assert_eq!(estado.ultimo_aviso_token, None);
    }

    #[test]
    fn calcula_la_caducidad_a_partir_del_sync_si_no_hay_verificacion() {
        let mut sync = sync_vacio("jparga");
        sync.caduca_token = Some(AHORA - Duration::hours(1)); // ya caducado
        let entrada = entrada_avisos("jparga", Some(sync), None, false);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Critica);
    }

    // --- Gitea parado: crítico, cada 6 h, reset al volver ---------------------------------

    #[test]
    fn gitea_parado_notifica_como_critico() {
        let entrada = entrada_avisos("jparga", None, None, true);
        let (notificaciones, estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(notificaciones[0].urgencia, Urgencia::Critica);
        assert_eq!(estado.ultimo_aviso_gitea_parado, Some(AHORA));
    }

    #[test]
    fn gitea_parado_no_repite_dentro_de_6h() {
        let entrada = entrada_avisos("jparga", None, None, true);
        let previo = previo_con_gitea_parado(hace(3));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert!(notificaciones.is_empty());
        assert_eq!(
            estado.ultimo_aviso_gitea_parado,
            previo.ultimo_aviso_gitea_parado
        );
    }

    #[test]
    fn gitea_parado_repite_tras_mas_de_6h() {
        let entrada = entrada_avisos("jparga", None, None, true);
        let previo = previo_con_gitea_parado(hace(7));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert_eq!(notificaciones.len(), 1);
        assert_eq!(estado.ultimo_aviso_gitea_parado, Some(AHORA));
    }

    #[test]
    fn gitea_que_vuelve_a_responder_resetea_el_estado() {
        let entrada = entrada_avisos("jparga", Some(sync_vacio("jparga")), None, false);
        let previo = previo_con_gitea_parado(hace(1));

        let (_notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        assert_eq!(estado.ultimo_aviso_gitea_parado, None);
    }

    #[test]
    fn gitea_parado_no_toca_los_fallos_de_repos_ya_conocidos() {
        let entrada = entrada_avisos("jparga", None, None, true);
        let mut previo = EstadoAvisos::default();
        previo.fallos.push(entrada_fallo(
            "repo1",
            TipoFallo::Sincronizacion,
            hace(100),
            hace(100),
        ));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        // Solo la notificación de "Gitea parado", ninguna de fallo ni de recuperación.
        assert_eq!(notificaciones.len(), 1);
        assert_eq!(estado.fallos, previo.fallos);
    }

    // --- Sin datos frescos de un tipo: no se toca ese tipo --------------------------------

    #[test]
    fn sin_verificacion_en_esta_pasada_no_se_tocan_los_fallos_de_verificacion() {
        // Solo llega `sync` (con un fallo de otro repo); `verificacion` es `None`.
        let sync = con_fallo_sync(sync_vacio("jparga"), "repo-sync");
        let entrada = entrada_avisos("jparga", Some(sync), None, false);
        let mut previo = EstadoAvisos::default();
        previo.fallos.push(entrada_fallo(
            "repo-verificacion",
            TipoFallo::Verificacion,
            hace(48),
            hace(48), // llevaría rato para reavisar si se evaluara, pero no debe tocarse
        ));

        let (notificaciones, estado) = decidir(&entrada, &previo, AHORA);

        // Solo se notifica el fallo nuevo de sincronización.
        assert_eq!(notificaciones.len(), 1);
        assert!(matches!(
            &notificaciones[0].cuerpo,
            TextoAviso::FalloRepositorio { repo, .. } if repo.contains("repo-sync")
        ));
        // El fallo de verificación se conserva exactamente igual: ni reaviso ni
        // recuperación sin datos frescos de ese tipo.
        assert!(
            estado
                .fallos
                .iter()
                .any(|f| f.id == id("jparga", "repo-verificacion")
                    && f.tipo == TipoFallo::Verificacion
                    && f.ultimo_aviso == hace(48))
        );
    }

    // --- Saneado de texto no fiable ---------------------------------------------------------

    #[test]
    fn el_mensaje_de_error_de_un_repo_se_sanea_en_el_cuerpo() {
        let mut verificacion = con_fallo_verificacion(verificacion_vacia("jparga"), "repo1");
        verificacion.avisos.push(AvisoVerificacion::ErrorRepo {
            id: id("jparga", "repo1"),
            mensaje: "<script>inyección</script>&\ncon salto de línea".to_string(),
        });
        let entrada = entrada_avisos("jparga", None, Some(verificacion), false);

        let (notificaciones, _estado) = decidir(&entrada, &EstadoAvisos::default(), AHORA);

        assert_eq!(notificaciones.len(), 1);
        for idioma in [Idioma::Es, Idioma::En] {
            let cuerpo = notificaciones[0].cuerpo.localizar(idioma);
            assert!(!cuerpo.contains('<'), "{idioma:?}: {cuerpo}");
            assert!(!cuerpo.contains('>'), "{idioma:?}: {cuerpo}");
            assert!(!cuerpo.contains('&'), "{idioma:?}: {cuerpo}");
            assert!(!cuerpo.contains('\n'), "{idioma:?}: {cuerpo}");
            assert!(cuerpo.contains("inyección"), "{idioma:?}: {cuerpo}");
        }
    }

    #[test]
    fn un_login_no_puede_llevar_marcado_porque_nombre_ya_lo_prohibe() {
        // `Nombre` ya restringe el alfabeto (ver `modelo::Nombre`), así que esto
        // documenta la invariante en vez de probar `sanear_texto` de nuevo (ver
        // `saneado::tests` para eso): construir un `Nombre` con `<` es un error.
        assert!(Nombre::nuevo("<script>").is_err());
    }
}
