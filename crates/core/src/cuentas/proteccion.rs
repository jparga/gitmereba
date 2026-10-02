//! Snapshots en cada pasada de sincronización periódica.
//!
//! [`proteger_cuenta`] se llama tras cada pasada de sincronización/verificación, sobre
//! los mirrors ya listados en esa misma pasada (ver `super::sincronizar`): ni los de
//! organizaciones `contingencia-*` (otro módulo se ocupa de ellos) ni los mirrors
//! todavía vacíos. Por cada uno:
//!
//! 1. Lee el resumen de la última captura ya existente (si la hay), antes de tocar nada.
//! 2. Llama a [`crate::snapshots::capturar`]. Si crea una captura nueva y ya había una
//!    anterior, compara ambas con [`crate::snapshots::detectar_en_bare`] (que resuelve la
//!    ancestría sobre el propio bare de snapshots, capaz de verla porque conserva los
//!    objetos de ambas capturas). Si encuentra un cambio destructivo, protege la captura
//!    ANTERIOR con [`crate::snapshots::proteger`] antes de que la rotación pueda
//!    alcanzarla, y añade una entrada al informe.
//! 3. Llama a [`crate::snapshots::rotar`] con la política de retención por defecto.
//!
//! Un error en un repo se anota en el informe y no impide proteger los demás.
//!
//! ## Ventana sin protección
//!
//! Gitea sincroniza los mirrors por su cuenta cada N minutos: cuando corre esta pasada,
//! un force-push (o un borrado de rama o tag) ya ha llegado al mirror local. La
//! protección funciona porque compara con la captura de la pasada ANTERIOR, que todavía
//! conserva en su bare los objetos que el force-push ha dejado inalcanzables en el
//! origen. Por eso conviene que la primera captura de un repo se haga cuanto antes tras
//! darlo de alta: esta función no se invoca desde `cuentas::alta` (fuera del alcance de
//! esta tarea), así que la primera vez que protege algo de un repo es en su primera
//! pasada periódica. Entre el alta de una cuenta y esa primera pasada, un origen ya
//! comprometido en ese intervalo no queda protegido: no hay todavía ninguna captura
//! anterior con la que compararlo.

use serde::Serialize;
use time::OffsetDateTime;

use crate::config::RutasCuenta;
use crate::modelo::{IdRepo, RepoLocal};
use crate::snapshots::{self, CambioDestructivo, Captura, PoliticaRetencion};

/// Prefijo de las organizaciones de contingencia: sus mirrors no se protegen aquí (otro módulo se ocupa de ellos).
const PREFIJO_CONTINGENCIA: &str = "contingencia-";

