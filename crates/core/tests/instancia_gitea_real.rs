//! Prueba de integración de extremo a extremo del módulo `instancia` contra un
//! Gitea **real**: provisiona una cuenta en un directorio temporal, la
//! arranca en primer plano y habla con ella por la API con el token generado.
//!
//! Se salta (sin fallar) si `GITMEREBA_TEST_GITEA` no apunta a un binario de Gitea
//! existente. Para obtenerlo con el propio código de la app (no a mano), hay un test
//! `#[ignore]` más abajo:
//!
//! ```text
//! cargo test --test instancia_gitea_real -- --ignored --nocapture \
//!     descarga_el_binario_real_de_gitea_para_las_pruebas
//! export GITMEREBA_TEST_GITEA=<la ruta que imprime>
//! cargo test --test instancia_gitea_real
//! ```

use std::path::PathBuf;

use gitmereba_core::config::{Rutas, RutasCuenta};
use gitmereba_core::gitea::{ApiGitea, ClienteGitea};
use gitmereba_core::instancia::{ParametrosProvision, ProcesoGitea, asegurar_binario, provisionar};
use gitmereba_core::modelo::Nombre;
use gitmereba_core::secretos::{ClaveSecreto, Llavero, LlaveroEnMemoria};

/// Fuera del repo y fuera de `/tmp` efímero de la sesión: sobrevive entre ejecuciones
/// para no tener que descargar el binario (~126 MB) más de una vez.
fn directorio_cache_binario() -> PathBuf {
    std::env::temp_dir().join("gitmereba-pruebas-gitea-binario")
}

/// Descarga y verifica (SHA-256 fijado + firma GPG) el binario real de Gitea con
/// `asegurar_binario`, para poder exportar `GITMEREBA_TEST_GITEA` a mano antes de
/// ejecutar el resto de este fichero. No se ejecuta en `cargo nextest run`
/// (`#[ignore]`): descarga de red y toca binarios del sistema (`gpgv`).
#[tokio::test]
#[ignore = "descarga ~126 MB de dl.gitea.com; ejecutar a mano para obtener GITMEREBA_TEST_GITEA"]
async fn descarga_el_binario_real_de_gitea_para_las_pruebas() {
    let rutas = Rutas::con_raiz(directorio_cache_binario());
    let ruta = asegurar_binario(&rutas)
        .await
        .expect("asegurar_binario debe descargar y verificar el binario real");
    println!("GITMEREBA_TEST_GITEA={}", ruta.display());
}

#[tokio::test]
async fn provisiona_arranca_y_habla_con_un_gitea_real() {
    let Ok(binario) = std::env::var("GITMEREBA_TEST_GITEA") else {
        eprintln!(
            "aviso: GITMEREBA_TEST_GITEA no está definida; se salta la prueba de integración \
             con un Gitea real (ver el test #[ignore] «descarga_el_binario_real_de_gitea_para_las_pruebas» \
             en este mismo fichero)"
        );
        return;
    };
    let binario = PathBuf::from(binario);
    assert!(
        binario.exists(),
        "GITMEREBA_TEST_GITEA no apunta a un fichero existente: {}",
        binario.display()
    );

    let carpeta = tempfile::tempdir().expect("directorio temporal");
    let rutas_cuenta = RutasCuenta::nueva(carpeta.path().join("cuenta"));
    let login = Nombre::nuevo("pruebas-jparga").expect("login de prueba válido");
    let llavero = LlaveroEnMemoria::nuevo();
    let puerto = gitmereba_core::instancia::puerto_libre(&[]).expect("hay un puerto libre");
    let run_user = std::env::var("USER").unwrap_or_else(|_| "usuario-de-pruebas".to_string());

    let parametros = ParametrosProvision {
        binario_gitea: &binario,
        rutas_cuenta: &rutas_cuenta,
        puerto,
        run_user: &run_user,
        login: &login,
        secretos: &llavero,
        // Solo para esta prueba: sin esto no se podría comprobar nada de forma
        // desatendida sin exponer Gitea a una red real. Nunca en producción.
        modo_pruebas: true,
    };

    let resultado = provisionar(&parametros)
        .await
        .expect("provisionar no debe fallar contra un Gitea real");
    assert!(
        resultado.app_ini_generado,
        "primera provisión: debe generar app.ini"
    );
    assert!(
        resultado.admin_creado,
        "primera provisión: debe crear el administrador"
    );
    assert!(
        resultado.token_generado,
        "primera provisión: debe generar el token"
    );

    let token = llavero
        .leer(&login, ClaveSecreto::TokenGitea)
        .expect("leer el llavero en memoria no falla")
        .expect("provisionar debe haber guardado un token");

    let url_base = format!("http://127.0.0.1:{puerto}");
    let app_ini = rutas_cuenta.gitea_app_ini();
    let directorio_trabajo = rutas_cuenta.gitea();

    let proceso = ProcesoGitea::arrancar_en_primer_plano(
        &binario,
        &app_ini,
        &directorio_trabajo,
        &url_base,
        None,
    )
    .await
    .expect("gitea debe arrancar y responder a /api/healthz en menos de 30 s");

    let cliente = ClienteGitea::nuevo(&url_base, token).expect("la URL local es válida");

    let version = cliente
        .version()
        .await
        .expect("version() debe responder con el token generado");
    assert!(!version.is_empty());
    println!("Gitea real levantado por gitmereba: versión {version}");

    let organizacion = Nombre::nuevo("gitmereba-pruebas").expect("nombre de organización válido");
    cliente.asegurar_organizacion(&organizacion).await.expect(
        "asegurar_organizacion debe funcionar con los scopes write:organization,write:repository",
    );

    // Hallazgo a documentar en el informe si esto llegara a devolver 403: la
    // documentación previa avisaba de que listar repos de una organización puede pedir `read:organization`
    // además de `write:organization`. Verificado a mano contra un Gitea 1.27.3 real
    // con exactamente estos scopes: `GET /orgs/{org}/repos` (lo que usa `repos_de`)
    // responde 200 sin `read:organization`, así que no hace falta añadirlo. Si esta
    // aserción fallara alguna vez con 403, el hallazgo sería el contrario y habría
    // que decidir entonces si se añade el scope (sin tocar el módulo `gitea` sin
    // confirmarlo primero).
    let repos = cliente.repos_de(&organizacion).await.expect(
        "repos_de (GET /orgs/{org}/repos) debe funcionar con los scopes mínimos \
         write:organization,write:repository, sin necesitar read:organization",
    );
    assert!(
        repos.is_empty(),
        "la organización recién creada no tiene repos todavía"
    );

    proceso
        .parar()
        .await
        .expect("parar el proceso de gitea no falla");
}
