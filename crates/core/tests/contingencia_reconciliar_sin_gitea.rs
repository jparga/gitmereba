//! Pruebas de integración de `contingencia::reconciliar` con repos git reales en un
//! directorio temporal, SIN Gitea de por medio: un bare local hace de «GitHub»
//! (`OrigenReconciliacion::LocalParaPruebas`). Cubre:
//! avance rápido, divergencia (con el remoto intacto), rama nueva, rama borrada que no se
//! borra en remoto, tag en conflicto que no se toca, y que el token de escritura no
//! aparece en ningún fichero del árbol de trabajo ni en el informe.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use gitmereba_core::config::RutasCuenta;
use gitmereba_core::contingencia::{
    OrigenReconciliacion, PuntoDePartida, ResultadoRama, ResultadoTag, reconciliar,
};
use gitmereba_core::modelo::{IdRepo, Nombre};
use gitmereba_core::secretos::Secreto;

const TOKEN_DE_PRUEBA: &str = "ghp_token-de-escritura-que-nunca-debe-quedar-en-disco";

fn nombre(v: &str) -> Nombre {
    Nombre::nuevo(v).expect("nombre de prueba válido")
}

fn id(dueno: &str, repo: &str) -> IdRepo {
    IdRepo {
        dueno: nombre(dueno),
        nombre: nombre(repo),
    }
}

/// Ejecuta `git` directamente (no `gitmereba_core::git::ejecutar`, que limpia el entorno
/// a propósito): estos helpers de prueba necesitan fijar el autor con `GIT_AUTHOR_*` /
/// `GIT_COMMITTER_*`, tal como pide la tarea.
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

fn salida_git(directorio: &Path, args: &[&str]) -> String {
    let salida = Command::new("git")
        .args(args)
        .current_dir(directorio)
        .output()
        .unwrap_or_else(|e| panic!("no se pudo ejecutar «git {args:?}»: {e}"));
    assert!(
        salida.status.success(),
        "«git {args:?}» terminó con {}",
        salida.status
    );
    String::from_utf8(salida.stdout)
        .expect("salida utf8")
        .trim()
        .to_string()
}

fn refs_de(bare: &Path) -> BTreeMap<String, String> {
    let salida = salida_git(bare, &["for-each-ref", "--format=%(refname) %(objectname)"]);
    salida
        .lines()
        .filter_map(|linea| linea.split_once(' '))
        .map(|(n, s)| (n.to_string(), s.to_string()))
        .collect()
}

/// Crea el origen (`origin.git`, hace de «GitHub») y el bare de contingencia
/// (`<raiz>/cuenta/gitea/repositories/contingencia-jparga/repo1.git`), ambos con un
/// commit inicial en `main`. Devuelve la ruta del origen, las rutas de la cuenta y el
/// punto de partida (coincide con el estado inicial de ambos).
fn preparar(raiz: &Path) -> (PathBuf, RutasCuenta, PuntoDePartida) {
    let origin = raiz.join("origin.git");
    ejecutar_git(
        raiz,
        &[
            "init",
            "--bare",
            "-q",
            "-b",
            "main",
            origin.to_str().expect("utf8"),
        ],
    );

    let trabajo = raiz.join("trabajo-origen");
    ejecutar_git(
        raiz,
        &[
            "clone",
            "-q",
            origin.to_str().expect("utf8"),
            trabajo.to_str().expect("utf8"),
        ],
    );
    std::fs::write(trabajo.join("README.md"), "hola\n").expect("escribir README");
    ejecutar_git(&trabajo, &["add", "README.md"]);
    ejecutar_git(&trabajo, &["commit", "-q", "-m", "inicial"]);
    ejecutar_git(&trabajo, &["push", "-q", "origin", "main"]);
    let sha_inicial = salida_git(&trabajo, &["rev-parse", "HEAD"]);

    let rutas = RutasCuenta::nueva(raiz.join("cuenta"));
    let bare_contingencia = rutas
        .gitea_repositorios()
        .join("contingencia-jparga")
        .join("repo1.git");
    std::fs::create_dir_all(bare_contingencia.parent().expect("padre"))
        .expect("crear carpeta del dueño");
    ejecutar_git(
        raiz,
        &[
            "clone",
            "--bare",
            "-q",
            origin.to_str().expect("utf8"),
            bare_contingencia.to_str().expect("utf8"),
        ],
    );

    let punto = PuntoDePartida {
        refs: [("refs/heads/main".to_string(), sha_inicial)].into(),
    };
    (origin, rutas, punto)
}

fn bare_contingencia_de(rutas: &RutasCuenta) -> PathBuf {
    rutas
        .gitea_repositorios()
        .join("contingencia-jparga")
        .join("repo1.git")
}

