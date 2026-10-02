//! Prueba de integración de extremo a extremo de `core::contingencia` contra un Gitea
//! **real**: mismo patrón que `cuentas_gitea_real.rs` (se
//! salta con aviso si no está `GITMEREBA_TEST_GITEA`).
//!
//! Flujo: alta con un origen bare local (como en `cuentas_gitea_real.rs`) → `activar` →
//! se clona el repo de contingencia por HTTP con el token y se le hace un commit (DEBE
//! ACEPTARSE) → un push directo al mirror original sigue rechazado → `estado` ve 1
//! commit de más → `reconciliar` con `LocalParaPruebas` lo lleva al origen → `cerrar`
//! reanuda el mirror → `sincronizar_mirror` deja el commit en el mirror original. Se
//! comprueba también que las refs del bare del mirror no cambiaron durante `activar`, y
//! al final se para Gitea y se confirma con `pgrep` que no queda ningún proceso.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use gitmereba_core::almacen::Almacen;
use gitmereba_core::config::{self, Rutas, RutasCuenta};
use gitmereba_core::contingencia::{
    self, CredencialGitea, EstadoContingencia, OrigenReconciliacion, ResultadoRama,
};
use gitmereba_core::cuentas::{
    self, AprovisionadorReal, Contexto, EstadoPaso, Lanzador, LanzadorPrimerPlano, PasoAlta,
    SolicitudAlta,
};
use gitmereba_core::git;
use gitmereba_core::gitea::{ApiGitea, ClienteGitea};
use gitmereba_core::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use gitmereba_core::instancia::VERSION_GITEA;
use gitmereba_core::modelo::{Alcance, IdRepo, Nombre, RepoOrigen};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, LlaveroEnMemoria, Secreto};

/// Nombre del administrador que provisiona `instancia::provisionar` (constante privada
/// de ese módulo, duplicada aquí solo para construir la credencial de `git push`; no es
/// un secreto).
const NOMBRE_ADMIN: &str = "gitmereba-admin";

/// Doble mínimo de `ApiGithub`: anuncia el repo bare local como si fuera de GitHub. No se
/// usa GitHub real en ningún momento de esta prueba.
struct GithubLocalDoble {
    login: Nombre,
    repo: RepoOrigen,
}

impl ApiGithub for GithubLocalDoble {
    async fn identidad(&self) -> Result<Identidad, ErrorGithub> {
        Ok(Identidad {
            login: self.login.clone(),
            scopes: Vec::new(),
            caduca: None,
        })
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGithub> {
        Ok(Vec::new())
    }

    async fn repos_de_usuario(&self) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        Ok(vec![self.repo.clone()])
    }

    async fn repos_de_organizacion(&self, _org: &Nombre) -> Result<Vec<RepoOrigen>, ErrorGithub> {
        Ok(Vec::new())
    }

    async fn sha_de_rama(
        &self,
        _repo: &IdRepo,
        _rama: &str,
    ) -> Result<Option<String>, ErrorGithub> {
        Ok(None)
    }

    async fn estado_servicio(&self) -> Result<EstadoServicio, ErrorGithub> {
        Ok(EstadoServicio::Operativo)
    }
}

fn nombre(v: &str) -> Nombre {
    Nombre::nuevo(v).expect("nombre de prueba válido")
}

/// Ejecuta `git` directamente (no `gitmereba_core::git::ejecutar`, que limpia el entorno
/// y no admite credenciales en la URL): el fixture de esta prueba sí las necesita, igual
/// que en las pruebas manuales previas de mirror a repo.
fn ejecutar_git(directorio: &Path, args: &[&str]) {
    let estado = Command::new("git")
        .args(args)
        .current_dir(directorio)
        .env("GIT_AUTHOR_NAME", "Pruebas Gitmereba")
        .env("GIT_AUTHOR_EMAIL", "pruebas@gitmereba.invalid")
        .env("GIT_COMMITTER_NAME", "Pruebas Gitmereba")
        .env("GIT_COMMITTER_EMAIL", "pruebas@gitmereba.invalid")
        .status()
        .unwrap_or_else(|e| panic!("no se pudo ejecutar «git {args:?}»: {e}"));
    assert!(estado.success(), "«git {args:?}» terminó con {estado}");
}

