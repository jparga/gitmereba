//! Prueba de integración de `cuentas::mover_carpeta` contra un Gitea **real**: da de alta
//! una cuenta de verdad (`AprovisionadorReal` + `LanzadorPrimerPlano`, con un doble de
//! GitHub que anuncia repos bare locales), la mueve a otra carpeta y comprueba que sigue
//! operativa. No toca systemd ni el llavero reales.
//!
//! Casos:
//! - (a) mismo sistema de ficheros: se renombra (el destino es un directorio vacío ya
//!   existente);
//! - (b) otro sistema de ficheros: el renombrado inyectado devuelve `EXDEV` y se copia,
//!   se verifica y se borra el origen;
//! - (c) un fallo inyectado después de mover y antes de confirmar (en las dos
//!   variantes) deja la cuenta operativa en la carpeta antigua y sin copia parcial.
//!
//! En todos se comprueba que Gitea responde y lista sus repos, que un `git push` al
//! Gitea local funciona (prueba de que los hooks de git se regeneraron con las rutas
//! nuevas) y que la configuración y las unidades apuntan a la carpeta correcta.
//!
//! Se salta (con un aviso, sin fallar) si `GITMEREBA_TEST_GITEA` no apunta a un binario
//! de Gitea existente.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use gitmereba_core::almacen::Almacen;
use gitmereba_core::config::{self, Rutas, RutasCuenta};
use gitmereba_core::cuentas::{
    self, AprovisionadorReal, AvisoMover, Contexto, ErrorCuentas, EstadoPaso, Lanzador,
    LanzadorPrimerPlano, ModoMover, ParametrosLanzamiento, PasoAlta, ProgresoMover, RenombradoReal,
    Renombrador, SolicitudAlta,
};
use gitmereba_core::gitea::{ApiGitea, ClienteGitea};
use gitmereba_core::github::{ApiGithub, ErrorGithub, EstadoServicio, Identidad};
use gitmereba_core::instancia::{self, ParametrosTemporizador, ParametrosUnidad, VERSION_GITEA};
use gitmereba_core::modelo::{Alcance, IdRepo, Nombre, RepoOrigen};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, LlaveroEnMemoria, Secreto};

const EJECUTABLE: &str = "/usr/bin/gitmereba-de-prueba";

struct GithubLocalDoble {
    login: Nombre,
    repos: Vec<RepoOrigen>,
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
        Ok(self.repos.clone())
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

/// Renombrado que falla con `EXDEV` (`CrossesDevices`) al mover el origen, como si el
/// destino estuviera en otro disco; el resto de renombrados son los reales.
struct RenombradoOtroDisco {
    origen: PathBuf,
}

impl Renombrador for RenombradoOtroDisco {
    fn renombrar(&self, de: &Path, a: &Path) -> io::Result<()> {
        if de == self.origen {
            return Err(io::Error::from(io::ErrorKind::CrossesDevices));
        }
        std::fs::rename(de, a)
    }
}

/// Envuelve un lanzador y hace fallar el primer `arrancar` en una carpeta concreta.
struct LanzadorQueFallaEn<'a> {
    interno: &'a LanzadorPrimerPlano,
    carpeta_que_falla: PathBuf,
    ya_fallo: Mutex<bool>,
}

impl Lanzador for LanzadorQueFallaEn<'_> {
    async fn arrancar(
        &self,
        rutas: &Rutas,
        parametros: &ParametrosLanzamiento<'_>,
    ) -> Result<(), ErrorCuentas> {
        let falla = parametros.carpeta_cuenta == self.carpeta_que_falla && {
            let mut ya = self.ya_fallo.lock().unwrap();
            !std::mem::replace(&mut *ya, true)
        };
        if falla {
            return Err(ErrorCuentas::Interno("fallo inyectado".to_string()));
        }
        self.interno.arrancar(rutas, parametros).await
    }

    async fn parar(&self, rutas: &Rutas, login: &Nombre) -> Result<(), ErrorCuentas> {
        self.interno.parar(rutas, login).await
    }
}

fn nombre(v: &str) -> Nombre {
    Nombre::nuevo(v).expect("nombre de prueba válido")
}

