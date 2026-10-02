//! Conversión de tipos de `core` a la forma JSON del contrato con la interfaz.
//!
//! Funciones puras (sin E/S), para poder comprobar la forma exacta del JSON con
//! `serde_json::to_value` sin tocar el almacén, el llavero ni la red.

use std::collections::HashMap;

use serde::Serialize;
use time::OffsetDateTime;

use gitmereba_core::almacen::{
    DatosOrigen, EstadoRepoGuardado, Sincronizacion, VerificacionAuditoria,
};
use gitmereba_core::contingencia::EstadoRama;
use gitmereba_core::cuentas::{ConteoRepos, EstadoCuenta};
use gitmereba_core::github::EstadoServicio;
use gitmereba_core::modelo::{Cuenta, EstadoRepo, IdRepo, Nombre, RepoLocal};
use gitmereba_core::snapshots::ResumenCaptura;

/// `resumen_cuenta`: contadores de repos por estado, más el total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ContadoresDto {
    pub total: usize,
    pub ok: usize,
    pub obsoleto: usize,
    pub fallo: usize,
    pub huerfano: usize,
    pub contingencia: usize,
    pub excluido: usize,
}

fn contadores_de(repos: &ConteoRepos) -> ContadoresDto {
    ContadoresDto {
        total: repos.total(),
        ok: repos.ok,
        obsoleto: repos.obsoleto,
        fallo: repos.fallo,
        huerfano: repos.huerfano,
        contingencia: repos.contingencia,
        excluido: repos.excluido,
    }
}

/// El repo en peor estado presente, según el orden de urgencia de [`EstadoRepo`]. `Ok`
/// si no hay ningún repo, o todos están `Ok`.
fn peor_estado(repos: &ConteoRepos) -> EstadoRepo {
    if repos.fallo > 0 {
        EstadoRepo::Fallo
    } else if repos.obsoleto > 0 {
        EstadoRepo::Obsoleto
    } else if repos.huerfano > 0 {
        EstadoRepo::Huerfano
    } else if repos.contingencia > 0 {
        EstadoRepo::Contingencia
    } else if repos.excluido > 0 {
        EstadoRepo::Excluido
    } else {
        EstadoRepo::Ok
    }
}

/// `resumen_cuenta`.
#[derive(Debug, Clone, Serialize)]
pub struct ResumenCuentaDto {
    pub cuenta: Cuenta,
    pub estado_global: EstadoRepo,
    pub contadores: ContadoresDto,
    pub espacio_disco_kb: u64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ultima_sincronizacion: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub caduca_token: Option<OffsetDateTime>,
}

/// Construye el resumen a partir de [`EstadoCuenta`] (`cuentas::estado`) y del espacio en
/// disco, que `core` no calcula (`verificacion::espacio_de` es aparte porque necesita la
/// carpeta de la cuenta, no solo su login).
pub fn resumen_cuenta_dto(estado: EstadoCuenta, espacio_disco_kb: u64) -> ResumenCuentaDto {
    ResumenCuentaDto {
        estado_global: peor_estado(&estado.repos),
        contadores: contadores_de(&estado.repos),
        ultima_sincronizacion: estado.ultima_correcta.map(|s| s.fin),
        caduca_token: estado.caduca_token,
        cuenta: estado.cuenta,
        espacio_disco_kb,
    }
}

/// `listar_repos`: una fila de la tabla de repositorios.
#[derive(Debug, Clone, Serialize)]
pub struct RepoListadoDto {
    pub id: IdRepo,
    pub privado: bool,
    pub es_fork: bool,
    pub archivado: bool,
    pub tamano_kb: u64,
    pub estado: EstadoRepo,
    pub incluido: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ultima_sync: Option<OffsetDateTime>,
    pub url_clon_local: String,
    pub detalle: Option<String>,
    /// `false` mientras Gitea todavía no ha traído el contenido por primera vez.
    pub clonado: bool,
    /// Por qué no se clona un repo que sí existe en GitHub: `"fork"` (la cuenta no incluye
    /// forks) o `"excluido"`. `None` en los repos que sí tienen copia local.
    pub omitido: Option<&'static str>,
}