/// Clona `bare`, hace un commit y lo empuja de vuelta a `rama` (creándola si no existe).
fn anadir_commit(raiz_trabajo: &Path, bare: &Path, rama: &str, mensaje: &str) -> String {
    ejecutar_git(
        bare.parent().expect("padre"),
        &[
            "clone",
            "-q",
            bare.to_str().expect("utf8"),
            raiz_trabajo.to_str().expect("utf8"),
        ],
    );
    let existe_local = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{rama}")])
        .current_dir(raiz_trabajo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !existe_local {
        ejecutar_git(raiz_trabajo, &["checkout", "-q", "-b", rama]);
    }
    ejecutar_git(
        raiz_trabajo,
        &["commit", "-q", "--allow-empty", "-m", mensaje],
    );
    ejecutar_git(raiz_trabajo, &["push", "-q", "origin", rama]);
    salida_git(raiz_trabajo, &["rev-parse", "HEAD"])
}

/// Busca `patron` (bytes) en el contenido de todos los ficheros bajo `raiz`, recursivamente.
fn algun_fichero_contiene(raiz: &Path, patron: &[u8]) -> bool {
    let mut pendientes = vec![raiz.to_path_buf()];
    while let Some(actual) = pendientes.pop() {
        let Ok(entradas) = std::fs::read_dir(&actual) else {
            continue;
        };
        for entrada in entradas.flatten() {
            let ruta = entrada.path();
            if ruta.is_dir() {
                pendientes.push(ruta);
            } else if let Ok(contenido) = std::fs::read(&ruta)
                && contenido.windows(patron.len()).any(|v| v == patron)
            {
                return true;
            }
        }
    }
    false
}

#[tokio::test]
async fn avance_rapido_llega_al_remoto() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let (origin, rutas, punto) = preparar(raiz.path());
    let bare_contingencia = bare_contingencia_de(&rutas);

    let trabajo = raiz.path().join("trabajo-contingencia");
    let sha_nuevo = anadir_commit(&trabajo, &bare_contingencia, "main", "avance");

    let informe = reconciliar(
        &rutas,
        &id("jparga", "repo1"),
        &punto,
        &OrigenReconciliacion::LocalParaPruebas(origin.clone()),
        &Secreto::nuevo(TOKEN_DE_PRUEBA),
    )
    .await
    .expect("reconciliar no falla");

    assert!(informe.completa, "{informe:?}");
    assert_eq!(
        informe.ramas,
        vec![ResultadoRama::Enviada {
            rama: "main".to_string()
        }]
    );
    assert_eq!(refs_de(&origin)["refs/heads/main"], sha_nuevo);
}

#[tokio::test]
async fn divergencia_se_detecta_y_el_remoto_queda_intacto() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let (origin, rutas, punto) = preparar(raiz.path());
    let bare_contingencia = bare_contingencia_de(&rutas);

    // El remoto avanza por su cuenta...
    let trabajo_origen = raiz.path().join("trabajo-origen-2");
    let sha_remoto = anadir_commit(
        &trabajo_origen,
        &origin,
        "main",
        "cambio directo en el origen",
    );

    // ...y el repo de contingencia avanza por la suya, a partir del mismo punto: las
    // historias divergen.
    let trabajo_contingencia = raiz.path().join("trabajo-contingencia");
    anadir_commit(
        &trabajo_contingencia,
        &bare_contingencia,
        "main",
        "cambio en contingencia",
    );

    let refs_antes = refs_de(&origin);

    let informe = reconciliar(
        &rutas,
        &id("jparga", "repo1"),
        &punto,
        &OrigenReconciliacion::LocalParaPruebas(origin.clone()),
        &Secreto::nuevo(TOKEN_DE_PRUEBA),
    )
    .await
    .expect("reconciliar no falla");

    assert!(!informe.completa, "{informe:?}");
    assert_eq!(
        informe.ramas,
        vec![ResultadoRama::Divergente {
            rama: "main".to_string(),
            locales: 1,
            remotos: 1
        }]
    );
    let refs_despues = refs_de(&origin);
    assert_eq!(
        refs_antes, refs_despues,
        "el remoto no debe tocarse ante una divergencia"
    );
    assert_eq!(refs_despues["refs/heads/main"], sha_remoto);
}

#[tokio::test]
async fn una_rama_nueva_se_crea_en_el_remoto() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let (origin, rutas, punto) = preparar(raiz.path());
    let bare_contingencia = bare_contingencia_de(&rutas);

    let trabajo = raiz.path().join("trabajo-contingencia");
    let sha_rama_nueva = anadir_commit(
        &trabajo,
        &bare_contingencia,
        "una-rama-nueva",
        "trabajo en una rama nueva",
    );

    let informe = reconciliar(
        &rutas,
        &id("jparga", "repo1"),
        &punto,
        &OrigenReconciliacion::LocalParaPruebas(origin.clone()),
        &Secreto::nuevo(TOKEN_DE_PRUEBA),
    )
    .await
    .expect("reconciliar no falla");

    assert!(informe.completa, "{informe:?}");
    assert_eq!(
        informe.ramas,
        vec![ResultadoRama::Creada {
            rama: "una-rama-nueva".to_string()
        }]
    );
    assert_eq!(
        refs_de(&origin)["refs/heads/una-rama-nueva"],
        sha_rama_nueva
    );
}