/// Igual que [`ejecutar_git`], pero sin comprobar el resultado: se usa para el push que
/// esta prueba espera que Gitea rechace.
fn intentar_git(directorio: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(directorio)
        .env("GIT_AUTHOR_NAME", "Pruebas Gitmereba")
        .env("GIT_AUTHOR_EMAIL", "pruebas@gitmereba.invalid")
        .env("GIT_COMMITTER_NAME", "Pruebas Gitmereba")
        .env("GIT_COMMITTER_EMAIL", "pruebas@gitmereba.invalid")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn crear_repo_bare_con_commit(raiz: &Path, nombre_repo: &str) -> PathBuf {
    let trabajo = raiz.join(format!("trabajo-{nombre_repo}"));
    std::fs::create_dir_all(&trabajo).expect("crear el directorio de trabajo");
    ejecutar_git(&trabajo, &["init", "-q", "-b", "main"]);
    std::fs::write(trabajo.join("README.md"), format!("# {nombre_repo}\n"))
        .expect("escribir README");
    ejecutar_git(&trabajo, &["add", "README.md"]);
    ejecutar_git(&trabajo, &["commit", "-q", "-m", "inicial"]);

    let bare = raiz.join(format!("{nombre_repo}.git"));
    ejecutar_git(
        raiz,
        &[
            "clone",
            "-q",
            "--bare",
            trabajo.to_str().expect("utf8"),
            bare.to_str().expect("utf8"),
        ],
    );
    bare
}

fn preparar_cache_del_binario(rutas: &Rutas, binario_origen: &Path) -> PathBuf {
    let directorio_bin = rutas.directorio_bin();
    std::fs::create_dir_all(&directorio_bin).expect("crear el directorio de binarios");
    let destino = directorio_bin.join(format!("gitea-{VERSION_GITEA}"));
    std::fs::copy(binario_origen, &destino).expect("copiar el binario de gitea al caché");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&destino, std::fs::Permissions::from_mode(0o700))
        .expect("marcar como ejecutable");
    destino
}

fn sin_progreso(_paso: PasoAlta, _estado: EstadoPaso) {}