/// Error interno de una pasada de protección sobre un repo. No se expone fuera de este
/// módulo: [`proteger_cuenta`] nunca falla, cada error se anota en
/// [`InformeProteccion::errores`].
#[derive(Debug, thiserror::Error)]
enum ErrorProteccion {
    #[error("snapshots: {0}")]
    Snapshots(#[from] snapshots::ErrorSnapshots),
}

/// Un cambio destructivo detectado y ya protegido en una pasada: la captura
/// `marca_protegida` de `id` conserva los objetos que el cambio dejó inalcanzables.
#[derive(Debug, Clone, Serialize)]
pub struct CambioDetectado {
    pub id: IdRepo,
    pub marca_protegida: String,
    pub cambios: Vec<CambioDestructivo>,
}

/// Resultado de [`proteger_cuenta`]: qué se ha capturado, qué cambios destructivos se
/// han detectado y protegido, y qué repos han fallado.
#[derive(Debug, Clone, Default, Serialize)]
pub struct InformeProteccion {
    /// Cuántos mirrors han tenido una captura nueva en esta pasada.
    pub capturas_nuevas: usize,
    /// Cambios destructivos detectados (y ya protegidos) en esta pasada.
    pub cambios: Vec<CambioDetectado>,
    /// Repos en los que ha fallado la protección, con el motivo; no detiene al resto.
    pub errores: Vec<(IdRepo, String)>,
}

impl InformeProteccion {
    /// Resumen de una línea, en español, para el histórico y la auditoría.
    pub fn resumen(&self) -> String {
        format!(
            "{} captura(s) nueva(s), {} cambio(s) destructivo(s) protegido(s), {} error(es)",
            self.capturas_nuevas,
            self.cambios.len(),
            self.errores.len()
        )
    }
}

fn en_contingencia(id: &IdRepo) -> bool {
    id.dueno
        .as_str()
        .to_lowercase()
        .starts_with(PREFIJO_CONTINGENCIA)
}

/// Protege un único repo: captura, detecta cambios destructivos frente a la captura
/// anterior (si la había) y rota. Ver el `//!` del módulo para el porqué de cada paso.
async fn proteger_repo(
    rutas: &RutasCuenta,
    id: &IdRepo,
    ahora: OffsetDateTime,
    politica: &PoliticaRetencion,
    informe: &mut InformeProteccion,
) -> Result<(), ErrorProteccion> {
    let anteriores = snapshots::listar(rutas, id)?;
    let marca_anterior = anteriores.last().map(|resumen| resumen.marca.clone());

    let captura = snapshots::capturar(rutas, id, ahora).await?;

    if let Captura::Nueva(nueva) = &captura {
        informe.capturas_nuevas += 1;

        if let Some(marca_anterior) = marca_anterior {
            let cambios =
                snapshots::detectar_desde_captura(rutas, id, &marca_anterior, &nueva.refs).await?;
            if !cambios.is_empty() {
                snapshots::proteger(rutas, id, &marca_anterior).await?;
                informe.cambios.push(CambioDetectado {
                    id: id.clone(),
                    marca_protegida: marca_anterior,
                    cambios,
                });
            }
        }
    }

    snapshots::rotar(rutas, id, politica, ahora).await?;
    Ok(())
}

/// Protege todos los mirrors locales de una cuenta en una pasada: captura su estado,
/// detecta y protege cambios destructivos frente a la pasada anterior, y rota las
/// capturas que ya no hace falta conservar. Se salta las organizaciones
/// `contingencia-*` y los mirrors todavía vacíos. Nunca falla: cada error se anota en
/// [`InformeProteccion::errores`] y no detiene al resto (secuencial, un repo cada vez).
pub async fn proteger_cuenta(
    rutas: &RutasCuenta,
    local: &[RepoLocal],
    ahora: OffsetDateTime,
    politica: &PoliticaRetencion,
) -> InformeProteccion {
    let mut informe = InformeProteccion::default();

    for repo in local {
        if repo.vacio || en_contingencia(&repo.id) {
            continue;
        }
        if let Err(error) = proteger_repo(rutas, &repo.id, ahora, politica, &mut informe).await {
            informe.errores.push((repo.id.clone(), error.to_string()));
        }
    }

    informe
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{self, Opciones};
    use crate::modelo::Nombre;

    fn id(dueno: &str, nombre: &str) -> IdRepo {
        IdRepo {
            dueno: Nombre::nuevo(dueno).expect("dueño válido"),
            nombre: Nombre::nuevo(nombre).expect("nombre válido"),
        }
    }

    fn repo_local(dueno: &str, nombre: &str, vacio: bool) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre),
            es_mirror: true,
            vacio,
            privado: true,
            tamano_kb: 5,
            ultima_sync: None,
        }
    }

    async fn git_de_prueba(directorio: &std::path::Path, args: &[&str]) -> String {
        let mut completos = vec!["-c", "user.name=Test", "-c", "user.email=test@test.invalid"];
        completos.extend_from_slice(args);
        let opciones = Opciones {
            directorio: Some(directorio.to_path_buf()),
            ..Opciones::default()
        };
        git::ejecutar(&completos, &opciones)
            .await
            .unwrap_or_else(|error| panic!("`git {args:?}` falló: {error}"))
            .stdout
    }

    /// Crea (o reutiliza) un bare «de Gitea» en `raiz/<dueno>/<nombre>.git» con un commit
    /// inicial en `main`, y devuelve su ruta.
    async fn crear_bare_origen(
        raiz: &std::path::Path,
        dueno: &str,
        nombre: &str,
    ) -> std::path::PathBuf {
        let trabajo = tempfile::tempdir().expect("directorio temporal de trabajo");
        git_de_prueba(
            trabajo.path(),
            &["-c", "init.defaultBranch=main", "init", "-q"],
        )
        .await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "inicial"],
        )
        .await;

        let bare = raiz
            .join(dueno.to_lowercase())
            .join(format!("{}.git", nombre.to_lowercase()));
        std::fs::create_dir_all(bare.parent().expect("padre")).expect("crear carpeta del dueño");
        git::ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                trabajo.path().to_str().expect("utf8"),
                bare.to_str().expect("utf8"),
            ],
            &Opciones::default(),
        )
        .await
        .expect("clonar en bare");
        bare
    }

    /// Reescribe la historia de `main` en el bare `origen`: simula el force-push que
    /// Gitea ya habría replicado en el mirror antes de que corriera esta pasada.
    async fn forzar_push(origen: &std::path::Path) {
        let trabajo = tempfile::tempdir().expect("árbol de trabajo");
        git::ejecutar(
            &[
                "clone",
                "-q",
                origen.to_str().expect("utf8"),
                trabajo.path().to_str().expect("utf8"),
            ],
            &Opciones::default(),
        )
        .await
        .expect("clonar para reescribir la historia");
        // Un commit huérfano (sin el padre anterior): no es descendiente del HEAD
        // anterior, así que el nuevo SHA de `main` no tiene al viejo como antepasado.
        git_de_prueba(
            trabajo.path(),
            &[
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "reescrito",
                "--no-verify",
            ],
        )
        .await;
        // El commit anterior sigue teniendo un padre: hay que crear una historia nueva
        // de verdad (checkpoint --orphan) para que no sea ancestro del anterior.
        git_de_prueba(trabajo.path(), &["checkout", "--orphan", "reescrita"]).await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "historia nueva"],
        )
        .await;
        git_de_prueba(trabajo.path(), &["branch", "-M", "reescrita", "main"]).await;
        git_de_prueba(trabajo.path(), &["push", "-q", "--force", "origin", "main"]).await;
    }

    fn rutas_de(raiz: &std::path::Path) -> RutasCuenta {
        RutasCuenta::nueva(raiz.join("cuenta"))
    }

    fn ahora() -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }

    #[tokio::test]
    async fn primera_pasada_captura_cada_mirror_no_vacio() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let local = vec![repo_local("jparga", "repo1", false)];

        let informe = proteger_cuenta(&rutas, &local, ahora(), &PoliticaRetencion::default()).await;

        assert_eq!(informe.capturas_nuevas, 1);
        assert!(informe.cambios.is_empty());
        assert!(informe.errores.is_empty(), "{:?}", informe.errores);
        assert_eq!(
            snapshots::listar(&rutas, &id("jparga", "repo1"))
                .expect("listar no falla")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn segunda_pasada_sin_cambios_no_captura() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let local = vec![repo_local("jparga", "repo1", false)];
        let politica = PoliticaRetencion::default();

        proteger_cuenta(&rutas, &local, ahora(), &politica).await;
        let segunda = proteger_cuenta(&rutas, &local, ahora(), &politica).await;

        assert_eq!(segunda.capturas_nuevas, 0);
        assert!(segunda.errores.is_empty(), "{:?}", segunda.errores);
    }

    #[tokio::test]
    async fn un_force_push_se_detecta_se_protege_y_queda_en_el_informe() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let origen = crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "repo1").await;
        let local = vec![repo_local("jparga", "repo1", false)];
        let politica = PoliticaRetencion::default();

        let primera = proteger_cuenta(&rutas, &local, ahora(), &politica).await;
        assert_eq!(primera.capturas_nuevas, 1);
        let marca_primera = snapshots::listar(&rutas, &id("jparga", "repo1"))
            .expect("listar no falla")[0]
            .marca
            .clone();

        forzar_push(&origen).await;

        let segunda = proteger_cuenta(&rutas, &local, ahora(), &politica).await;

        assert_eq!(segunda.capturas_nuevas, 1);
        assert_eq!(segunda.cambios.len(), 1, "{:?}", segunda.cambios);
        assert_eq!(segunda.cambios[0].id, id("jparga", "repo1"));
        assert_eq!(segunda.cambios[0].marca_protegida, marca_primera);
        assert!(
            segunda.cambios[0]
                .cambios
                .iter()
                .any(|c| matches!(c, CambioDestructivo::HistoriaReescrita { .. }))
        );

        let listado = snapshots::listar(&rutas, &id("jparga", "repo1")).expect("listar no falla");
        let anterior = listado
            .iter()
            .find(|r| r.marca == marca_primera)
            .expect("la captura anterior sigue en el listado");
        assert!(
            anterior.protegida,
            "la captura anterior debe quedar protegida"
        );
    }

    #[tokio::test]
    async fn un_repo_roto_no_impide_proteger_los_demas() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        crear_bare_origen(&rutas.gitea_repositorios(), "jparga", "bueno").await;
        // "roto" no tiene bare de origen: `snapshots::capturar` fallará con
        // `OrigenNoExiste`, pero el resto de repos deben protegerse igualmente.
        std::fs::create_dir_all(rutas.gitea_repositorios().join("jparga"))
            .expect("crear carpeta del dueño");
        let local = vec![
            repo_local("jparga", "bueno", false),
            repo_local("jparga", "roto", false),
        ];

        let informe = proteger_cuenta(&rutas, &local, ahora(), &PoliticaRetencion::default()).await;

        assert_eq!(informe.capturas_nuevas, 1);
        assert_eq!(informe.errores.len(), 1);
        assert_eq!(informe.errores[0].0, id("jparga", "roto"));
        assert_eq!(
            snapshots::listar(&rutas, &id("jparga", "bueno"))
                .expect("listar no falla")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn un_mirror_vacio_no_se_captura() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let local = vec![repo_local("jparga", "vacio", true)];

        let informe = proteger_cuenta(&rutas, &local, ahora(), &PoliticaRetencion::default()).await;

        assert_eq!(informe.capturas_nuevas, 0);
        assert!(informe.errores.is_empty());
    }

    #[tokio::test]
    async fn los_repos_de_organizaciones_de_contingencia_no_se_tocan() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = rutas_de(raiz.path());
        let local = vec![repo_local("contingencia-jparga", "repo1", false)];

        let informe = proteger_cuenta(&rutas, &local, ahora(), &PoliticaRetencion::default()).await;

        assert_eq!(informe.capturas_nuevas, 0);
        assert!(informe.errores.is_empty());
    }

    #[test]
    fn el_resumen_incluye_los_tres_contadores() {
        let informe = InformeProteccion {
            capturas_nuevas: 2,
            cambios: vec![CambioDetectado {
                id: id("jparga", "repo1"),
                marca_protegida: "20260101T000000Z".to_string(),
                cambios: vec![CambioDestructivo::RamaBorrada {
                    rama: "main".to_string(),
                }],
            }],
            errores: vec![(id("jparga", "repo2"), "boom".to_string())],
        };
        let resumen = informe.resumen();
        assert!(resumen.contains('2'));
        assert!(resumen.contains('1'));
    }
}
