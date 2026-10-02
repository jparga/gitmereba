//! Prueba de integración de extremo a extremo del acceso LAN contra un Gitea
//! **real**: da de alta una cuenta en primer plano (como `cuentas_gitea_real.rs`),
//! activa el acceso LAN, comprueba que `https://127.0.0.1:<puerto>/api/v1/version`
//! responde con el cliente que confía solo en el certificado generado, que un cliente
//! reqwest SIN ese certificado falla la verificación TLS, y que tras desactivar vuelve a
//! responder por HTTP en loopback.
//!
//! Se salta (con un aviso, sin fallar) si `GITMEREBA_TEST_GITEA` no apunta a un binario
//! de Gitea existente (ver el test `#[ignore]` de `instancia_gitea_real.rs`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

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

/// Doble mínimo de `ApiGithub`, sin ningún repo: esta prueba no necesita clonar nada,
/// solo un Gitea real al que activar y desactivar el acceso LAN.
struct GithubSinRepos {
    login: Nombre,
}

impl ApiGithub for GithubSinRepos {
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
        Ok(Vec::new())
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

/// Copia el binario ya descargado (`GITMEREBA_TEST_GITEA`) al caché que espera
/// `instancia::asegurar_binario` (y `cuentas::exponer_lan`/`ocultar_lan`, que reconstruyen
/// la misma ruta), para no depender de la red en esta prueba.
fn preparar_cache_del_binario(rutas: &Rutas, binario_origen: &std::path::Path) -> PathBuf {
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
async fn activar_y_desactivar_el_acceso_lan_contra_un_gitea_real() {
    let Ok(binario_env) = std::env::var("GITMEREBA_TEST_GITEA") else {
        eprintln!(
            "aviso: GITMEREBA_TEST_GITEA no está definida; se salta la prueba de integración \
             de acceso LAN con un Gitea real (ver el test #[ignore] en instancia_gitea_real.rs)"
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

    let raiz_datos = tempfile::tempdir().expect("directorio temporal de datos de la app");
    let rutas = Rutas::con_raiz(raiz_datos.path());
    let binario_cacheado = preparar_cache_del_binario(&rutas, &binario_env);

    let llavero = LlaveroEnMemoria::nuevo();
    let almacen = Almacen::en_memoria().expect("almacén en memoria");
    let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

    let login = nombre("pruebas-lan");
    let carpeta = raiz_datos.path().join("cuenta");
    let github = GithubSinRepos {
        login: login.clone(),
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
        modo_pruebas: false,
    };

    let lanzador = LanzadorPrimerPlano::nuevo();
    let aprovisionador = AprovisionadorReal;
    let mut pasos_vistos = Vec::new();
    let mut progreso = |paso: PasoAlta, estado: EstadoPaso| pasos_vistos.push((paso, estado));

    cuentas::alta(
        &contexto,
        &solicitud,
        &github,
        ClienteGitea::nuevo,
        &lanzador,
        &aprovisionador,
        &mut progreso,
    )
    .await
    .unwrap_or_else(|error| panic!("el alta contra un Gitea real no debe fallar: {error}"));
    assert!(
        pasos_vistos
            .iter()
            .any(|(paso, estado)| *paso == PasoAlta::ArrancarGitea && *estado == EstadoPaso::Hecho)
    );
    assert!(binario_cacheado.exists());

    let rutas_cuenta = RutasCuenta::nueva(&carpeta);
    let token_gitea = llavero
        .leer(&login, ClaveSecreto::TokenGitea)
        .expect("leer el llavero no falla")
        .expect("provisionar debe haber guardado un token de Gitea");

    // --- activar el acceso LAN --------------------------------------------------

    let informe_activacion = cuentas::exponer_lan(&contexto, &lanzador, &login, None)
        .await
        .expect("exponer_lan contra un Gitea real no debe fallar");
    let host = informe_activacion
        .host
        .as_ref()
        .expect("el informe de activación lleva un host");
    assert_eq!(host.as_str(), "pruebas-lan.gitmereba.internal");
    assert!(informe_activacion.huella_sha256.is_some());
    let ruta_certificado = informe_activacion
        .ruta_certificado
        .clone()
        .expect("el informe lleva la ruta del certificado");
    assert!(ruta_certificado.exists());

    let cuenta_con_lan = config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
    assert!(cuenta_con_lan.lan.is_some());
    assert_eq!(
        cuenta_con_lan.url_gitea(),
        format!("https://127.0.0.1:{}", cuenta_con_lan.puerto)
    );

    // El cliente que confía SOLO en el certificado generado sí puede hablar con Gitea.
    let certificado_pem = std::fs::read(&ruta_certificado).expect("leer el certificado generado");
    let cliente_con_certificado = ClienteGitea::nuevo_con_certificado(
        &cuenta_con_lan.url_gitea(),
        token_gitea.clone(),
        &certificado_pem,
    )
    .expect("el cliente con el certificado pinned se construye");
    let version = cliente_con_certificado
        .version()
        .await
        .expect("version() debe responder por HTTPS con el certificado generado");
    assert!(!version.is_empty());
    println!("Gitea real (LAN) respondió a version(): {version}");

    // Un cliente reqwest normal, SIN el certificado, debe fallar la verificación TLS
    // (el certificado es autofirmado: ninguna CA del sistema lo reconoce).
    let cliente_sin_certificado = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("construir un cliente reqwest normal");
    let url_version = format!("{}/api/v1/version", cuenta_con_lan.url_gitea());
    let resultado_sin_certificado = cliente_sin_certificado.get(&url_version).send().await;
    match resultado_sin_certificado {
        Err(error) => assert!(
            error.is_connect() || error.to_string().to_lowercase().contains("certificate"),
            "se esperaba un fallo de verificación TLS, no: {error}"
        ),
        Ok(respuesta) => panic!(
            "un cliente sin el certificado NUNCA debe poder hablar con el Gitea expuesto: {}",
            respuesta.status()
        ),
    }

    // --- desactivar el acceso LAN ------------------------------------------------

    let informe_desactivacion = cuentas::ocultar_lan(&contexto, &lanzador, &login)
        .await
        .expect("ocultar_lan contra un Gitea real no debe fallar");
    assert_eq!(informe_desactivacion.host, None);
    // El certificado se conserva (no invalida la confianza ya repartida).
    assert!(ruta_certificado.exists());

    let cuenta_sin_lan = config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
    assert!(cuenta_sin_lan.lan.is_none());
    assert_eq!(
        cuenta_sin_lan.url_gitea(),
        format!("http://127.0.0.1:{}", cuenta_sin_lan.puerto)
    );

    let cliente_http = ClienteGitea::nuevo(&cuenta_sin_lan.url_gitea(), token_gitea)
        .expect("el cliente HTTP local se construye");
    assert!(
        cliente_http.salud().await.expect("salud() no debe fallar"),
        "tras desactivar, Gitea debe volver a responder por HTTP en loopback"
    );

    // --- limpieza ------------------------------------------------------------

    lanzador
        .parar(&rutas, &login)
        .await
        .expect("parar el Gitea de la prueba no debe fallar");

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