fn git_en(directorio: &Path, args: &[&str], token: Option<&Secreto>) -> bool {
    let mut comando = Command::new("git");
    comando
        .args(args)
        .current_dir(directorio)
        .env("GIT_AUTHOR_NAME", "Pruebas Gitmereba")
        .env("GIT_AUTHOR_EMAIL", "pruebas@gitmereba.invalid")
        .env("GIT_COMMITTER_NAME", "Pruebas Gitmereba")
        .env("GIT_COMMITTER_EMAIL", "pruebas@gitmereba.invalid")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(token) = token {
        // La credencial viaja por el entorno del hijo, nunca en la URL ni en los argumentos.
        comando
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: token {}", token.exponer()),
            );
    }
    comando.output().expect("ejecutar git").status.success()
}

fn git_ok(directorio: &Path, args: &[&str], token: Option<&Secreto>) {
    assert!(git_en(directorio, args, token), "«git {args:?}» falló");
}

fn crear_repo_bare_con_commit(raiz: &Path, nombre_repo: &str) -> PathBuf {
    let trabajo = raiz.join(format!("trabajo-{nombre_repo}"));
    std::fs::create_dir_all(&trabajo).expect("crear trabajo");
    git_ok(&trabajo, &["init", "-q", "-b", "main"], None);
    std::fs::write(trabajo.join("README.md"), format!("# {nombre_repo}\n")).expect("escribir");
    git_ok(&trabajo, &["add", "README.md"], None);
    git_ok(&trabajo, &["commit", "-q", "-m", "inicial"], None);
    let bare = raiz.join(format!("{nombre_repo}.git"));
    git_ok(
        raiz,
        &[
            "clone",
            "-q",
            "--bare",
            trabajo.to_str().expect("utf8"),
            bare.to_str().expect("utf8"),
        ],
        None,
    );
    bare
}

fn repo_origen(dueno: &Nombre, nombre_repo: &str, ruta_bare: &Path) -> RepoOrigen {
    RepoOrigen {
        id: IdRepo {
            dueno: dueno.clone(),
            nombre: nombre(nombre_repo),
        },
        url_clon: ruta_bare.to_str().expect("utf8").to_string(),
        privado: false,
        es_fork: false,
        archivado: false,
        rama_por_defecto: Some("main".to_string()),
        descripcion: None,
        tamano_kb: 1,
    }
}

/// Todo lo necesario para mover una cuenta recién dada de alta.
struct Fixture {
    _origenes: tempfile::TempDir,
    raiz: tempfile::TempDir,
    rutas: Rutas,
    llavero: LlaveroEnMemoria,
    almacen: Almacen,
    login: Nombre,
    carpeta: PathBuf,
    binario: PathBuf,
    lanzador: LanzadorPrimerPlano,
    /// Clon de trabajo del repo propio (no mirror) de Gitea, para probar `git push`.
    clon: PathBuf,
}

const ORG: &str = "org-pruebas";
const REPO_PROPIO: &str = "propio";

