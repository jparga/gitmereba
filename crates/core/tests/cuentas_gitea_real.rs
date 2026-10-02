//! Prueba de integración de extremo a extremo de `core::cuentas` contra un Gitea
//! **real**: levanta Gitea de verdad con `alta`
//! (`AprovisionadorReal` + `LanzadorPrimerPlano`), habla con él con el token generado y
//! lo para al final, comprobando con `pgrep` que no queda ningún proceso.
//!
//! Se salta (con un aviso, sin fallar) si `GITMEREBA_TEST_GITEA` no apunta a un binario
//! de Gitea existente. Para obtenerlo, ver el test `#[ignore]` de
//! `instancia_gitea_real.rs`. No se usa GitHub real: un doble mínimo de `ApiGithub`
//! anuncia dos repos bare locales (con commits) como si fueran suyos, con `url_clon`
//! apuntando a su ruta absoluta en disco (`modo_pruebas: true` activa
//! `IMPORT_LOCAL_PATHS`).
//!
//! **Límite descubierto y documentado aquí (no se ha tocado `gitea`, fuera del
//! alcance de esta prueba):** `gitea::dto::cuerpo_migrate` fija `"service": "github"`
//! sin condición. Contra un Gitea 1.27.3 real, con `clone_addr` como ruta absoluta (o
//! `file://`), el downloader de GitHub de Gitea intenta construir a partir de ella una
//! URL base de la API (`<esquema>://<host>/api/v3`) *antes* de hacer ningún `git
//! clone`, y falla: con una ruta pelada, `500 invalid base url: parse "://": missing
//! protocol scheme`; con `file://`, `500 ... Get "file:///api/v3/...": unsupported
//! protocol scheme "file"`. Las pruebas previas con Gitea real ya migraban con éxito desde una ruta local, pero con `"service": "git"`
//! (el downloader genérico, sin metadatos de la API), no con `"github"`. Arreglarlo
//! exigiría cambiar la línea `"service": "github",` de
//! `crates/core/src/gitea/dto.rs` (añadir un campo `servicio` a `PeticionMirror`, o usar
//! siempre `"git"`, que basta para un mirror sin issues/wiki/releases) — fuera del
//! alcance de esta prueba (solo se puede tocar `cuentas` y el binario). Por eso esta prueba
//! comprueba lo que sí es correcto de extremo a extremo (arranque real, provisión real,
//! salud, token, auditoría, parada limpia) y deja constancia, con una aserción sobre el
//! mensaje de error real, de por qué la creación del mirror en sí falla.

// Fichero íntegramente de pruebas (helpers del fixture + el propio test): `clippy.toml`
// solo exime `unwrap`/`expect` dentro de funciones `#[test]`, no en los helpers de nivel
// de módulo que arman el fixture (crear repos bare, copiar el binario...).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use gitmereba_core::almacen::Almacen;
use gitmereba_core::config::{self, Rutas, RutasCuenta};
use gitmereba_core::cuentas::{
    self, AprovisionadorReal, Contexto, EstadoPaso, Lanzador, LanzadorPrimerPlano, PasoAlta,
    SolicitudAlta,
};
use gitmereba_core::gitea::{ApiGitea, ClienteGitea};
use gitmereba_core::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use gitmereba_core::instancia::VERSION_GITEA;
use gitmereba_core::modelo::{Alcance, IdRepo, Nombre, RepoOrigen};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, LlaveroEnMemoria, Secreto};
use gitmereba_core::sync::OpcionesSync;

