//! Provisión desatendida de una instancia de Gitea (pasos 4-6 del alta): migración de
//! la base de datos, administrador con contraseña generada y token de API.
//!
//! Idempotente: si `app.ini` ya existe se conserva tal cual (regenerar `SECRET_KEY`
//! invalidaría los datos que Gitea ya cifró con él); si el administrador ya existe no
//! se vuelve a crear; si el llavero ya tiene un token no se genera otro.

use std::path::Path;
use std::time::Duration;

use crate::config::RutasCuenta;
use crate::modelo::Nombre;
use crate::secretos::{ClaveSecreto, Llavero, Secreto};

use super::app_ini::{self, ParametrosAppIni};
use super::error::ErrorInstancia;
use super::fichero;
use super::proceso::ejecutar_gitea;

/// Usuario administrador que provisiona la instancia; también el usuario cuyo token
/// de API se usa para las operaciones administrativas de la ventana (contingencia).
pub const NOMBRE_ADMIN_GITEA: &str = "gitmereba-admin";
const CORREO_ADMIN: &str = "gitmereba-admin@localhost.local";
/// Scopes mínimos verificados por prueba y error contra Gitea 1.27.3:
/// bastan para crear organizaciones, migrar mirrors, listar (incluida
/// `/orgs/{org}/repos`, que usa `gitea::ClienteGitea::repos_de`), editar, borrar y
/// disparar `mirror-sync`.
const SCOPES_TOKEN: &str = "write:organization,write:repository";
/// Longitud de las contraseñas generadas por Gitea, reutilizada por `super::usuarios`
/// para los usuarios de la LAN.
pub(super) const LONGITUD_PASSWORD: usize = 32;
const TIEMPO_LIMITE_CORTO: Duration = Duration::from_secs(30);
const TIEMPO_LIMITE_MIGRATE: Duration = Duration::from_secs(120);

/// Parámetros de [`provisionar`].
pub struct ParametrosProvision<'a> {
    pub binario_gitea: &'a Path,
    pub rutas_cuenta: &'a RutasCuenta,
    pub puerto: u16,
    pub run_user: &'a str,
    pub login: &'a Nombre,
    pub secretos: &'a dyn Llavero,
    /// Ver [`ParametrosAppIni::modo_pruebas`]. Solo para pruebas de integración.
    pub modo_pruebas: bool,
}

/// Qué hizo falta hacer en esta llamada (para poder informar o depurar; ninguno de
/// estos campos afecta a que la operación sea idempotente).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultadoProvision {
    pub app_ini_generado: bool,
    pub admin_creado: bool,
    pub token_generado: bool,
}

/// Provisiona la instancia de Gitea descrita por `parametros`. Puede llamarse varias
/// veces sobre la misma carpeta: cada paso comprueba primero si ya está hecho.
pub async fn provisionar(
    parametros: &ParametrosProvision<'_>,
) -> Result<ResultadoProvision, ErrorInstancia> {
    let rutas = parametros.rutas_cuenta;
    fichero::crear_directorio_privado(rutas.carpeta())?;
    fichero::crear_directorio_privado(&rutas.gitea_datos())?;
    fichero::crear_directorio_privado(&rutas.gitea_repositorios())?;

    let app_ini_ruta = rutas.gitea_app_ini();
    let app_ini_generado = if app_ini_ruta.exists() {
        false
    } else {
        generar_y_escribir_app_ini(parametros).await?;
        true
    };

    let directorio_trabajo = rutas.gitea();
    let ini = mostrar(&app_ini_ruta);
    ejecutar_gitea(
        parametros.binario_gitea,
        &["migrate", "--config", &ini],
        &directorio_trabajo,
        TIEMPO_LIMITE_MIGRATE,
    )
    .await?;

    let admin_creado = if admin_existe(parametros.binario_gitea, &ini, &directorio_trabajo).await? {
        false
    } else {
        crear_admin(parametros, &ini, &directorio_trabajo).await?;
        true
    };

    let token_generado = if parametros
        .secretos
        .leer(parametros.login, ClaveSecreto::TokenGitea)?
        .is_some()
    {
        false
    } else {
        generar_y_guardar_token(parametros, &ini, &directorio_trabajo).await?;
        true
    };

    Ok(ResultadoProvision {
        app_ini_generado,
        admin_creado,
        token_generado,
    })
}