/// Un mirror recién creado está vacío y sin fecha de sincronización hasta que Gitea termina
/// de traerlo. Un repo vacío en origen sí tiene fecha, así que no se queda «clonando» para
/// siempre; y si Gitea no responde no se afirma que falte nada.
fn esta_clonado(local: Option<&RepoLocal>) -> bool {
    local.is_none_or(|repo| !repo.vacio || repo.ultima_sync.is_some())
}

/// Combina el último estado guardado (`almacen::estados_de`) con los metadatos del
/// Gitea local (`gitea::ApiGitea::repos_de`) y con lo que GitHub dijo en la última
/// sincronización (`almacen::origenes_de`), si se tienen.
///
/// `privado` es la visibilidad **en GitHub**: la copia local es siempre privada, así que
/// la de Gitea solo se usa mientras no haya habido una sincronización que guarde la real.
fn repo_listado_dto(
    cuenta: &Cuenta,
    guardado: EstadoRepoGuardado,
    local: Option<&RepoLocal>,
    origen: Option<&DatosOrigen>,
) -> RepoListadoDto {
    let url_clon_local = url_clon_local(cuenta, &guardado.id);
    RepoListadoDto {
        privado: origen
            .map(|datos| datos.privado)
            .or(local.map(|repo| repo.privado))
            .unwrap_or(false),
        es_fork: origen.is_some_and(|datos| datos.es_fork),
        archivado: origen.is_some_and(|datos| datos.archivado),
        tamano_kb: local.map(|repo| repo.tamano_kb).unwrap_or(0),
        incluido: guardado.estado != EstadoRepo::Excluido,
        estado: guardado.estado,
        ultima_sync: local.and_then(|repo| repo.ultima_sync),
        clonado: esta_clonado(local),
        omitido: None,
        detalle: guardado.detalle,
        id: guardado.id,
        url_clon_local,
    }
}

fn url_clon_local(cuenta: &Cuenta, id: &IdRepo) -> String {
    format!(
        "{}/{}/{}.git",
        cuenta.url_publica(),
        id.dueno.as_str(),
        id.nombre.as_str()
    )
}

/// Fila de un repo que GitHub lista pero que no tiene (ni ha tenido) copia local: un fork
/// con los forks fuera del alcance, o uno desmarcado en el alta. Sin ella no habría forma
/// de verlo ni de volver a incluirlo desde la ventana.
fn repo_omitido_dto(cuenta: &Cuenta, origen: &DatosOrigen) -> RepoListadoDto {
    let por_fork = origen.es_fork && !cuenta.alcance.incluir_forks;
    RepoListadoDto {
        privado: origen.privado,
        es_fork: origen.es_fork,
        archivado: origen.archivado,
        tamano_kb: 0,
        incluido: false,
        estado: EstadoRepo::Excluido,
        ultima_sync: None,
        clonado: true,
        omitido: Some(if por_fork { "fork" } else { "excluido" }),
        detalle: None,
        id: origen.id.clone(),
        url_clon_local: url_clon_local(cuenta, &origen.id),
    }
}

/// `true` si `origen` está en el alcance de dueños de `cuenta` pero el plan de
/// sincronización lo omite (fork no incluido o excluido a mano).
fn esta_omitido(cuenta: &Cuenta, origen: &DatosOrigen) -> bool {
    let dueno = &origen.id.dueno;
    let dueno_en_alcance = *dueno == cuenta.login
        || cuenta
            .alcance
            .organizaciones
            .iter()
            .any(|org| org.as_str().eq_ignore_ascii_case(dueno.as_str()));
    dueno_en_alcance
        && ((origen.es_fork && !cuenta.alcance.incluir_forks)
            || cuenta.alcance.excluidos.contains(&origen.id))
}