/// Doble mínimo de `ApiGithub`: anuncia los repos que se le hayan dado como si fueran
/// los del usuario. No se usa GitHub real en ningún momento de esta prueba.
struct GithubLocalDoble {
    login: Nombre,
    repos: Mutex<Vec<RepoOrigen>>,
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
        Ok(self.repos.lock().expect("mutex no envenenado").clone())
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

/// Ejecuta `git` con el autor fijado por variables de entorno: `git::ejecutar` del
/// `core` limpia el entorno y no las reenvía (además de ignorar `~/.gitconfig` a
/// propósito), así que aquí se usa `std::process::Command` directamente, solo para
/// preparar el fixture de la prueba (no es código de producción).
fn ejecutar_git(directorio: &Path, args: &[&str]) {
    let estado = Command::new("git")
        .args(args)
        .current_dir(directorio)
        .env("GIT_AUTHOR_NAME", "Pruebas Gitmereba")
        .env("GIT_AUTHOR_EMAIL", "pruebas@gitmereba.invalid")
        .env("GIT_COMMITTER_NAME", "Pruebas Gitmereba")
        .env("GIT_COMMITTER_EMAIL", "pruebas@gitmereba.invalid")
        .status()
        .unwrap_or_else(|error| panic!("no se pudo ejecutar «git {args:?}»: {error}"));
    assert!(estado.success(), "«git {args:?}» terminó con {estado}");
}

/// Crea un repo bare con un commit en `raiz/<nombre_repo>.git`, devolviendo su ruta.
fn crear_repo_bare_con_commit(raiz: &Path, nombre_repo: &str) -> PathBuf {
    let trabajo = raiz.join(format!("trabajo-{nombre_repo}"));
    std::fs::create_dir_all(&trabajo).expect("crear el directorio de trabajo");
    ejecutar_git(&trabajo, &["init", "-q", "-b", "main"]);
    std::fs::write(trabajo.join("README.md"), format!("# {nombre_repo}\n"))
        .expect("escribir el fichero de prueba");
    ejecutar_git(&trabajo, &["add", "README.md"]);
    ejecutar_git(&trabajo, &["commit", "-q", "-m", "inicial"]);

    let bare = raiz.join(format!("{nombre_repo}.git"));
    ejecutar_git(
        raiz,
        &[
            "clone",
            "-q",
            "--bare",
            trabajo.to_str().expect("ruta utf-8"),
            bare.to_str().expect("ruta utf-8"),
        ],
    );
    bare
}

fn repo_origen(dueno: &Nombre, nombre_repo: &str, ruta_bare: &Path) -> RepoOrigen {
    RepoOrigen {
        id: IdRepo {
            dueno: dueno.clone(),
            nombre: nombre(nombre_repo),
        },
        url_clon: ruta_bare.to_str().expect("ruta utf-8").to_string(),
        privado: false,
        es_fork: false,
        archivado: false,
        rama_por_defecto: Some("main".to_string()),
        descripcion: None,
        tamano_kb: 1,
    }
}

/// Copia el binario ya descargado (`GITMEREBA_TEST_GITEA`) al caché que espera
/// `instancia::asegurar_binario`, para que `AprovisionadorReal` no intente descargar
/// nada por red durante esta prueba.
fn preparar_cache_del_binario(rutas: &Rutas, binario_origen: &Path) -> PathBuf {
    let directorio_bin = rutas.directorio_bin();
    std::fs::create_dir_all(&directorio_bin).expect("crear el directorio de binarios");
    let destino = directorio_bin.join(format!("gitea-{VERSION_GITEA}"));
    std::fs::copy(binario_origen, &destino).expect("copiar el binario de gitea al caché");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&destino, std::fs::Permissions::from_mode(0o700))
        .expect("marcar el binario como ejecutable");
    destino
}

