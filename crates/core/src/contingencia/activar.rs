//! `activar`: crea (o reutiliza) el repo hermano de contingencia y lo puebla con el
//! contenido del mirror.

use std::path::Path;

use crate::git::{self, CertificadoCa, Credencial, ErrorGit, Opciones};
use crate::gitea::ApiGitea;
use crate::modelo::{Cuenta, IdRepo};
use crate::secretos::Secreto;

use super::error::{ErrorContingencia, PasoActivar};
use super::lfs::detectar_advertencias;
use super::modelo::{PuntoDePartida, RepoContingencia};
use super::nombres::org_contingencia;
use super::rutas::ruta_bare_validada;
use super::temporal::DirTemporal;

const PREFIJO_TEMPORAL: &str = "gitmereba-activar";

/// Credenciales de administración de Gitea para el `git push` de `activar`. Se
/// construyen fuera de este módulo (usuario administrador y token generados por
/// `instancia::provisionar`, guardado en el llavero): `contingencia` no conoce esos
/// detalles, solo los usa para empujar al repo hermano.
#[derive(Clone)]
pub struct CredencialGitea {
    pub usuario: String,
    pub token: Secreto,
}

/// Activa la contingencia de `id` (un mirror existente): pausa su sincronización, anota
/// el punto de partida, crea el repo hermano en `contingencia-<dueño>` y lo puebla con
/// `git push --mirror` desde el bare del mirror.
///
/// Idempotente: si el repo hermano ya existe y no está vacío, se devuelve su información
/// sin tocar nada más; si existe pero está vacío (un intento anterior falló después de
/// crearlo), se reintenta el empuje. Si algo falla, no se borra nada de lo ya hecho: el
/// error dice en qué paso quedó (repetir `activar` lo completa).
pub async fn activar<T: ApiGitea>(
    gitea: &T,
    cuenta: &Cuenta,
    rutas: &crate::config::RutasCuenta,
    id: &IdRepo,
    credencial_gitea: &CredencialGitea,
) -> Result<RepoContingencia, ErrorContingencia> {
    let original = gitea
        .repo(id)
        .await?
        .ok_or_else(|| ErrorContingencia::RepoNoExiste(id.clone()))?;
    if !original.es_mirror {
        return Err(ErrorContingencia::NoEsMirror(id.clone()));
    }

    let bare_mirror = ruta_bare_validada(&rutas.gitea_repositorios(), id)
        .ok_or_else(|| ErrorContingencia::RutaBareInvalida(id.clone()))?;

    let dueno_contingencia = org_contingencia(&id.dueno)?;
    let id_contingencia = IdRepo {
        dueno: dueno_contingencia.clone(),
        nombre: id.nombre.clone(),
    };

    let existente = gitea.repo(&id_contingencia).await?;
    if let Some(repo) = &existente
        && !repo.vacio
    {
        let punto_de_partida = PuntoDePartida::leer(&bare_mirror).await?;
        let advertencias = detectar_advertencias(&bare_mirror).await;
        reconciliar_lan_mejor_esfuerzo(gitea).await;
        return Ok(construir_resultado(
            cuenta,
            id,
            &id_contingencia,
            punto_de_partida,
            advertencias,
        ));
    }

    gitea
        .fijar_intervalo(id, "0")
        .await
        .map_err(|origen| ErrorContingencia::FalloAlActivar {
            repo: id.clone(),
            paso: PasoActivar::PausarMirror,
            origen: origen.to_string(),
        })?;

    let punto_de_partida = PuntoDePartida::leer(&bare_mirror).await?;
    let mut advertencias = detectar_advertencias(&bare_mirror).await;

    gitea
        .asegurar_organizacion(&dueno_contingencia)
        .await
        .map_err(|origen| ErrorContingencia::FalloAlActivar {
            repo: id.clone(),
            paso: PasoActivar::AsegurarOrganizacion,
            origen: origen.to_string(),
        })?;

    if existente.is_none() {
        gitea
            .crear_repo(&dueno_contingencia, &id.nombre, true)
            .await
            .map_err(|origen| ErrorContingencia::FalloAlActivar {
                repo: id.clone(),
                paso: PasoActivar::CrearRepo,
                origen: origen.to_string(),
            })?;
    }

    let base_gitea = cuenta.url_gitea();
    let url_destino = format!(
        "{base_gitea}/{}/{}.git",
        dueno_contingencia.as_str(),
        id.nombre.as_str()
    );
    let origen_credencial = format!("{base_gitea}/");
    // Con acceso LAN activo, `url_gitea()` es `https://127.0.0.1:<puerto>` con el
    // certificado autofirmado de la cuenta: sin `certificado_ca`, `git` lo rechazaría
    // por no ser una CA reconocida (nunca se usa `sslVerify=false`).
    let certificado_ca = cuenta.lan.as_ref().map(|_| CertificadoCa {
        origen: origen_credencial.clone(),
        ruta: rutas.gitea_tls_cert(),
    });
    let aviso_reintento = empujar_mirror(
        &bare_mirror,
        &url_destino,
        origen_credencial,
        credencial_gitea,
        certificado_ca,
    )
    .await
    .map_err(|origen| ErrorContingencia::FalloAlActivar {
        repo: id.clone(),
        paso: PasoActivar::Empujar,
        origen: origen.to_string(),
    })?;
    if let Some(aviso) = aviso_reintento {
        advertencias.push(aviso);
    }

    reconciliar_lan_mejor_esfuerzo(gitea).await;

    Ok(construir_resultado(
        cuenta,
        id,
        &id_contingencia,
        punto_de_partida,
        advertencias,
    ))
}