/// Convierte los estados guardados de una cuenta en la lista que espera `listar_repos`,
/// y le añade los repos de GitHub que no se clonan (ver [`repo_omitido_dto`]).
pub fn convertir_repos_listado(
    cuenta: &Cuenta,
    guardados: Vec<EstadoRepoGuardado>,
    locales: &HashMap<IdRepo, RepoLocal>,
    origenes: &[DatosOrigen],
) -> Vec<RepoListadoDto> {
    let por_id: HashMap<&IdRepo, &DatosOrigen> =
        origenes.iter().map(|datos| (&datos.id, datos)).collect();
    let con_estado: std::collections::HashSet<IdRepo> =
        guardados.iter().map(|g| g.id.clone()).collect();

    let mut filas: Vec<RepoListadoDto> = guardados
        .into_iter()
        .map(|guardado| {
            let local = locales.get(&guardado.id);
            let origen = por_id.get(&guardado.id).copied();
            repo_listado_dto(cuenta, guardado, local, origen)
        })
        .collect();
    filas.extend(
        origenes
            .iter()
            .filter(|datos| !con_estado.contains(&datos.id) && esta_omitido(cuenta, datos))
            .map(|datos| repo_omitido_dto(cuenta, datos)),
    );
    filas
}

/// Ajusta el listado a lo que el usuario entiende por «en contingencia»: el repo
/// original con una contingencia activa (`activas`) se muestra como tal —su mirror está
/// en pausa a propósito, no obsoleto— y las copias hermanas de `contingencia-<dueño>` no
/// se listan como repos aparte.
pub fn marcar_contingencias(
    guardados: Vec<EstadoRepoGuardado>,
    activas: &[IdRepo],
) -> Vec<EstadoRepoGuardado> {
    guardados
        .into_iter()
        .filter(|guardado| !gitmereba_core::contingencia::es_org_contingencia(&guardado.id.dueno))
        .map(|mut guardado| {
            if activas.contains(&guardado.id) && guardado.estado != EstadoRepo::Fallo {
                guardado.estado = EstadoRepo::Contingencia;
                guardado.detalle = None;
            }
            guardado
        })
        .collect()
}

/// `historial`: como [`Sincronizacion`], sin `detalle_json` (puede pesar hasta 1 MB y la
/// UI no lo necesita en la tabla).
#[derive(Debug, Clone, Serialize)]
pub struct SincronizacionDto {
    pub id: i64,
    pub cuenta: Nombre,
    #[serde(with = "time::serde::rfc3339")]
    pub inicio: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub fin: OffsetDateTime,
    pub resultado: gitmereba_core::almacen::ResultadoSincronizacion,
    pub creados: u32,
    pub huerfanos: u32,
    pub fallos: u32,
    pub resumen: String,
}

fn sincronizacion_dto(sincronizacion: Sincronizacion) -> SincronizacionDto {
    SincronizacionDto {
        id: sincronizacion.id,
        cuenta: sincronizacion.cuenta,
        inicio: sincronizacion.inicio,
        fin: sincronizacion.fin,
        resultado: sincronizacion.resultado,
        creados: sincronizacion.creados,
        huerfanos: sincronizacion.huerfanos,
        fallos: sincronizacion.fallos,
        resumen: sincronizacion.resumen,
    }
}

pub fn convertir_historial(listado: Vec<Sincronizacion>) -> Vec<SincronizacionDto> {
    listado.into_iter().map(sincronizacion_dto).collect()
}

/// `verificar_auditoria`: normaliza el enum de `core` (sin `rename_all`, así que su
/// `derive(Serialize)` por defecto no coincide con el contrato) a `{ integra, rota_en_id? }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VerificacionAuditoriaDto {
    pub integra: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rota_en_id: Option<i64>,
}

pub fn convertir_verificacion(verificacion: VerificacionAuditoria) -> VerificacionAuditoriaDto {
    match verificacion {
        VerificacionAuditoria::Integra => VerificacionAuditoriaDto {
            integra: true,
            rota_en_id: None,
        },
        VerificacionAuditoria::Rota { id } => VerificacionAuditoriaDto {
            integra: false,
            rota_en_id: Some(id),
        },
    }
}

/// `estado_github`: mapeo 1:1 de [`EstadoServicio`], en minúsculas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EstadoServicioDto {
    Operativo,
    Degradado,
    Caido,
    Desconocido,
}

#[derive(Debug, Clone, Serialize)]
pub struct EstadoGithubDto {
    pub estado: EstadoServicioDto,
    #[serde(with = "time::serde::rfc3339")]
    pub consultado: OffsetDateTime,
}