async fn generar_y_escribir_app_ini(
    parametros: &ParametrosProvision<'_>,
) -> Result<(), ErrorInstancia> {
    let directorio_trabajo = parametros.rutas_cuenta.gitea();
    let secret_key =
        generar_secreto(parametros.binario_gitea, "SECRET_KEY", &directorio_trabajo).await?;
    let internal_token = generar_secreto(
        parametros.binario_gitea,
        "INTERNAL_TOKEN",
        &directorio_trabajo,
    )
    .await?;
    let jwt_secret =
        generar_secreto(parametros.binario_gitea, "JWT_SECRET", &directorio_trabajo).await?;
    let lfs_jwt_secret = generar_secreto(
        parametros.binario_gitea,
        "LFS_JWT_SECRET",
        &directorio_trabajo,
    )
    .await?;

    let parametros_ini = ParametrosAppIni {
        rutas: parametros.rutas_cuenta,
        puerto: parametros.puerto,
        run_user: parametros.run_user,
        secret_key: &secret_key,
        internal_token: &internal_token,
        jwt_secret: &jwt_secret,
        lfs_jwt_secret: &lfs_jwt_secret,
        modo_pruebas: parametros.modo_pruebas,
        // La provisión inicial nunca conoce acceso LAN: se activa aparte, después,
        // con `cuentas::exponer_lan`, que regenera el `app.ini` conservando
        // los secretos con `regenerar_app_ini_conservando_secretos`.
        acceso_lan: None,
    };
    let texto = app_ini::generar_app_ini(&parametros_ini)?;
    app_ini::escribir_app_ini(parametros.rutas_cuenta, &texto)
}

async fn generar_secreto(
    binario: &Path,
    tipo: &str,
    directorio_trabajo: &Path,
) -> Result<Secreto, ErrorInstancia> {
    let salida = ejecutar_gitea(
        binario,
        &["generate", "secret", tipo],
        directorio_trabajo,
        TIEMPO_LIMITE_CORTO,
    )
    .await?;
    Ok(Secreto::nuevo(salida.stdout.trim().to_string()))
}

/// `gitea admin user list` y busca `gitmereba-admin` en la columna «Username».
async fn admin_existe(
    binario: &Path,
    ini: &str,
    directorio_trabajo: &Path,
) -> Result<bool, ErrorInstancia> {
    let salida = ejecutar_gitea(
        binario,
        &["admin", "user", "list", "--config", ini],
        directorio_trabajo,
        TIEMPO_LIMITE_CORTO,
    )
    .await?;
    Ok(salida
        .stdout
        .lines()
        .skip(1) // encabezado: «ID   Username   Email   ...»
        .any(|linea| linea.split_whitespace().nth(1) == Some(NOMBRE_ADMIN_GITEA)))
}

async fn crear_admin(
    parametros: &ParametrosProvision<'_>,
    ini: &str,
    directorio_trabajo: &Path,
) -> Result<(), ErrorInstancia> {
    // La contraseña la genera Gitea (`--random-password`) y se lee de su salida
    // estándar. No se pasa con `--password` porque los argumentos de un proceso son
    // legibles por CUALQUIER usuario del sistema en `/proc/<pid>/cmdline`.
    let longitud = LONGITUD_PASSWORD.to_string();
    let salida = ejecutar_gitea(
        parametros.binario_gitea,
        &[
            "admin",
            "user",
            "create",
            "--config",
            ini,
            "--username",
            NOMBRE_ADMIN_GITEA,
            "--email",
            CORREO_ADMIN,
            "--random-password",
            "--random-password-length",
            &longitud,
            "--admin",
            "--must-change-password=false",
        ],
        directorio_trabajo,
        TIEMPO_LIMITE_CORTO,
    )
    .await?;

    let password = extraer_password_generada(&salida.stdout)?;
    parametros.secretos.guardar(
        parametros.login,
        ClaveSecreto::PasswordAdminGitea,
        &password,
    )?;
    Ok(())
}

/// Extrae la contraseña de la línea `generated random password is '<valor>'`.
///
/// Exige la longitud pedida y un valor sin espacios ni comillas: ante cualquier otra
/// cosa falla sin incluir la salida en el error, que podría contener la contraseña.
///
/// Reutilizada por `super::usuarios::crear_usuario`: Gitea genera la contraseña
/// exactamente igual para un usuario normal que para el administrador.
pub(super) fn extraer_password_generada(stdout: &str) -> Result<Secreto, ErrorInstancia> {
    const MARCA: &str = "generated random password is '";
    let valor = stdout
        .lines()
        .find_map(|linea| linea.trim().strip_prefix(MARCA)?.strip_suffix('\''))
        .filter(|valor| {
            valor.chars().count() == LONGITUD_PASSWORD
                && !valor.contains(|c: char| c.is_whitespace() || c == '\'')
        })
        .ok_or_else(|| {
            ErrorInstancia::Io(
                "Gitea no devolvió la contraseña generada del administrador".to_string(),
            )
        })?;
    Ok(Secreto::nuevo(valor))
}