/// Reconcilia los permisos LAN tras activar una contingencia: la organización de
/// contingencia recién creada (o ya existente) recibe su equipo de escritura si ya hay
/// algún usuario LAN. Un fallo aquí nunca hace fallar la activación, que ya ha terminado
/// con éxito: solo se registra con `tracing::warn!`.
async fn reconciliar_lan_mejor_esfuerzo<T: ApiGitea>(gitea: &T) {
    if let Err(error) = crate::cuentas::reconciliar_permisos_lan(gitea, &[]).await {
        tracing::warn!(
            error = %error,
            "no se pudo reconciliar los permisos LAN tras activar una contingencia"
        );
    }
}

/// Activa secuencialmente todos los mirrors de `cuenta`. Un fallo en un repo no detiene
/// a los demás: se devuelve el resultado (éxito o error) de cada uno.
pub async fn activar_cuenta<T: ApiGitea>(
    gitea: &T,
    cuenta: &Cuenta,
    rutas: &crate::config::RutasCuenta,
    credencial_gitea: &CredencialGitea,
) -> Result<Vec<(IdRepo, Result<RepoContingencia, ErrorContingencia>)>, ErrorContingencia> {
    let repos = gitea.repos_de(&cuenta.login).await?;
    let mut resultados = Vec::with_capacity(repos.len());
    for repo in repos.into_iter().filter(|repo| repo.es_mirror) {
        let resultado = activar(gitea, cuenta, rutas, &repo.id, credencial_gitea).await;
        resultados.push((repo.id, resultado));
    }
    Ok(resultados)
}

fn construir_resultado(
    cuenta: &Cuenta,
    original: &IdRepo,
    contingencia: &IdRepo,
    punto_de_partida: PuntoDePartida,
    advertencias: Vec<String>,
) -> RepoContingencia {
    let url_remoto = format!(
        "{}/{}/{}.git",
        cuenta.url_gitea(),
        contingencia.dueno.as_str(),
        contingencia.nombre.as_str()
    );
    RepoContingencia {
        original: original.clone(),
        contingencia: contingencia.clone(),
        comando_remote: format!("git remote add mereba {url_remoto}"),
        url_remoto,
        punto_de_partida,
        advertencias,
    }
}