pub fn convertir_estado_github(
    estado: EstadoServicio,
    consultado: OffsetDateTime,
) -> EstadoGithubDto {
    let estado = match estado {
        EstadoServicio::Operativo => EstadoServicioDto::Operativo,
        EstadoServicio::Degradado => EstadoServicioDto::Degradado,
        EstadoServicio::Caido => EstadoServicioDto::Caido,
        EstadoServicio::Desconocido => EstadoServicioDto::Desconocido,
    };
    EstadoGithubDto { estado, consultado }
}

/// `estado_contingencia`: una rama con commits pendientes de reconciliar.
#[derive(Debug, Clone, Serialize)]
pub struct ConmitsDeMasDto {
    pub rama: String,
    pub commits: u64,
}

/// `estado_contingencia`: un repo en contingencia.
#[derive(Debug, Clone, Serialize)]
pub struct EntradaContingenciaDto {
    pub id: IdRepo,
    pub comando: String,
    pub commits_de_mas: Vec<ConmitsDeMasDto>,
}

/// Solo las ramas con commits de más: las que están al día no aportan nada a la vista.
pub fn convertir_commits_de_mas(ramas: Vec<EstadoRama>) -> Vec<ConmitsDeMasDto> {
    ramas
        .into_iter()
        .filter(|rama| rama.commits_de_mas > 0)
        .map(|rama| ConmitsDeMasDto {
            rama: rama.rama,
            commits: rama.commits_de_mas,
        })
        .collect()
}

/// `ajustes_leer`: una captura de `snapshots`, sin las refs completas (`marca` +
/// `protegido` basta para la UI; ver `snapshots::ResumenCaptura` para el resto).
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotDto {
    pub id: IdRepo,
    pub marca: String,
    pub protegido: bool,
}