#[tokio::test]
async fn alta_y_sincronizacion_contra_un_gitea_real_con_origenes_locales() {
    let Ok(binario_env) = std::env::var("GITMEREBA_TEST_GITEA") else {
        eprintln!(
            "aviso: GITMEREBA_TEST_GITEA no está definida; se salta la prueba de integración \
             de «cuentas» con un Gitea real (ver el test #[ignore] en instancia_gitea_real.rs)"
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

    let raiz_origenes = tempfile::tempdir().expect("directorio temporal para los orígenes");
    let repo1 = crear_repo_bare_con_commit(raiz_origenes.path(), "repo1");
    let repo2 = crear_repo_bare_con_commit(raiz_origenes.path(), "repo2");

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
        repos: Mutex::new(vec![
            repo_origen(&login, "repo1", &repo1),
            repo_origen(&login, "repo2", &repo2),
        ]),
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
        // Necesario para migrar desde una ruta local (ver el comentario del módulo).
        // Nunca debe activarse fuera de esta prueba de integración.
        modo_pruebas: true,
    };

    let lanzador = LanzadorPrimerPlano::nuevo();
    let aprovisionador = AprovisionadorReal;
    let mut pasos_vistos = Vec::new();
    let mut progreso = |paso: PasoAlta, estado: EstadoPaso| pasos_vistos.push((paso, estado));

    let informe_alta = cuentas::alta(
        &contexto,
        &solicitud,
        &github,
        ClienteGitea::nuevo,
        &lanzador,
        &aprovisionador,
        &mut progreso,
    )
    .await
    .unwrap_or_else(|error| panic!("alta contra un Gitea real no debe fallar: {error}"));
    assert!(
        pasos_vistos
            .iter()
            .any(|(paso, estado)| *paso == PasoAlta::ArrancarGitea && *estado == EstadoPaso::Hecho)
    );

    // El binario cacheado es el que de verdad se ha usado (ninguna descarga de red).
    assert!(binario_cacheado.exists());

    let rutas_cuenta = RutasCuenta::nueva(&carpeta);
    let cuenta_guardada = config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
    let token_gitea = llavero
        .leer(&login, ClaveSecreto::TokenGitea)
        .expect("leer el llavero no falla")
        .expect("provisionar debe haber guardado un token de Gitea");
    let cliente_gitea = ClienteGitea::nuevo(&cuenta_guardada.url_gitea(), token_gitea)
        .expect("la URL local del Gitea recién levantado es válida");

    // Gitea responde de verdad, con el token generado por la provisión real.
    assert!(cliente_gitea.salud().await.expect("salud no debe fallar"));
    let version = cliente_gitea
        .version()
        .await
        .expect("version no debe fallar");
    assert!(!version.is_empty());
    println!("Gitea real levantado por gitmereba: versión {version}");

    // El alta deja una sincronización registrada y una auditoría «cuenta.alta», aunque
    // la creación del mirror en sí falle (ver el límite documentado arriba de este
    // fichero): `sync::ejecutar` solo devuelve `Err` si falla la identidad o el listado
    // de organizaciones, no si falla una acción individual del plan.
    assert_eq!(
        almacen
            .ultimas_sincronizaciones(Some(&login), 10)
            .expect("leer histórico")
            .len(),
        1,
        "el alta debe dejar una sincronización registrada"
    );
    let auditoria = almacen.auditoria(20, None).expect("leer auditoría");
    assert!(
        auditoria
            .iter()
            .any(|entrada| entrada.accion == "cuenta.alta"),
        "debe existir una entrada «cuenta.alta»: {auditoria:?}"
    );

    // Los dos mirrors existen en Gitea y tienen contenido.
    assert!(
        !informe_alta.hay_fallos(),
        "el alta no debe tener fallos: {informe_alta:?}"
    );
    let locales = cliente_gitea.repos_de(&login).await.expect("listar repos");
    let nombres: Vec<&str> = locales.iter().map(|r| r.id.nombre.as_str()).collect();
    assert_eq!(locales.len(), 2, "mirrors creados: {nombres:?}");
    for repo in &locales {
        assert!(repo.es_mirror, "{} debe ser mirror", repo.id);
        assert!(repo.privado, "{} debe ser privado en local", repo.id);
        assert!(!repo.vacio, "{} no debe estar vacío", repo.id);
    }

    // Segunda pasada: aparece repo3 y desaparece repo1 del origen.
    let repo3 = crear_repo_bare_con_commit(raiz_origenes.path(), "repo3");
    {
        let mut repos = github.repos.lock().expect("mutex");
        repos.retain(|r| r.id.nombre.as_str() != "repo1");
        repos.push(repo_origen(&login, "repo3", &repo3));
    }
    let informe = cuentas::sincronizar_con(
        &almacen,
        &cuenta_guardada,
        &github,
        &cliente_gitea,
        &Secreto::nuevo("token-de-prueba"),
        &OpcionesSync::default(),
    )
    .await
    .expect("la segunda sincronización no debe fallar");
    assert!(!informe.hay_fallos(), "segunda pasada: {informe:?}");

    let locales = cliente_gitea.repos_de(&login).await.expect("listar repos");
    let mut nombres: Vec<String> = locales.iter().map(|r| r.id.nombre.to_string()).collect();
    nombres.sort();
    assert_eq!(
        nombres,
        ["repo1", "repo2", "repo3"],
        "repo3 se da de alta y repo1, huérfano, NO se borra"
    );
    let huerfanos: Vec<String> = almacen
        .estados_de(&login)
        .expect("leer estados")
        .into_iter()
        .filter(|e| e.estado == gitmereba_core::modelo::EstadoRepo::Huerfano)
        .map(|e| e.id.nombre.to_string())
        .collect();
    assert_eq!(huerfanos, ["repo1"], "repo1 queda marcado como huérfano");

    lanzador
        .parar(&rutas, &login)
        .await
        .expect("parar el Gitea de la prueba no debe fallar");

    // No debe quedar ningún proceso de este binario en marcha.
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