async fn generar_y_guardar_token(
    parametros: &ParametrosProvision<'_>,
    ini: &str,
    directorio_trabajo: &Path,
) -> Result<(), ErrorInstancia> {
    let nombre_token = nombre_token_unico();
    let salida = ejecutar_gitea(
        parametros.binario_gitea,
        &[
            "admin",
            "user",
            "generate-access-token",
            "--config",
            ini,
            "--username",
            NOMBRE_ADMIN_GITEA,
            "--token-name",
            &nombre_token,
            "--scopes",
            SCOPES_TOKEN,
            "--raw",
        ],
        directorio_trabajo,
        TIEMPO_LIMITE_CORTO,
    )
    .await?;
    let token = Secreto::nuevo(salida.stdout.trim().to_string());
    parametros
        .secretos
        .guardar(parametros.login, ClaveSecreto::TokenGitea, &token)?;
    Ok(())
}

/// Nombre único del token, para poder distinguir tokens de distintas provisiones si
/// alguna vez hiciera falta revocarlos desde la interfaz web de Gitea.
fn nombre_token_unico() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("gitmereba-{segundos}")
}

fn mostrar(ruta: &Path) -> String {
    ruta.display().to_string()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use crate::secretos::LlaveroEnMemoria;

    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    // --- extraer_password_generada -------------------------------------------

    #[test]
    fn extrae_la_password_de_la_salida_de_gitea() {
        let valor = "a".repeat(LONGITUD_PASSWORD);
        let stdout = format!(
            "generated random password is '{valor}'\nNew user 'x' has been successfully created!\n"
        );
        let password = extraer_password_generada(&stdout).expect("extrae");
        assert_eq!(password.exponer(), valor);
    }

    #[test]
    fn rechaza_salidas_sin_password_o_con_longitud_inesperada() {
        for stdout in [
            "",
            "New user 'x' has been successfully created!",
            "generated random password is 'corta'",
            "generated random password is ''",
        ] {
            let error = extraer_password_generada(stdout).expect_err("debe fallar");
            assert!(!error.to_string().contains("corta"));
        }
    }

    // --- provisionar (con un `gitea` falso: sin red, sin Gitea real) ---------

    /// Un script que imita lo mínimo de `gitea` que usa `provisionar`, guardando en
    /// `$GITEA_WORK_DIR/.admin-creado` si ya se «creó» el administrador, para poder
    /// probar la idempotencia sin un Gitea real.
    fn crear_gitea_falso(directorio: &Path) -> std::path::PathBuf {
        let script = directorio.join("gitea-falso.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -e
case "$1" in
  generate)
    echo "secreto-falso-$3"
    ;;
  migrate)
    ;;
  admin)
    case "$3" in
      list)
        echo "ID   Username        Email"
        if [ -f "$GITEA_WORK_DIR/.admin-creado" ]; then
          echo "1    gitmereba-admin gitmereba-admin@localhost.local"
        fi
        ;;
      create)
        touch "$GITEA_WORK_DIR/.admin-creado"
        for a in "$@"; do [ "$a" = "--password" ] && exit 9; done
        echo "generated random password is 'abcdefghijklmnopqrstuvwxyzABCDEF'"
        echo "New user 'gitmereba-admin' has been successfully created!"
        ;;
      generate-access-token)
        echo "token-falso-de-prueba"
        ;;
    esac
    ;;
esac
"#,
        )
        .expect("escribir el script de gitea falso");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el script");
        script
    }

    /// Variante que aborta si se le pide `generate secret`: para comprobar que
    /// `provisionar` no vuelve a generar secretos cuando `app.ini` ya existe.
    fn crear_gitea_falso_sin_generate(directorio: &Path) -> std::path::PathBuf {
        let script = directorio.join("gitea-sin-generate.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -e
case "$1" in
  generate)
    echo "no debería llamarse a «generate»" >&2
    exit 9
    ;;
  migrate)
    ;;
  admin)
    case "$3" in
      list)
        echo "ID   Username        Email"
        echo "1    gitmereba-admin gitmereba-admin@localhost.local"
        ;;
      generate-access-token)
        echo "token-falso-de-prueba"
        ;;
    esac
    ;;
