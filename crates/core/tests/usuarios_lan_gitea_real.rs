//! Prueba de integración de extremo a extremo de los usuarios de la LAN contra
//! un Gitea **real**: mismo patrón que `lan_gitea_real.rs` (se salta con aviso si no está
//! `GITMEREBA_TEST_GITEA`).
//!
//! Flujo: alta en primer plano de una cuenta sin repos (como en `lan_gitea_real.rs`) →
//! se aseguran directamente dos organizaciones («acme», un mirror, y
//! «contingencia-acme», una organización de contingencia) → se da de alta un usuario LAN
//! → se comprueba por la API que es miembro de `lan-lectura` en «acme» y de
//! `lan-escritura` en «contingencia-acme» → `eliminar_usuario_lan` lo borra → ya no
//! aparece en `listar_usuarios_lan`. Al final se para Gitea y se confirma con `pgrep`
//! que no queda ningún proceso.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use gitmereba_core::almacen::Almacen;
use gitmereba_core::config::{Rutas, RutasCuenta};
use gitmereba_core::cuentas::{
    self, AprovisionadorReal, Contexto, EstadoPaso, Lanzador, LanzadorPrimerPlano, PasoAlta,
    SolicitudAlta, crear_usuario_lan, eliminar_usuario_lan, listar_usuarios_lan,
};
use gitmereba_core::gitea::{ApiGitea, ClienteGitea};
use gitmereba_core::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use gitmereba_core::instancia::VERSION_GITEA;
use gitmereba_core::modelo::{Alcance, IdRepo, Nombre, RepoOrigen};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, LlaveroEnMemoria, Secreto};

/// Doble mínimo de `ApiGithub`, sin ningún repo: esta prueba no necesita clonar nada,
/// solo un Gitea real al que dar de alta usuarios de la LAN.
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

fn sin_progreso(_paso: PasoAlta, _estado: EstadoPaso) {}

#[tokio::test]
async fn usuarios_lan_de_extremo_a_extremo_contra_un_gitea_real() {
    let Ok(binario_env) = std::env::var("GITMEREBA_TEST_GITEA") else {
        eprintln!(
            "aviso: GITMEREBA_TEST_GITEA no está definida; se salta la prueba de integración \
             de usuarios LAN (ver el test #[ignore] en instancia_gitea_real.rs)"
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
    preparar_cache_del_binario(&rutas, &binario_env);

    let llavero = LlaveroEnMemoria::nuevo();
    let almacen = Almacen::en_memoria().expect("almacén en memoria");
    let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

    let login = nombre("pruebas-usuarios-lan");
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
        modo_pruebas: true,
    };

    let lanzador = LanzadorPrimerPlano::nuevo();
    let aprovisionador = AprovisionadorReal;
    cuentas::alta(
        &contexto,
        &solicitud,
        &github,
        ClienteGitea::nuevo,
        &lanzador,
        &aprovisionador,
        &mut sin_progreso,
    )
    .await
    .unwrap_or_else(|error| panic!("el alta contra un Gitea real no debe fallar: {error}"));

    let rutas_cuenta = RutasCuenta::nueva(&carpeta);
    let cuenta = gitmereba_core::config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
    let token_gitea = llavero
        .leer(&login, ClaveSecreto::TokenGitea)
        .expect("leer el llavero no falla")
        .expect("provisionar debe haber guardado un token de Gitea");
    let gitea = ClienteGitea::nuevo(&cuenta.url_gitea(), token_gitea).expect("URL local válida");

    // --- dos organizaciones: un mirror y una de contingencia -------------------
    let org_mirror = nombre("acme");
    let org_contingencia = nombre("contingencia-acme");
    gitea
        .asegurar_organizacion(&org_mirror)
        .await
        .expect("asegurar la organización mirror no debe fallar");
    gitea
        .asegurar_organizacion(&org_contingencia)
        .await
        .expect("asegurar la organización de contingencia no debe fallar");

    // --- alta de un usuario LAN --------------------------------------------------
    let creado = crear_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
        .await
        .unwrap_or_else(|error| panic!("crear_usuario_lan no debe fallar: {error}"));
    assert_eq!(creado.nombre, nombre("ana"));
    assert!(!creado.password.esta_vacio());

    // --- comprobación por la API: miembro de lan-lectura y lan-escritura --------
    let id_lectura = gitea
        .buscar_equipo(&org_mirror, "lan-lectura")
        .await
        .expect("buscar el equipo de lectura no debe fallar")
        .expect("el equipo de lectura debe existir tras crear el usuario LAN");
    let miembros_lectura = gitea
        .miembros_equipo(id_lectura)
        .await
        .expect("listar miembros no debe fallar");
    assert!(
        miembros_lectura.contains(&nombre("ana")),
        "«ana» debe ser miembro de lan-lectura en «acme»: {miembros_lectura:?}"
    );

    let id_escritura = gitea
        .buscar_equipo(&org_contingencia, "lan-escritura")
        .await
        .expect("buscar el equipo de escritura no debe fallar")
        .expect("el equipo de escritura debe existir tras crear el usuario LAN");
    let miembros_escritura = gitea
        .miembros_equipo(id_escritura)
        .await
        .expect("listar miembros no debe fallar");
    assert!(
        miembros_escritura.contains(&nombre("ana")),
        "«ana» debe ser miembro de lan-escritura en «contingencia-acme»: {miembros_escritura:?}"
    );

    let lan_antes = listar_usuarios_lan(&contexto, &login)
        .await
        .expect("listar_usuarios_lan no debe fallar");
    assert!(lan_antes.iter().any(|u| u.nombre == nombre("ana")));

    // --- baja del usuario LAN ----------------------------------------------------
    eliminar_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
        .await
        .unwrap_or_else(|error| panic!("eliminar_usuario_lan no debe fallar: {error}"));

    let lan_despues = listar_usuarios_lan(&contexto, &login)
        .await
        .expect("listar_usuarios_lan no debe fallar");
    assert!(
        !lan_despues.iter().any(|u| u.nombre == nombre("ana")),
        "«ana» no debe seguir apareciendo tras eliminar_usuario_lan: {lan_despues:?}"
    );

    // --- limpieza: parar Gitea y comprobar que no queda ningún proceso ---------
    lanzador
        .parar(&rutas, &login)
        .await
        .expect("parar el Gitea de la prueba no debe fallar");

    let binario_cacheado = rutas
        .directorio_bin()
        .join(format!("gitea-{VERSION_GITEA}"));
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