pub fn convertir_snapshots(capturas: Vec<ResumenCaptura>) -> Vec<SnapshotDto> {
    capturas
        .into_iter()
        .map(|captura| SnapshotDto {
            id: captura.id,
            marca: captura.marca,
            protegido: captura.protegida,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct AjustesCuentaDto {
    pub cuenta: Cuenta,
    pub version_gitea: String,
    pub snapshots: Vec<SnapshotDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AjustesGlobalDto {
    pub version_gitea: String,
    pub snapshots: Vec<SnapshotDto>,
}

/// `ajustes_leer`: por cuenta lleva la clave `cuenta`; global, no. `#[serde(untagged)]`
/// hace que cada variante serialice con exactamente los campos de su propio struct.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AjustesDto {
    Cuenta(AjustesCuentaDto),
    Global(AjustesGlobalDto),
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitmereba_core::almacen::ResultadoSincronizacion;
    use gitmereba_core::modelo::Alcance;
    use std::path::PathBuf;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(repo),
        }
    }

    fn cuenta_de_prueba() -> Cuenta {
        Cuenta {
            login: nombre("jparga"),
            carpeta: PathBuf::from("/home/jparga/gitmereba/jparga"),
            puerto: 3900,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![nombre("mereba-oss")],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    fn sincronizacion_de_prueba() -> Sincronizacion {
        Sincronizacion {
            id: 501,
            cuenta: nombre("jparga"),
            inicio: OffsetDateTime::UNIX_EPOCH,
            fin: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(4),
            resultado: ResultadoSincronizacion::Ok,
            creados: 0,
            huerfanos: 0,
            fallos: 0,
            resumen: "resumen de prueba".to_string(),
            detalle_json: Some("{\"secreto\":true}".to_string()),
        }
    }

    #[test]
    fn peor_estado_es_ok_sin_repos() {
        assert_eq!(peor_estado(&ConteoRepos::default()), EstadoRepo::Ok);
    }

    #[test]
    fn peor_estado_respeta_el_orden_de_urgencia() {
        let repos = ConteoRepos {
            ok: 4,
            obsoleto: 1,
            fallo: 0,
            huerfano: 1,
            contingencia: 0,
            excluido: 1,
        };
        assert_eq!(peor_estado(&repos), EstadoRepo::Obsoleto);
    }

    #[test]
    fn resumen_cuenta_dto_produce_la_forma_del_contrato() {
        let estado = EstadoCuenta {
            cuenta: cuenta_de_prueba(),
            gitea_responde: true,
            version_gitea: Some("1.27.3".to_string()),
            repos: ConteoRepos {
                ok: 4,
                obsoleto: 1,
                fallo: 1,
                huerfano: 1,
                contingencia: 1,
                excluido: 1,
            },
            ultima_sincronizacion: Some(sincronizacion_de_prueba()),
            ultima_correcta: Some(sincronizacion_de_prueba()),
            caduca_token: Some(OffsetDateTime::UNIX_EPOCH + time::Duration::days(30)),
        };

        let json = serde_json::to_value(resumen_cuenta_dto(estado, 33792)).unwrap();

        assert_eq!(json["cuenta"]["login"], "jparga");
        assert_eq!(json["estado_global"], "fallo");
        assert_eq!(json["contadores"]["total"], 9);
        assert_eq!(json["contadores"]["ok"], 4);
        assert_eq!(json["espacio_disco_kb"], 33792);
        assert_eq!(json["ultima_sincronizacion"], "1970-01-01T00:00:04Z");
        assert_eq!(json["caduca_token"], "1970-01-31T00:00:00Z");
    }

    #[test]
    fn resumen_cuenta_dto_deja_null_las_fechas_ausentes() {
        let estado = EstadoCuenta {
            cuenta: cuenta_de_prueba(),
            gitea_responde: false,
            version_gitea: None,
            repos: ConteoRepos::default(),
            ultima_sincronizacion: None,
            ultima_correcta: None,
            caduca_token: None,
        };

        let json = serde_json::to_value(resumen_cuenta_dto(estado, 0)).unwrap();

        assert_eq!(json["estado_global"], "ok");
        assert!(json["ultima_sincronizacion"].is_null());
        assert!(json["caduca_token"].is_null());
    }

    fn guardado(dueno: &str, repo: &str, estado: EstadoRepo) -> EstadoRepoGuardado {
        EstadoRepoGuardado {
            cuenta: nombre("jparga"),
            id: id(dueno, repo),
            estado,
            detalle: Some("detalle".to_string()),
            actualizado: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn esta_clonado_distingue_un_mirror_recien_creado() {
        let mut repo = RepoLocal {
            id: id("jparga", "nuevo"),
            es_mirror: true,
            vacio: true,
            privado: true,
            tamano_kb: 0,
            ultima_sync: None,
        };
        assert!(!esta_clonado(Some(&repo)));
        repo.ultima_sync = Some(OffsetDateTime::UNIX_EPOCH);
        assert!(
            esta_clonado(Some(&repo)),
            "vacío en origen, pero ya sincronizado"
        );
        repo.vacio = false;
        repo.ultima_sync = None;
        assert!(esta_clonado(Some(&repo)));
        assert!(
            esta_clonado(None),
            "sin datos de Gitea no se afirma que falte"
        );
    }

    #[test]
    fn marcar_contingencias_muestra_el_original_y_oculta_la_copia_hermana() {
        let guardados = vec![
            guardado("jparga", "uno", EstadoRepo::Obsoleto),
            guardado("contingencia-jparga", "uno", EstadoRepo::Contingencia),
            guardado("jparga", "dos", EstadoRepo::Ok),
        ];

        let listado = marcar_contingencias(guardados, &[id("jparga", "uno")]);

        assert_eq!(listado.len(), 2);
        assert_eq!(listado[0].estado, EstadoRepo::Contingencia);
        assert_eq!(listado[0].detalle, None);
        assert_eq!(listado[1].estado, EstadoRepo::Ok);
    }

    #[test]
    fn marcar_contingencias_no_tapa_un_fallo() {
        let guardados = vec![guardado("jparga", "uno", EstadoRepo::Fallo)];
        let listado = marcar_contingencias(guardados, &[id("jparga", "uno")]);
        assert_eq!(listado[0].estado, EstadoRepo::Fallo);
    }

    #[test]
    fn repo_listado_dto_usa_los_metadatos_locales_cuando_los_hay() {
        let cuenta = cuenta_de_prueba();
        let guardado = EstadoRepoGuardado {
            cuenta: nombre("jparga"),
            id: id("jparga", "gitmereba"),
            estado: EstadoRepo::Ok,
            detalle: None,
            actualizado: OffsetDateTime::UNIX_EPOCH,
        };
        let mut locales = HashMap::new();
        locales.insert(
            id("jparga", "gitmereba"),
            RepoLocal {
                id: id("jparga", "gitmereba"),
                es_mirror: true,
                vacio: false,
                privado: false,
                tamano_kb: 8192,
                ultima_sync: Some(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(10)),
            },
        );

        let json = serde_json::to_value(convertir_repos_listado(
            &cuenta,
            vec![guardado],
            &locales,
            &[],
        ))
        .unwrap();

        assert_eq!(json[0]["id"]["dueno"], "jparga");
        assert_eq!(json[0]["id"]["nombre"], "gitmereba");
        assert_eq!(json[0]["privado"], false);
        assert_eq!(json[0]["es_fork"], false);
        assert_eq!(json[0]["archivado"], false);
        assert_eq!(json[0]["tamano_kb"], 8192);
        assert_eq!(json[0]["estado"], "ok");
        assert_eq!(json[0]["incluido"], true);
        assert_eq!(json[0]["ultima_sync"], "1970-01-01T00:00:10Z");
        assert_eq!(
            json[0]["url_clon_local"],
            "http://127.0.0.1:3900/jparga/gitmereba.git"
        );
        assert!(json[0]["detalle"].is_null());
    }

    #[test]
    fn repo_listado_dto_sin_metadatos_locales_usa_valores_por_defecto() {
        let cuenta = cuenta_de_prueba();
        let guardado = EstadoRepoGuardado {
            cuenta: nombre("jparga"),
            id: id("jparga", "roto-ci"),
            estado: EstadoRepo::Excluido,
            detalle: Some("excluido a mano".to_string()),
            actualizado: OffsetDateTime::UNIX_EPOCH,
        };

        let json = serde_json::to_value(convertir_repos_listado(
            &cuenta,
            vec![guardado],
            &HashMap::new(),
            &[],
        ))
        .unwrap();

        assert_eq!(json[0]["estado"], "excluido");
        assert_eq!(json[0]["incluido"], false);
        assert_eq!(json[0]["tamano_kb"], 0);
        assert!(json[0]["ultima_sync"].is_null());
        assert_eq!(json[0]["detalle"], "excluido a mano");
    }

    fn origen(nombre_repo: &str, privado: bool, es_fork: bool) -> DatosOrigen {
        DatosOrigen {
            id: id("jparga", nombre_repo),
            privado,
            es_fork,
            archivado: false,
        }
    }

    fn guardado_ok(nombre_repo: &str) -> EstadoRepoGuardado {
        EstadoRepoGuardado {
            cuenta: nombre("jparga"),
            id: id("jparga", nombre_repo),
            estado: EstadoRepo::Ok,
            detalle: None,
            actualizado: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn la_visibilidad_es_la_de_github_y_no_la_de_la_copia_local() {
        let cuenta = cuenta_de_prueba();
        let mut locales = HashMap::new();
        locales.insert(
            id("jparga", "upptime"),
            RepoLocal {
                id: id("jparga", "upptime"),
                es_mirror: true,
                vacio: false,
                // La copia local es siempre privada.
                privado: true,
                tamano_kb: 10,
                ultima_sync: None,
            },
        );

        let listado = convertir_repos_listado(
            &cuenta,
            vec![guardado_ok("upptime")],
            &locales,
            &[origen("upptime", false, false)],
        );

        assert!(!listado[0].privado);
        assert_eq!(listado[0].omitido, None);
    }

    #[test]
    fn los_forks_fuera_del_alcance_se_listan_como_omitidos() {
        let cuenta = cuenta_de_prueba();
        assert!(!cuenta.alcance.incluir_forks);

        let listado = convertir_repos_listado(
            &cuenta,
            vec![guardado_ok("propio")],
            &HashMap::new(),
            &[origen("propio", true, false), origen("yatl", false, true)],
        );

        assert_eq!(listado.len(), 2);
        let fork = &listado[1];
        assert_eq!(fork.id, id("jparga", "yatl"));
        assert_eq!(fork.omitido, Some("fork"));
        assert_eq!(fork.estado, EstadoRepo::Excluido);
        assert!(fork.es_fork && !fork.incluido && !fork.privado);
    }

    #[test]
    fn un_excluido_sin_copia_local_se_lista_y_con_forks_incluidos_ya_no_es_por_fork() {
        let mut cuenta = cuenta_de_prueba();
        cuenta.alcance.incluir_forks = true;
        cuenta.alcance.excluidos = vec![id("jparga", "yatl")];

        let listado = convertir_repos_listado(
            &cuenta,
            Vec::new(),
            &HashMap::new(),
            &[
                origen("yatl", false, true),
                // Fork incluido pero aún sin sincronizar: no se inventa una fila.
                origen("otro-fork", false, true),
            ],
        );

        assert_eq!(listado.len(), 1);
        assert_eq!(listado[0].omitido, Some("excluido"));
    }

    #[test]
    fn un_repo_de_un_dueno_ajeno_al_alcance_no_se_lista() {
        let cuenta = cuenta_de_prueba();
        let ajeno = DatosOrigen {
            id: id("otra-org", "fork"),
            privado: false,
            es_fork: true,
            archivado: false,
        };

        assert!(convertir_repos_listado(&cuenta, Vec::new(), &HashMap::new(), &[ajeno]).is_empty());
    }

    #[test]
    fn convertir_historial_omite_detalle_json() {
        let json =
            serde_json::to_value(convertir_historial(vec![sincronizacion_de_prueba()])).unwrap();

        assert_eq!(json[0]["id"], 501);
        assert_eq!(json[0]["resultado"], "ok");
        assert!(json[0].get("detalle_json").is_none());
    }

    #[test]
    fn verificacion_integra_no_lleva_rota_en_id() {
        let json =
            serde_json::to_value(convertir_verificacion(VerificacionAuditoria::Integra)).unwrap();
        assert_eq!(json["integra"], true);
        assert!(json.get("rota_en_id").is_none());
    }

    #[test]
    fn verificacion_rota_lleva_el_id() {
        let json = serde_json::to_value(convertir_verificacion(VerificacionAuditoria::Rota {
            id: 42,
        }))
        .unwrap();
        assert_eq!(json["integra"], false);
        assert_eq!(json["rota_en_id"], 42);
    }

    #[test]
    fn estado_github_se_serializa_en_minusculas() {
        for (origen, esperado) in [
            (EstadoServicio::Operativo, "operativo"),
            (EstadoServicio::Degradado, "degradado"),
            (EstadoServicio::Caido, "caido"),
            (EstadoServicio::Desconocido, "desconocido"),
        ] {
            let json =
                serde_json::to_value(convertir_estado_github(origen, OffsetDateTime::UNIX_EPOCH))
                    .unwrap();
            assert_eq!(json["estado"], esperado);
            assert_eq!(json["consultado"], "1970-01-01T00:00:00Z");
        }
    }

    #[test]
    fn commits_de_mas_omite_las_ramas_al_dia() {
        let ramas = vec![
            EstadoRama {
                rama: "main".to_string(),
                commits_de_mas: 3,
                es_nueva: false,
            },
            EstadoRama {
                rama: "al-dia".to_string(),
                commits_de_mas: 0,
                es_nueva: false,
            },
        ];
        let json = serde_json::to_value(convertir_commits_de_mas(ramas)).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_eq!(json[0]["rama"], "main");
        assert_eq!(json[0]["commits"], 3);
    }

    #[test]
    fn ajustes_por_cuenta_lleva_la_clave_cuenta() {
        let dto = AjustesDto::Cuenta(AjustesCuentaDto {
            cuenta: cuenta_de_prueba(),
            version_gitea: "1.27.3".to_string(),
            snapshots: vec![],
        });
        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["cuenta"]["login"], "jparga");
        assert_eq!(json["version_gitea"], "1.27.3");
        assert_eq!(json["snapshots"], serde_json::json!([]));
    }

    #[test]
    fn ajustes_globales_no_llevan_la_clave_cuenta() {
        let dto = AjustesDto::Global(AjustesGlobalDto {
            version_gitea: "1.27.3".to_string(),
            snapshots: vec![],
        });
        let json = serde_json::to_value(dto).unwrap();
        assert!(json.get("cuenta").is_none());
        assert_eq!(json["version_gitea"], "1.27.3");
    }
}