/// Empuja el contenido de `bare_mirror` a `url_destino` con `git push --mirror`, sin
/// nunca poner el directorio de trabajo del proceso dentro del bare (se usa
/// `--git-dir` y un directorio de trabajo neutral, para que quede claro que el bare del
/// mirror es de solo lectura para este módulo).
///
/// Si `--mirror` falla (Gitea puede rechazar refs internas como `refs/pull/*` que un
/// mirror de GitHub sí guarda), se reintenta con refspecs explícitos de ramas y tags, y
/// se devuelve un aviso describiéndolo.
async fn empujar_mirror(
    bare_mirror: &Path,
    url_destino: &str,
    origen_credencial: String,
    credencial: &CredencialGitea,
    certificado_ca: Option<CertificadoCa>,
) -> Result<Option<String>, ErrorGit> {
    let bare_str = bare_mirror
        .to_str()
        .ok_or_else(|| ErrorGit::EntradaInvalida("la ruta del bare no es UTF-8".to_string()))?;
    let git_dir_arg = format!("--git-dir={bare_str}");

    let scratch =
        DirTemporal::nueva(PREFIJO_TEMPORAL).map_err(|e| ErrorGit::Sistema(e.to_string()))?;
    let opciones = Opciones {
        directorio: Some(scratch.ruta().to_path_buf()),
        credencial: Some(Credencial {
            origen: origen_credencial,
            usuario: credencial.usuario.clone(),
            token: credencial.token.clone(),
        }),
        certificado_ca,
        ..Opciones::default()
    };

    match git::ejecutar(&[&git_dir_arg, "push", "--mirror", url_destino], &opciones).await {
        Ok(_) => Ok(None),
        Err(primero) => {
            git::ejecutar(
                &[
                    &git_dir_arg,
                    "push",
                    url_destino,
                    "+refs/heads/*:refs/heads/*",
                    "+refs/tags/*:refs/tags/*",
                ],
                &opciones,
            )
            .await?;
            Ok(Some(format!(
                "«git push --mirror» fue rechazado ({primero}); se completó con refspecs \
                 explícitos de ramas y tags (probablemente el servidor rechazó alguna ref \
                 interna del mirror, como «refs/pull/*»)"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use crate::config::RutasCuenta;
    use crate::modelo::{Alcance, Nombre, RepoLocal};

    use super::super::dobles::GiteaDoble;
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(repo),
        }
    }

    fn cuenta(login: &str, puerto: u16, carpeta: &std::path::Path) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: carpeta.to_path_buf(),
            puerto,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    fn repo_local(dueno: &str, nombre_repo: &str, es_mirror: bool, vacio: bool) -> RepoLocal {
        RepoLocal {
            id: id(dueno, nombre_repo),
            es_mirror,
            vacio,
            privado: true,
            tamano_kb: 10,
            ultima_sync: Some(OffsetDateTime::now_utc()),
        }
    }

    async fn crear_bare_con_commit(
        raiz: &std::path::Path,
        dueno: &str,
        nombre_repo: &str,
    ) -> std::path::PathBuf {
        let trabajo = tempfile::tempdir().expect("directorio de trabajo temporal");
        let opciones = Opciones {
            directorio: Some(trabajo.path().to_path_buf()),
            ..Opciones::default()
        };
        git::ejecutar(&["-c", "init.defaultBranch=main", "init", "-q"], &opciones)
            .await
            .expect("git init");
        git::ejecutar(
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test.invalid",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "inicial",
            ],
            &opciones,
        )
        .await
        .expect("git commit");

        let bare = raiz
            .join(dueno.to_lowercase())
            .join(format!("{}.git", nombre_repo.to_lowercase()));
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

    fn rutas_cuenta(carpeta: &std::path::Path) -> RutasCuenta {
        RutasCuenta::nueva(carpeta)
    }

    #[tokio::test]
    async fn falla_si_el_repo_no_existe_en_gitea() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let gitea = GiteaDoble::nueva();
        let cuenta = cuenta("jparga", 3000, temporal.path());
        let rutas = rutas_cuenta(temporal.path());
        let credencial = CredencialGitea {
            usuario: "admin".to_string(),
            token: Secreto::nuevo("t"),
        };

        let resultado = activar(&gitea, &cuenta, &rutas, &id("jparga", "repo1"), &credencial).await;
        assert!(matches!(resultado, Err(ErrorContingencia::RepoNoExiste(_))));
    }

    #[tokio::test]
    async fn falla_si_el_repo_no_es_un_mirror() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let gitea = GiteaDoble::nueva();
        gitea.con_repo(repo_local("jparga", "repo1", false, false));
        let cuenta = cuenta("jparga", 3000, temporal.path());
        let rutas = rutas_cuenta(temporal.path());
        let credencial = CredencialGitea {
            usuario: "admin".to_string(),
            token: Secreto::nuevo("t"),
        };

        let resultado = activar(&gitea, &cuenta, &rutas, &id("jparga", "repo1"), &credencial).await;
        assert!(matches!(resultado, Err(ErrorContingencia::NoEsMirror(_))));
    }

    #[tokio::test]
    async fn idempotente_si_el_repo_hermano_ya_existe_y_no_esta_vacio() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let carpeta = temporal.path().join("cuenta");
        let rutas = rutas_cuenta(&carpeta);
        crear_bare_con_commit(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        let gitea = GiteaDoble::nueva();
        gitea.con_repo(repo_local("jparga", "repo1", true, false));
        gitea.con_repo(repo_local("contingencia-jparga", "repo1", false, false));

        let cuenta = cuenta("jparga", 3000, &carpeta);
        let credencial = CredencialGitea {
            usuario: "admin".to_string(),
            token: Secreto::nuevo("t"),
        };

        let resultado = activar(&gitea, &cuenta, &rutas, &id("jparga", "repo1"), &credencial)
            .await
            .expect("activar debe ser idempotente");

        assert_eq!(resultado.contingencia, id("contingencia-jparga", "repo1"));
        assert!(
            resultado
                .url_remoto
                .ends_with("/contingencia-jparga/repo1.git")
        );
        assert!(
            gitea.intervalos_fijados().is_empty(),
            "no debe tocar el intervalo si ya está activada"
        );
    }

    #[tokio::test]
    async fn pausa_el_mirror_y_anota_el_punto_de_partida_antes_de_empujar() {
        // No hay Gitea real en esta prueba: el empuje fallará (no hay servidor
        // escuchando en el puerto), pero debe fallar EN el paso «Empujar», después de
        // haber pausado el mirror y creado el repo hermano (idempotencia probada aparte
        // contra un Gitea real en `contingencia_gitea_real.rs`).
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let carpeta = temporal.path().join("cuenta");
        let rutas = rutas_cuenta(&carpeta);
        crear_bare_con_commit(&rutas.gitea_repositorios(), "jparga", "repo1").await;

        let gitea = GiteaDoble::nueva();
        gitea.con_repo(repo_local("jparga", "repo1", true, false));

        let cuenta = cuenta("jparga", 1, &carpeta); // puerto 1: nadie escucha ahí.
        let credencial = CredencialGitea {
            usuario: "admin".to_string(),
            token: Secreto::nuevo("t"),
        };

        let resultado = activar(&gitea, &cuenta, &rutas, &id("jparga", "repo1"), &credencial).await;
        assert!(matches!(
            resultado,
            Err(ErrorContingencia::FalloAlActivar {
                paso: PasoActivar::Empujar,
                ..
            })
        ));
        assert_eq!(
            gitea.intervalos_fijados(),
            vec![(id("jparga", "repo1"), "0".to_string())]
        );
    }
}