#[tokio::test]
async fn activar_reconciliar_y_cerrar_una_contingencia_contra_un_gitea_real() {
    let Ok(binario_env) = std::env::var("GITMEREBA_TEST_GITEA") else {
        eprintln!(
            "aviso: GITMEREBA_TEST_GITEA no está definida; se salta la prueba de integración \
             de «contingencia» con un Gitea real"
        );
        return;
    };
    let binario_env = PathBuf::from(binario_env);
    if !binario_env.exists() {
        eprintln!(
            "aviso: GITMEREBA_TEST_GITEA no apunta a un fichero existente: {}",
            binario_env.display()
        );
        return;
    }

    // --- alta (como en cuentas_gitea_real.rs): un Gitea real, un origen bare local. ---
    let raiz_origenes = tempfile::tempdir().expect("directorio temporal para los orígenes");
    let bare_origen = crear_repo_bare_con_commit(raiz_origenes.path(), "repo1");

    let raiz_datos = tempfile::tempdir().expect("directorio temporal de datos de la app");
    let rutas = Rutas::con_raiz(raiz_datos.path());
    let binario_cacheado = preparar_cache_del_binario(&rutas, &binario_env);

    let llavero = LlaveroEnMemoria::nuevo();
    let almacen = Almacen::en_memoria().expect("almacén en memoria");
    let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

    let login = nombre("pruebas-jparga");
    let carpeta = raiz_datos.path().join("cuenta");
    let github = GithubLocalDoble {
        login: login.clone(),
        repo: RepoOrigen {
            id: IdRepo {
                dueno: login.clone(),
                nombre: nombre("repo1"),
            },
            url_clon: bare_origen.to_str().expect("ruta utf-8").to_string(),
            privado: false,
            es_fork: false,
            archivado: false,
            rama_por_defecto: Some("main".to_string()),
            descripcion: None,
            tamano_kb: 1,
        },
    };

    let solicitud = SolicitudAlta {
        login: login.clone(),
        token: Secreto::nuevo("token-de-github-sin-uso-real-en-esta-prueba"),
        carpeta: carpeta.clone(),
        alcance: Alcance {
            incluir_forks: false,
            organizaciones: Vec::new(),
            excluidos: Vec::new(),
        },
        intervalo_minutos: 10,
        modo_pruebas: true,
    };

    let lanzador = LanzadorPrimerPlano::nuevo();
    let aprovisionador = AprovisionadorReal;
    let informe_alta = cuentas::alta(
        &contexto,
        &solicitud,
        &github,
        ClienteGitea::nuevo,
        &lanzador,
        &aprovisionador,
        &mut sin_progreso,
    )
    .await
    .unwrap_or_else(|error| panic!("alta contra un Gitea real no debe fallar: {error}"));
    assert!(
        !informe_alta.hay_fallos(),
        "el alta no debe tener fallos: {informe_alta:?}"
    );

    let rutas_cuenta = RutasCuenta::nueva(&carpeta);
    let cuenta = config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
    let token_gitea = llavero
        .leer(&login, ClaveSecreto::TokenGitea)
        .expect("leer el llavero no falla")
        .expect("provisionar debe haber guardado un token de Gitea");
    let cliente_gitea =
        ClienteGitea::nuevo(&cuenta.url_gitea(), token_gitea.clone()).expect("URL local válida");

    let id = IdRepo {
        dueno: login.clone(),
        nombre: nombre("repo1"),
    };

    // --- activar ---------------------------------------------------------
    let bare_mirror = rutas_cuenta
        .gitea_repositorios()
        .join(login.as_str())
        .join("repo1.git");
    let refs_mirror_antes = git::refs(&bare_mirror).await.expect("leer refs del mirror");

    let credencial = CredencialGitea {
        usuario: NOMBRE_ADMIN.to_string(),
        token: token_gitea.clone(),
    };
    let resultado = contingencia::activar(&cliente_gitea, &cuenta, &rutas_cuenta, &id, &credencial)
        .await
        .unwrap_or_else(|error| panic!("activar no debe fallar contra un Gitea real: {error}"));

    let refs_mirror_despues = git::refs(&bare_mirror).await.expect("leer refs del mirror");
    assert_eq!(
        refs_mirror_antes, refs_mirror_despues,
        "activar no debe tocar el bare del mirror"
    );
    assert!(
        resultado.advertencias.is_empty(),
        "{:?}",
        resultado.advertencias
    );
    assert_eq!(
        resultado.contingencia,
        IdRepo {
            dueno: nombre("contingencia-pruebas-jparga"),
            nombre: nombre("repo1")
        }
    );

    // --- clonar el repo de contingencia por HTTP y escribir: DEBE ACEPTARSE ----
    let url_contingencia_con_credenciales = format!(
        "http://{}:{}@127.0.0.1:{}/contingencia-pruebas-jparga/repo1.git",
        NOMBRE_ADMIN,
        token_gitea.exponer(),
        cuenta.puerto
    );
    let clon_contingencia = raiz_datos.path().join("clon-contingencia");
    ejecutar_git(
        raiz_datos.path(),
        &[
            "clone",
            "-q",
            &url_contingencia_con_credenciales,
            clon_contingencia.to_str().expect("utf8"),
        ],
    );
    std::fs::write(
        clon_contingencia.join("README.md"),
        "# repo1\nescrito durante la contingencia\n",
    )
    .expect("escribir en el clon de contingencia");
    ejecutar_git(
        &clon_contingencia,
        &["commit", "-q", "-am", "escritura de contingencia"],
    );
    assert!(
        intentar_git(&clon_contingencia, &["push", "-q", "origin", "main"]),
        "el push al repo de contingencia debe aceptarse"
    );
    let sha_de_contingencia = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&clon_contingencia)
            .output()
            .expect("rev-parse")
            .stdout,
    )
    .expect("utf8")
    .trim()
    .to_string();

    // --- un push directo al mirror original sigue rechazándose ---------------
    let url_mirror_con_credenciales = format!(
        "http://{}:{}@127.0.0.1:{}/pruebas-jparga/repo1.git",
        NOMBRE_ADMIN,
        token_gitea.exponer(),
        cuenta.puerto
    );
    let clon_mirror = raiz_datos.path().join("clon-mirror");
    ejecutar_git(
        raiz_datos.path(),
        &[
            "clone",
            "-q",
            &url_mirror_con_credenciales,
            clon_mirror.to_str().expect("utf8"),
        ],
    );
    std::fs::write(
        clon_mirror.join("README.md"),
        "cambio que nunca debe llegar\n",
    )
    .expect("escribir en el clon del mirror");
    ejecutar_git(
        &clon_mirror,
        &["commit", "-q", "-am", "intento de escritura en el mirror"],
    );
    assert!(
        !intentar_git(&clon_mirror, &["push", "-q", "origin", "main"]),
        "el push directo al mirror debe seguir rechazándose"
    );

    // --- estado: 1 commit de más en «main» ------------------------------------
    let estado: EstadoContingencia =
        contingencia::estado(&rutas_cuenta, &id, &resultado.punto_de_partida)
            .await
            .expect("estado no debe fallar");
    assert!(estado.hay_commits_de_mas());
    let rama_main = estado
        .ramas
        .iter()
        .find(|r| r.rama == "main")
        .expect("rama main presente");
    assert_eq!(rama_main.commits_de_mas, 1);
    assert!(!rama_main.es_nueva);

    // --- reconciliar hacia el origen (bare local, «GitHub» de pruebas) --------
    let informe_reconciliacion = contingencia::reconciliar(
        &rutas_cuenta,
        &id,
        &resultado.punto_de_partida,
        &OrigenReconciliacion::LocalParaPruebas(bare_origen.clone()),
        &Secreto::nuevo("no-se-usa-con-un-origen-local"),
    )
    .await
    .expect("reconciliar no debe fallar");
    assert!(
        informe_reconciliacion.completa,
        "{informe_reconciliacion:?}"
    );
    assert_eq!(
        informe_reconciliacion.ramas,
        vec![ResultadoRama::Enviada {
            rama: "main".to_string()
        }]
    );

    let refs_origen = git::refs(&bare_origen).await.expect("leer refs del origen");
    let sha_origen_main = refs_origen
        .iter()
        .find(|(nombre, _)| nombre == "refs/heads/main")
        .map(|(_, sha)| sha.clone())
        .expect("main en el origen");
    assert_eq!(
        sha_origen_main, sha_de_contingencia,
        "el origen debe tener el commit reconciliado"
    );

    // --- cerrar: reanuda el mirror original -----------------------------------
    let resultado_cierre = contingencia::cerrar(
        &cliente_gitea,
        &rutas_cuenta,
        &id,
        cuenta.intervalo_minutos,
        &resultado.punto_de_partida,
        Some(&informe_reconciliacion),
    )
    .await
    .expect("cerrar no debe fallar");
    assert!(resultado_cierre.pendiente_de_borrar);

    // --- tras sincronizar_mirror, el mirror original contiene el commit ------
    cliente_gitea
        .sincronizar_mirror(&id)
        .await
        .expect("sincronizar_mirror no debe fallar");
    let mut visto = false;
    for _ in 0..40 {
        let refs_mirror = git::refs(&bare_mirror).await.expect("leer refs del mirror");
        if refs_mirror
            .iter()
            .any(|(nombre, sha)| nombre == "refs/heads/main" && sha == &sha_de_contingencia)
        {
            visto = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        visto,
        "el mirror original debe contener el commit reconciliado tras sincronizar_mirror"
    );

    // --- limpieza: parar Gitea y comprobar que no queda ningún proceso -------
    lanzador
        .parar(&rutas, &login)
        .await
        .expect("parar Gitea no debe fallar");

    let salida_pgrep = Command::new("pgrep")
        .arg("-f")
        .arg(binario_cacheado.to_str().expect("ruta utf-8"))
        .output()
        .expect("ejecutar pgrep");
    assert!(
        !salida_pgrep.status.success(),
        "sigue habiendo un proceso de gitea en marcha: {}",
        String::from_utf8_lossy(&salida_pgrep.stdout)
    );
}