impl Fixture {
    async fn nuevo() -> Option<Self> {
        let Ok(binario_env) = std::env::var("GITMEREBA_TEST_GITEA") else {
            eprintln!("aviso: GITMEREBA_TEST_GITEA no está definida; se salta la prueba");
            return None;
        };
        let binario_env = PathBuf::from(binario_env);
        if !binario_env.exists() {
            eprintln!("aviso: GITMEREBA_TEST_GITEA no apunta a un fichero existente");
            return None;
        }

        let origenes = tempfile::tempdir().expect("orígenes");
        let repo1 = crear_repo_bare_con_commit(origenes.path(), "repo1");
        let repo2 = crear_repo_bare_con_commit(origenes.path(), "repo2");

        let raiz = tempfile::tempdir().expect("raíz");
        let rutas = Rutas::con_raiz(raiz.path());
        let directorio_bin = rutas.directorio_bin();
        std::fs::create_dir_all(&directorio_bin).expect("bin");
        let binario = directorio_bin.join(format!("gitea-{VERSION_GITEA}"));
        std::fs::copy(&binario_env, &binario).expect("copiar el binario");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binario, std::fs::Permissions::from_mode(0o700))
                .expect("chmod");
        }

        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén");
        let login = nombre("pruebas-jparga");
        let carpeta = raiz.path().join("cuenta");
        let github = GithubLocalDoble {
            login: login.clone(),
            repos: vec![
                repo_origen(&login, "repo1", &repo1),
                repo_origen(&login, "repo2", &repo2),
            ],
        };
        let solicitud = SolicitudAlta {
            login: login.clone(),
            token: Secreto::nuevo("token-de-github-sin-uso-real"),
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
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let mut sin_progreso = |_: PasoAlta, _: EstadoPaso| {};
        let informe = cuentas::alta(
            &contexto,
            &solicitud,
            &github,
            ClienteGitea::nuevo,
            &lanzador,
            &AprovisionadorReal,
            &mut sin_progreso,
        )
        .await
        .unwrap_or_else(|error| panic!("el alta no debe fallar: {error}"));
        assert!(!informe.hay_fallos(), "{informe:?}");

        let fixture = Self {
            _origenes: origenes,
            raiz,
            rutas,
            llavero,
            almacen,
            login,
            carpeta,
            binario,
            lanzador,
            clon: PathBuf::new(),
        };
        let mut fixture = fixture;
        fixture.crear_repo_propio_y_primer_push().await;
        fixture.escribir_unidades(&fixture.carpeta.clone());
        Some(fixture)
    }

    fn token_gitea(&self) -> Secreto {
        self.llavero
            .leer(&self.login, ClaveSecreto::TokenGitea)
            .expect("leer llavero")
            .expect("token de Gitea")
    }

    fn cliente(&self, carpeta: &Path) -> ClienteGitea {
        let cuenta = config::leer_cuenta(&RutasCuenta::nueva(carpeta)).expect("leer cuenta");
        ClienteGitea::nuevo(&cuenta.url_gitea(), self.token_gitea()).expect("cliente")
    }

    fn puerto(&self) -> u16 {
        self.puerto_de(&self.carpeta)
    }

    fn puerto_de(&self, carpeta: &Path) -> u16 {
        config::leer_cuenta(&RutasCuenta::nueva(carpeta))
            .expect("leer cuenta")
            .puerto
    }

    /// Crea un repo normal (no mirror) en Gitea, que sí tiene hooks, y hace el primer push.
    async fn crear_repo_propio_y_primer_push(&mut self) {
        let cliente = self.cliente(&self.carpeta);
        cliente
            .asegurar_organizacion(&nombre(ORG))
            .await
            .expect("crear organización");
        cliente
            .crear_repo(&nombre(ORG), &nombre(REPO_PROPIO), true)
            .await
            .expect("crear repo propio");

        let clon = self.raiz.path().join("clon-propio");
        std::fs::create_dir_all(&clon).expect("crear clon");
        git_ok(&clon, &["init", "-q", "-b", "main"], None);
        let url = format!("http://127.0.0.1:{}/{ORG}/{REPO_PROPIO}.git", self.puerto());
        git_ok(&clon, &["remote", "add", "origin", &url], None);
        self.clon = clon;
        self.commit_y_push("uno");
    }

    /// Hace un commit nuevo en el clon y lo empuja al Gitea local. Devuelve si el push
    /// funcionó.
    fn intentar_commit_y_push(&self, texto: &str) -> bool {
        std::fs::write(self.clon.join("fichero.txt"), texto).expect("escribir");
        git_ok(&self.clon, &["add", "fichero.txt"], None);
        git_ok(&self.clon, &["commit", "-q", "-m", texto], None);
        git_en(
            &self.clon,
            &["push", "-q", "origin", "main"],
            Some(&self.token_gitea()),
        )
    }

    fn commit_y_push(&self, texto: &str) {
        assert!(self.intentar_commit_y_push(texto), "el push debe funcionar");
    }

    /// Unidades de systemd tal como las dejaría el alta con `LanzadorSystemd`, escritas sin
    /// tocar systemd, para comprobar que el movimiento las reescribe.
    fn escribir_unidades(&self, carpeta: &Path) {
        let rutas_cuenta = RutasCuenta::nueva(carpeta);
        let app_ini = rutas_cuenta.gitea_app_ini();
        let trabajo = rutas_cuenta.gitea();
        let unidad = instancia::generar_unidad(&ParametrosUnidad {
            login: &self.login,
            binario_gitea: &self.binario,
            app_ini: &app_ini,
            directorio_trabajo: &trabajo,
            carpeta_cuenta: carpeta,
        });
        instancia::escribir_unidad(&self.rutas, &self.login, &unidad).expect("unidad");
        let parametros = ParametrosTemporizador {
            login: &self.login,
            ruta_ejecutable: Path::new(EJECUTABLE),
            carpeta_cuenta: carpeta,
            directorio_datos: self.rutas.directorio_datos(),
            intervalo_minutos: 10,
        };
        instancia::escribir_temporizador(
            &self.rutas,
            &self.login,
            &instancia::generar_servicio_sync(&parametros),
            &instancia::generar_timer_sync(&parametros),
        )
        .expect("temporizador");
    }

    fn texto_unidades(&self) -> String {
        let directorio = self.rutas.directorio_systemd_usuario();
        [
            instancia::nombre_unidad(&self.login),
            instancia::nombre_servicio_sync(&self.login),
        ]
        .iter()
        .map(|n| std::fs::read_to_string(directorio.join(n)).expect("leer unidad"))
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// Contenido de los hooks de Gitea del repo propio (rutas absolutas, sin secretos).
    fn hooks_del_repo_propio(&self, carpeta: &Path) -> String {
        let repo = RutasCuenta::nueva(carpeta)
            .gitea_repositorios()
            .join(ORG)
            .join(format!("{REPO_PROPIO}.git"));
        ["pre-receive.d", "update.d", "post-receive.d"]
            .iter()
            .map(|d| {
                std::fs::read_to_string(repo.join("hooks").join(d).join("gitea"))
                    .unwrap_or_else(|e| panic!("hook {d}: {e}"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn mover(
        &self,
        destino: &Path,
        renombrador: &impl Renombrador,
        progreso: &mut Vec<ProgresoMover>,
    ) -> Result<cuentas::ResultadoMover, ErrorCuentas> {
        let contexto = Contexto::nuevo(&self.rutas, &self.llavero, &self.almacen);
        cuentas::mover_carpeta_con(
            &contexto,
            &self.lanzador,
            &self.login,
            destino,
            Path::new(EJECUTABLE),
            &mut |p| progreso.push(p),
            renombrador,
        )
        .await
    }

    /// La cuenta está operativa en `carpeta` y solo en ella.
    async fn comprobar_operativa_en(&self, carpeta: &Path, otra: &Path) {
        let cuenta = config::leer_cuenta(&RutasCuenta::nueva(carpeta)).expect("gitmereba.toml");
        assert_eq!(cuenta.carpeta, carpeta);
        assert!(
            cuentas::leer_marca(&self.rutas, &self.login)
                .expect("marca")
                .is_none(),
            "no debe quedar la marca de movimiento en curso"
        );
        assert!(
            cuentas::CerrojoCuenta::tomar(&self.rutas, &self.login).is_ok(),
            "el cerrojo debe haberse liberado"
        );
        let indice = config::leer_indice_cuentas(&self.rutas).expect("índice");
        assert_eq!(indice.cuentas[self.login.as_str()].carpeta, carpeta);

        let rutas_cuenta = RutasCuenta::nueva(carpeta);
        let app_ini = std::fs::read_to_string(rutas_cuenta.gitea_app_ini()).expect("app.ini");
        assert!(app_ini.contains(carpeta.to_str().unwrap()));
        assert!(
            !app_ini.contains(otra.to_str().unwrap()),
            "app.ini apunta a la otra carpeta"
        );

        let unidades = self.texto_unidades();
        assert!(unidades.contains(carpeta.to_str().unwrap()), "{unidades}");
        assert!(!unidades.contains(otra.to_str().unwrap()), "{unidades}");

        let hooks = self.hooks_del_repo_propio(carpeta);
        assert!(
            hooks.contains(carpeta.to_str().unwrap()),
            "hooks sin la carpeta: {hooks}"
        );
        assert!(
            !hooks.contains(otra.to_str().unwrap()),
            "hooks con la otra: {hooks}"
        );

        let cliente = self.cliente(carpeta);
        assert!(cliente.salud().await.expect("salud"), "Gitea no responde");
        let mirrors = cliente.repos_de(&self.login).await.expect("repos");
        let mut nombres: Vec<String> = mirrors.iter().map(|r| r.id.nombre.to_string()).collect();
        nombres.sort();
        assert_eq!(nombres, ["repo1", "repo2"]);
        let propios = cliente
            .repos_de(&nombre(ORG))
            .await
            .expect("repos de la org");
        assert_eq!(propios.len(), 1);
        self.commit_y_push(&format!("tras-mover-{}", carpeta.display()));
    }

    fn entrada_auditoria(&self) -> Option<String> {
        self.almacen
            .auditoria(50, None)
            .expect("auditoría")
            .into_iter()
            .find(|e| e.accion == "cuenta.mover")
            .map(|e| format!("{e:?}"))
    }

    async fn terminar(self) {
        self.lanzador
            .parar(&self.rutas, &self.login)
            .await
            .expect("parar");
        let salida = Command::new("pgrep")
            .arg("-f")
            .arg(self.binario.to_str().unwrap())
            .output()
            .expect("pgrep");
        assert!(!salida.status.success(), "queda un proceso de gitea");
    }
}

#[tokio::test]
async fn mueve_por_renombrado_a_un_directorio_vacio() {
    let Some(f) = Fixture::nuevo().await else {
        return;
    };
    let hooks_antes = f.hooks_del_repo_propio(&f.carpeta);
    assert!(hooks_antes.contains(f.carpeta.to_str().unwrap()));
    let destino = f.raiz.path().join("destino-vacio");
    std::fs::create_dir(&destino).expect("crear destino vacío");
    let origen = f.carpeta.clone();

    let mut progreso = Vec::new();
    let resultado = f
        .mover(&destino, &RenombradoReal, &mut progreso)
        .await
        .unwrap_or_else(|error| panic!("mover no debe fallar: {error}"));

    assert_eq!(resultado.modo, ModoMover::Renombrado);
    assert!(resultado.avisos.is_empty(), "{:?}", resultado.avisos);
    assert_eq!(resultado.origen, origen);
    assert_eq!(resultado.destino, destino);
    assert!(!origen.exists(), "la carpeta antigua no debe existir");
    assert!(progreso.contains(&ProgresoMover::Renombrando));
    assert_eq!(progreso.last(), Some(&ProgresoMover::Terminado));
    f.comprobar_operativa_en(&destino, &origen).await;
    let auditoria = f.entrada_auditoria().expect("entrada cuenta.mover");
    assert!(auditoria.contains("modo=renombrado"), "{auditoria}");
    f.terminar().await;
}

#[tokio::test]
async fn mueve_por_copia_cuando_el_destino_esta_en_otro_sistema_de_ficheros() {
    let Some(f) = Fixture::nuevo().await else {
        return;
    };
    let destino = f.raiz.path().join("destino-nuevo");
    let origen = f.carpeta.clone();
    let renombrador = RenombradoOtroDisco {
        origen: origen.clone(),
    };

    let mut progreso = Vec::new();
    let resultado = f
        .mover(&destino, &renombrador, &mut progreso)
        .await
        .unwrap_or_else(|error| panic!("mover no debe fallar: {error}"));

    assert_eq!(resultado.modo, ModoMover::Copia);
    assert!(resultado.avisos.is_empty(), "{:?}", resultado.avisos);
    assert!(!origen.exists(), "la carpeta antigua no debe existir");
    assert!(
        !f.raiz
            .path()
            .join("destino-nuevo.gitmereba-parcial")
            .exists(),
        "no debe quedar la copia parcial"
    );
    assert!(
        progreso
            .iter()
            .any(|p| matches!(p, ProgresoMover::Copiando { ficheros, ficheros_total, .. } if ficheros == ficheros_total && *ficheros_total > 0))
    );
    assert!(
        progreso
            .iter()
            .any(|p| matches!(p, ProgresoMover::Verificando { hechos, total } if hechos == total && *total >= 3))
    );
    f.comprobar_operativa_en(&destino, &origen).await;
    let auditoria = f.entrada_auditoria().expect("entrada cuenta.mover");
    assert!(auditoria.contains("modo=copia"), "{auditoria}");
    f.terminar().await;
}

async fn fallo_antes_de_confirmar(copiando: bool) {
    let Some(f) = Fixture::nuevo().await else {
        return;
    };
    let destino = f.raiz.path().join("destino-nuevo");
    let origen = f.carpeta.clone();
    let lanzador_que_falla = LanzadorQueFallaEn {
        interno: &f.lanzador,
        carpeta_que_falla: destino.clone(),
        ya_fallo: Mutex::new(false),
    };
    let contexto = Contexto::nuevo(&f.rutas, &f.llavero, &f.almacen);
    let mut progreso = Vec::new();
    let resultado = if copiando {
        cuentas::mover_carpeta_con(
            &contexto,
            &lanzador_que_falla,
            &f.login,
            &destino,
            Path::new(EJECUTABLE),
            &mut |p| progreso.push(p),
            &RenombradoOtroDisco {
                origen: origen.clone(),
            },
        )
        .await
    } else {
        cuentas::mover_carpeta_con(
            &contexto,
            &lanzador_que_falla,
            &f.login,
            &destino,
            Path::new(EJECUTABLE),
            &mut |p| progreso.push(p),
            &RenombradoReal,
        )
        .await
    };

    let Err(ErrorCuentas::Interno(mensaje)) = resultado else {
        panic!("debía devolverse el error original: {resultado:?}");
    };
    assert_eq!(mensaje, "fallo inyectado");
    assert!(origen.exists());
    assert!(!destino.exists(), "el destino no debe existir");
    assert!(
        !f.raiz
            .path()
            .join("destino-nuevo.gitmereba-parcial")
            .exists(),
        "no debe quedar la copia parcial"
    );
    assert!(f.entrada_auditoria().is_none(), "no se ha confirmado nada");
    // La cuenta sigue operativa en la carpeta antigua: Gitea responde y los hooks (que en
    // el renombrado se habían regenerado hacia la carpeta nueva) vuelven a ser válidos.
    f.comprobar_operativa_en(&origen, &destino).await;
    f.terminar().await;
}

#[tokio::test]
async fn un_fallo_tras_copiar_y_antes_de_confirmar_deja_la_cuenta_en_la_carpeta_antigua() {
    fallo_antes_de_confirmar(true).await;
}

#[tokio::test]
async fn un_fallo_tras_renombrar_y_antes_de_confirmar_deshace_el_renombrado() {
    fallo_antes_de_confirmar(false).await;
}

/// El comando de Gitea por sí solo reescribe los hooks, sin arrancar Gitea en la carpeta
/// nueva (Gitea 1.27 también los repone al arrancar; esto demuestra que no dependemos de eso).
#[tokio::test]
async fn regenerar_hooks_reescribe_las_rutas_con_gitea_parado() {
    let Some(f) = Fixture::nuevo().await else {
        return;
    };
    f.lanzador
        .parar(&f.rutas, &f.login)
        .await
        .expect("parar Gitea");
    let nueva = f.raiz.path().join("movida-a-mano");
    std::fs::rename(&f.carpeta, &nueva).expect("mover a mano");
    let rutas_nueva = RutasCuenta::nueva(&nueva);
    let usuario = std::env::var("USER").expect("USER");
    instancia::regenerar_app_ini_conservando_secretos(
        &rutas_nueva,
        f.puerto_de(&nueva),
        &usuario,
        None,
    )
    .expect("repuntar app.ini");
    assert!(
        f.hooks_del_repo_propio(&nueva)
            .contains(f.carpeta.to_str().unwrap())
    );

    let app_ini = rutas_nueva.gitea_app_ini();
    instancia::regenerar_hooks(&f.binario, app_ini.to_str().unwrap(), &rutas_nueva.gitea())
        .await
        .expect("regenerar hooks");

    let hooks = f.hooks_del_repo_propio(&nueva);
    assert!(hooks.contains(nueva.to_str().unwrap()), "{hooks}");
    assert!(!hooks.contains(f.carpeta.to_str().unwrap()), "{hooks}");
    f.terminar().await;
}

#[test]
fn el_aviso_de_origen_sin_borrar_es_un_valor_tipado() {
    // Comprueba que los tipos públicos necesarios para informar del aviso están exportados.
    let aviso = AvisoMover::OrigenSinBorrar {
        ruta: PathBuf::from("/x"),
        motivo: gitmereba_core::idioma::TextoExterno::literal("boom"),
    };
    assert!(matches!(aviso, AvisoMover::OrigenSinBorrar { .. }));
}