esac
"#,
        )
        .expect("escribir el script de gitea falso");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el script");
        script
    }

    #[tokio::test]
    async fn primera_provision_crea_app_ini_admin_y_token() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());
        let rutas_cuenta = RutasCuenta::nueva(directorio.path().join("cuenta"));
        let llavero = LlaveroEnMemoria::nuevo();
        let login = nombre("jparga");

        let parametros = ParametrosProvision {
            binario_gitea: &binario,
            rutas_cuenta: &rutas_cuenta,
            puerto: 33042,
            run_user: "jparga",
            login: &login,
            secretos: &llavero,
            modo_pruebas: true,
        };

        let resultado = provisionar(&parametros)
            .await
            .expect("provisionar no falla");

        assert!(resultado.app_ini_generado);
        assert!(resultado.admin_creado);
        assert!(resultado.token_generado);
        assert!(rutas_cuenta.gitea_app_ini().exists());
        assert!(
            llavero
                .leer(&login, ClaveSecreto::PasswordAdminGitea)
                .expect("leer no falla")
                .is_some()
        );
        assert_eq!(
            llavero
                .leer(&login, ClaveSecreto::TokenGitea)
                .expect("leer no falla")
                .map(|s| s.exponer().to_string()),
            Some("token-falso-de-prueba".to_string())
        );
    }

    #[tokio::test]
    async fn provisionar_dos_veces_es_idempotente() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());
        let rutas_cuenta = RutasCuenta::nueva(directorio.path().join("cuenta"));
        let llavero = LlaveroEnMemoria::nuevo();
        let login = nombre("jparga");

        let parametros = ParametrosProvision {
            binario_gitea: &binario,
            rutas_cuenta: &rutas_cuenta,
            puerto: 33042,
            run_user: "jparga",
            login: &login,
            secretos: &llavero,
            modo_pruebas: true,
        };

        let primero = provisionar(&parametros).await.expect("primera provisión");
        let contenido_tras_primero =
            std::fs::read_to_string(rutas_cuenta.gitea_app_ini()).expect("leer app.ini");
        let token_tras_primero = llavero
            .leer(&login, ClaveSecreto::TokenGitea)
            .expect("leer no falla");

        let segundo = provisionar(&parametros).await.expect("segunda provisión");
        let contenido_tras_segundo =
            std::fs::read_to_string(rutas_cuenta.gitea_app_ini()).expect("leer app.ini");

        assert!(primero.app_ini_generado);
        assert!(!segundo.app_ini_generado);
        assert!(primero.admin_creado);
        assert!(!segundo.admin_creado);
        assert!(primero.token_generado);
        assert!(!segundo.token_generado);
        // app.ini no se ha tocado: regenerar SECRET_KEY invalidaría los datos cifrados.
        assert_eq!(contenido_tras_primero, contenido_tras_segundo);
        assert_eq!(
            token_tras_primero.map(|s| s.exponer().to_string()),
            llavero
                .leer(&login, ClaveSecreto::TokenGitea)
                .expect("leer no falla")
                .map(|s| s.exponer().to_string())
        );
    }

    #[tokio::test]
    async fn no_regenera_secretos_si_app_ini_ya_existe() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso_sin_generate(directorio.path());
        let rutas_cuenta = RutasCuenta::nueva(directorio.path().join("cuenta"));
        std::fs::create_dir_all(rutas_cuenta.gitea_app_ini().parent().expect("padre"))
            .expect("crear el directorio de conf");
        std::fs::write(rutas_cuenta.gitea_app_ini(), "app.ini ya existente")
            .expect("preparar un app.ini existente");
        let llavero = LlaveroEnMemoria::nuevo();
        let login = nombre("jparga");

        let parametros = ParametrosProvision {
            binario_gitea: &binario,
            rutas_cuenta: &rutas_cuenta,
            puerto: 33042,
            run_user: "jparga",
            login: &login,
            secretos: &llavero,
            modo_pruebas: true,
        };

        let resultado = provisionar(&parametros)
            .await
            .expect("provisionar no falla aunque no llame a «generate»");

        assert!(!resultado.app_ini_generado);
        assert_eq!(
            std::fs::read_to_string(rutas_cuenta.gitea_app_ini()).expect("leer app.ini"),
            "app.ini ya existente"
        );
    }
}