#[tokio::test]
async fn una_rama_borrada_localmente_no_se_borra_en_el_remoto() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let (origin, rutas, mut punto) = preparar(raiz.path());
    let bare_contingencia = bare_contingencia_de(&rutas);

    // La rama «vieja» existe en ambos sitios en el punto de partida: se crea en el bare
    // de contingencia y se lleva también al origen real (con una URL explícita, no con
    // el remoto «origin» del clon, que apunta al bare de contingencia).
    let trabajo = raiz.path().join("trabajo-contingencia");
    let sha_vieja = anadir_commit(
        &trabajo,
        &bare_contingencia,
        "vieja",
        "rama que se borrará localmente",
    );
    ejecutar_git(
        &trabajo,
        &["push", "-q", origin.to_str().expect("utf8"), "vieja"],
    );
    punto
        .refs
        .insert("refs/heads/vieja".to_string(), sha_vieja.clone());

    // ...pero se borra en el repo de contingencia (sin tocar el origen).
    ejecutar_git(&bare_contingencia, &["branch", "-D", "vieja"]);

    let informe = reconciliar(
        &rutas,
        &id("jparga", "repo1"),
        &punto,
        &OrigenReconciliacion::LocalParaPruebas(origin.clone()),
        &Secreto::nuevo(TOKEN_DE_PRUEBA),
    )
    .await
    .expect("reconciliar no falla");

    assert!(informe.completa, "{informe:?}");
    assert!(
        informe.ramas.contains(&ResultadoRama::BorradaLocalmente {
            rama: "vieja".to_string()
        }),
        "{informe:?}"
    );
    assert_eq!(
        refs_de(&origin).get("refs/heads/vieja"),
        Some(&sha_vieja),
        "la rama borrada en local NUNCA se borra en el remoto"
    );
}

#[tokio::test]
async fn un_tag_en_conflicto_no_se_toca() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let (origin, rutas, punto) = preparar(raiz.path());
    let bare_contingencia = bare_contingencia_de(&rutas);

    // El origen tiene v1 apuntando a un commit propio...
    let trabajo_origen = raiz.path().join("trabajo-origen-2");
    ejecutar_git(
        raiz.path(),
        &[
            "clone",
            "-q",
            origin.to_str().expect("utf8"),
            trabajo_origen.to_str().expect("utf8"),
        ],
    );
    ejecutar_git(
        &trabajo_origen,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "para el tag del origen",
        ],
    );
    ejecutar_git(&trabajo_origen, &["tag", "v1"]);
    ejecutar_git(&trabajo_origen, &["push", "-q", "origin", "v1"]);
    let sha_tag_origen = salida_git(&trabajo_origen, &["rev-parse", "v1"]);

    // ...y el repo de contingencia tiene v1 apuntando a OTRO commit.
    let trabajo_contingencia = raiz.path().join("trabajo-contingencia");
    ejecutar_git(&bare_contingencia, &["rev-parse", "HEAD"]); // el bare existe.
    ejecutar_git(
        raiz.path(),
        &[
            "clone",
            "-q",
            bare_contingencia.to_str().expect("utf8"),
            trabajo_contingencia.to_str().expect("utf8"),
        ],
    );
    ejecutar_git(
        &trabajo_contingencia,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "avance para que haya algo que reconciliar",
        ],
    );
    ejecutar_git(&trabajo_contingencia, &["tag", "v1"]);
    ejecutar_git(
        &trabajo_contingencia,
        &["push", "-q", "origin", "main", "v1"],
    );

    let informe = reconciliar(
        &rutas,
        &id("jparga", "repo1"),
        &punto,
        &OrigenReconciliacion::LocalParaPruebas(origin.clone()),
        &Secreto::nuevo(TOKEN_DE_PRUEBA),
    )
    .await
    .expect("reconciliar no falla");

    assert!(
        informe.tags.contains(&ResultadoTag::ConflictoNoTocado {
            tag: "v1".to_string()
        }),
        "{informe:?}"
    );
    assert_eq!(
        refs_de(&origin)["refs/tags/v1"],
        sha_tag_origen,
        "el tag del remoto no debe tocarse"
    );
}

#[tokio::test]
async fn el_token_de_escritura_no_aparece_en_ningun_fichero_ni_en_el_informe() {
    let raiz = tempfile::tempdir().expect("directorio temporal");
    let (origin, rutas, punto) = preparar(raiz.path());
    let bare_contingencia = bare_contingencia_de(&rutas);

    let trabajo = raiz.path().join("trabajo-contingencia");
    anadir_commit(&trabajo, &bare_contingencia, "main", "avance");

    let token = Secreto::nuevo(TOKEN_DE_PRUEBA);
    let informe = reconciliar(
        &rutas,
        &id("jparga", "repo1"),
        &punto,
        &OrigenReconciliacion::LocalParaPruebas(origin.clone()),
        &token,
    )
    .await
    .expect("reconciliar no falla");

    let volcado = format!("{informe:?}");
    assert!(!volcado.contains(TOKEN_DE_PRUEBA), "{volcado}");
    assert!(
        !algun_fichero_contiene(raiz.path(), TOKEN_DE_PRUEBA.as_bytes()),
        "el token de escritura ha quedado en algún fichero bajo {}",
        raiz.path().display()
    );
}
