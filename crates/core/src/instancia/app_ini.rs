//! Generación del `app.ini` de Gitea, endurecido para producción.

use std::path::Path;

use crate::config::RutasCuenta;
use crate::modelo::NombreHostInterno;
use crate::secretos::Secreto;

use super::error::ErrorInstancia;
use super::fichero;

/// Acceso LAN a incluir en el `app.ini` generado: además del nombre `.internal`,
/// las rutas del certificado y la clave generados por [`super::certificado`].
pub struct ParametrosLanAppIni<'a> {
    pub host: &'a NombreHostInterno,
    pub ruta_certificado: &'a Path,
    pub ruta_clave: &'a Path,
}

/// Parámetros para [`generar_app_ini`]. Todo lo que no es secreto ni ruta derivada de
/// la cuenta se fija dentro de la función: no hay nada configurable por el usuario
/// que pueda debilitar el endurecimiento.
pub struct ParametrosAppIni<'a> {
    pub rutas: &'a RutasCuenta,
    pub puerto: u16,
    /// Usuario del sistema que ejecuta el proceso (`RUN_USER`).
    pub run_user: &'a str,
    pub secret_key: &'a Secreto,
    pub internal_token: &'a Secreto,
    pub jwt_secret: &'a Secreto,
    pub lfs_jwt_secret: &'a Secreto,
    /// Solo para pruebas de integración con un origen local:
    /// pone a `true` `[migrations] ALLOW_LOCALNETWORKS` e `[security]
    /// IMPORT_LOCAL_PATHS`. **Nunca debe activarse en producción**: abre una vía de
    /// SSRF hacia redes locales y permite migrar repos desde el disco del servidor.
    pub modo_pruebas: bool,
    /// `None` (por defecto): Gitea solo escucha en `127.0.0.1` por HTTP, exactamente
    /// como sin acceso LAN. `Some`: acceso LAN por HTTPS.
    pub acceso_lan: Option<ParametrosLanAppIni<'a>>,
}

/// Genera el contenido de `app.ini` a partir de `parametros`. Función pura: no toca
/// el disco (para eso está [`escribir_app_ini`]).
///
/// Rechaza cualquier valor con un salto de línea, que en un fichero INI permitiría
/// inyectar claves o secciones no previstas.
pub fn generar_app_ini(parametros: &ParametrosAppIni<'_>) -> Result<String, ErrorInstancia> {
    let rutas = parametros.rutas;
    let work_path = rutas.gitea().display().to_string();
    let db_path = rutas.gitea_datos().join("gitea.db").display().to_string();
    let repo_root = rutas.gitea_repositorios().display().to_string();
    let log_path = rutas.gitea().join("log").display().to_string();

    for valor in [
        parametros.run_user,
        work_path.as_str(),
        db_path.as_str(),
        repo_root.as_str(),
        log_path.as_str(),
        parametros.secret_key.exponer(),
        parametros.internal_token.exponer(),
        parametros.jwt_secret.exponer(),
        parametros.lfs_jwt_secret.exponer(),
    ] {
        validar_sin_salto_de_linea(valor)?;
    }

    let (allow_localnetworks, import_local_paths) = if parametros.modo_pruebas {
        ("true", "true")
    } else {
        ("false", "false")
    };
    let advertencia_pruebas = if parametros.modo_pruebas {
        "; ADVERTENCIA: modo_pruebas=true. Fuera de pruebas de integración con un\n\
         ; origen local esto abre una vía de SSRF y de lectura del disco del\n\
         ; servidor: nunca debe llegar así a producción.\n"
    } else {
        ""
    };

    let bloque_servidor = match &parametros.acceso_lan {
        None => format!(
            "PROTOCOL = http\n\
             DOMAIN = 127.0.0.1\n\
             HTTP_ADDR = 127.0.0.1\n\
             HTTP_PORT = {puerto}\n\
             ROOT_URL = http://127.0.0.1:{puerto}/\n",
            puerto = parametros.puerto,
        ),
        Some(lan) => {
            let ruta_certificado = mostrar_ruta(lan.ruta_certificado);
            let ruta_clave = mostrar_ruta(lan.ruta_clave);
            for valor in [
                lan.host.as_str(),
                ruta_certificado.as_str(),
                ruta_clave.as_str(),
            ] {
                validar_sin_salto_de_linea(valor)?;
            }
            format!(
                "PROTOCOL = https\n\
                 DOMAIN = {host}\n\
                 HTTP_ADDR = 0.0.0.0\n\
                 HTTP_PORT = {puerto}\n\
                 ROOT_URL = https://{host}:{puerto}/\n\
                 CERT_FILE = {ruta_certificado}\n\
                 KEY_FILE = {ruta_clave}\n\
                 SSL_MIN_VERSION = TLSv1.2\n",
                host = lan.host,
                puerto = parametros.puerto,
                ruta_certificado = ruta_certificado,
                ruta_clave = ruta_clave,
            )
        }
    };

    Ok(format!(
        "APP_NAME = gitmereba\n\
         RUN_MODE = prod\n\
         RUN_USER = {run_user}\n\
         WORK_PATH = {work_path}\n\
         \n\
         [server]\n\
         {bloque_servidor}DISABLE_SSH = true\n\
         LFS_START_SERVER = true\n\
         OFFLINE_MODE = true\n\
         LFS_JWT_SECRET = {lfs_jwt_secret}\n\
         \n\
         [database]\n\
         DB_TYPE = sqlite3\n\
         PATH = {db_path}\n\
         \n\
         [repository]\n\
         ROOT = {repo_root}\n\
         DEFAULT_PRIVATE = private\n\
         DISABLE_MIGRATIONS = false\n\
         \n\
         [security]\n\
         INSTALL_LOCK = true\n\
         SECRET_KEY = {secret_key}\n\
         INTERNAL_TOKEN = {internal_token}\n\
         MIN_PASSWORD_LENGTH = 12\n\
         {advertencia_pruebas}IMPORT_LOCAL_PATHS = {import_local_paths}\n\
         \n\
         [oauth2]\n\
         JWT_SECRET = {jwt_secret}\n\
         \n\
         [service]\n\
         DISABLE_REGISTRATION = true\n\
         REQUIRE_SIGNIN_VIEW = true\n\
         ALLOW_ONLY_INTERNAL_REGISTRATION = false\n\
         ALLOW_ONLY_EXTERNAL_REGISTRATION = false\n\
         ENABLE_NOTIFY_MAIL = false\n\
         REGISTER_EMAIL_CONFIRM = false\n\
         ENABLE_CAPTCHA = false\n\
         DEFAULT_KEEP_EMAIL_PRIVATE = true\n\
         NO_REPLY_ADDRESS = noreply.localhost\n\
         \n\
         [openid]\n\
         ENABLE_OPENID_SIGNIN = false\n\
         ENABLE_OPENID_SIGNUP = false\n\
         \n\
         [log]\n\
         MODE = file\n\
         LEVEL = Info\n\
         ROOT_PATH = {log_path}\n\
         \n\
         [cron.update_checker]\n\
         ENABLED = false\n\
         \n\
         [webhook]\n\
         ALLOWED_HOST_LIST = loopback\n\
         \n\
         [mirror]\n\
         ENABLED = true\n\
         DEFAULT_INTERVAL = 10m\n\
         MIN_INTERVAL = 10m\n\
         \n\
         [migrations]\n\
         {advertencia_pruebas}ALLOW_LOCALNETWORKS = {allow_localnetworks}\n\
         \n\
         [actions]\n\
         ENABLED = false\n\
         \n\
         [packages]\n\
         ENABLED = false\n",
        run_user = parametros.run_user,
        work_path = work_path,
        bloque_servidor = bloque_servidor,
        lfs_jwt_secret = parametros.lfs_jwt_secret.exponer(),
        db_path = db_path,
        repo_root = repo_root,
        secret_key = parametros.secret_key.exponer(),
        internal_token = parametros.internal_token.exponer(),
        advertencia_pruebas = advertencia_pruebas,
        import_local_paths = import_local_paths,
        jwt_secret = parametros.jwt_secret.exponer(),
        log_path = log_path,
        allow_localnetworks = allow_localnetworks,
    ))
}

fn mostrar_ruta(ruta: &Path) -> String {
    ruta.display().to_string()
}

/// Los cuatro secretos leídos de un `app.ini` ya existente, para poder regenerarlo
/// (p. ej. al activar o desactivar el acceso LAN) sin invalidar los datos que Gitea ya
/// cifró con `SECRET_KEY`. Ninguna función de este módulo registra su contenido.
#[derive(Debug)]
pub struct SecretosAppIni {
    pub secret_key: Secreto,
    pub internal_token: Secreto,
    pub jwt_secret: Secreto,
    pub lfs_jwt_secret: Secreto,
}

/// Extrae los cuatro secretos de un `app.ini` ya generado por [`generar_app_ini`]. El
/// error solo nombra la clave que falta, nunca el contenido del fichero.
pub fn leer_secretos_app_ini(texto: &str) -> Result<SecretosAppIni, ErrorInstancia> {
    Ok(SecretosAppIni {
        secret_key: Secreto::nuevo(extraer_valor(texto, "SECRET_KEY")?),
        internal_token: Secreto::nuevo(extraer_valor(texto, "INTERNAL_TOKEN")?),
        jwt_secret: Secreto::nuevo(extraer_valor(texto, "JWT_SECRET")?),
        lfs_jwt_secret: Secreto::nuevo(extraer_valor(texto, "LFS_JWT_SECRET")?),
    })
}

fn extraer_valor(texto: &str, clave: &str) -> Result<String, ErrorInstancia> {
    let patron = format!("\n{clave} = ");
    let inicio = texto
        .find(&patron)
        .ok_or_else(|| ErrorInstancia::Io(format!("app.ini no contiene la clave «{clave}»")))?
        + patron.len();
    let resto = &texto[inicio..];
    let fin = resto.find('\n').unwrap_or(resto.len());
    Ok(resto[..fin].to_string())
}

/// Regenera el `app.ini` de una cuenta ya provisionada, conservando sus cuatro
/// secretos (leídos del `app.ini` actual, nunca regenerados) y cambiando solo lo que
/// dependa de `acceso_lan` y del resto de `parametros`. Pensado para activar o
/// desactivar el acceso LAN sin invalidar los datos ya cifrados por Gitea.
pub fn regenerar_app_ini_conservando_secretos(
    rutas: &RutasCuenta,
    puerto: u16,
    run_user: &str,
    acceso_lan: Option<ParametrosLanAppIni<'_>>,
) -> Result<(), ErrorInstancia> {
    let actual = std::fs::read_to_string(rutas.gitea_app_ini())
        .map_err(|error| ErrorInstancia::Io(error.to_string()))?;
    let secretos = leer_secretos_app_ini(&actual)?;
    let parametros = ParametrosAppIni {
        rutas,
        puerto,
        run_user,
        secret_key: &secretos.secret_key,
        internal_token: &secretos.internal_token,
        jwt_secret: &secretos.jwt_secret,
        lfs_jwt_secret: &secretos.lfs_jwt_secret,
        modo_pruebas: false,
        acceso_lan,
    };
    let texto = generar_app_ini(&parametros)?;
    escribir_app_ini(rutas, &texto)
}

/// Escribe `texto` en `rutas.gitea_app_ini()` de forma atómica, con permisos 0600.
pub fn escribir_app_ini(rutas: &RutasCuenta, texto: &str) -> Result<(), ErrorInstancia> {
    fichero::escribir_privado(&rutas.gitea_app_ini(), texto.as_bytes())
}

/// Un valor de `app.ini` con un salto de línea podría inyectar una clave o sección
/// nueva en el INI (p. ej. reabrir `[security]` con una clave distinta).
fn validar_sin_salto_de_linea(valor: &str) -> Result<(), ErrorInstancia> {
    if valor.contains('\n') || valor.contains('\r') {
        Err(ErrorInstancia::EntradaInvalida(
            "un valor de app.ini contiene un salto de línea".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn parametros(rutas: &RutasCuenta, modo_pruebas: bool) -> ParametrosAppIni<'_> {
        ParametrosAppIni {
            rutas,
            puerto: 33042,
            run_user: "jparga",
            secret_key: SECRETOS.secret_key(),
            internal_token: SECRETOS.internal_token(),
            jwt_secret: SECRETOS.jwt_secret(),
            lfs_jwt_secret: SECRETOS.lfs_jwt_secret(),
            modo_pruebas,
            acceso_lan: None,
        }
    }

    /// Los cuatro secretos de prueba viven aquí para no reconstruirlos en cada test.
    struct Secretos {
        secret_key: Secreto,
        internal_token: Secreto,
        jwt_secret: Secreto,
        lfs_jwt_secret: Secreto,
    }

    impl Secretos {
        fn secret_key(&self) -> &Secreto {
            &self.secret_key
        }
        fn internal_token(&self) -> &Secreto {
            &self.internal_token
        }
        fn jwt_secret(&self) -> &Secreto {
            &self.jwt_secret
        }
        fn lfs_jwt_secret(&self) -> &Secreto {
            &self.lfs_jwt_secret
        }
    }

    static SECRETOS: std::sync::LazyLock<Secretos> = std::sync::LazyLock::new(|| Secretos {
        secret_key: Secreto::nuevo("secreto-clave-de-prueba"),
        internal_token: Secreto::nuevo("secreto-interno-de-prueba"),
        jwt_secret: Secreto::nuevo("secreto-jwt-de-prueba"),
        lfs_jwt_secret: Secreto::nuevo("secreto-lfs-de-prueba"),
    });

    #[test]
    fn produccion_deja_las_claves_criticas_de_seguridad_endurecidas() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let ini = generar_app_ini(&parametros(&rutas, false)).expect("genera el ini");

        for esperado in [
            "ALLOW_LOCALNETWORKS = false",
            "IMPORT_LOCAL_PATHS = false",
            "HTTP_ADDR = 127.0.0.1",
            "DISABLE_SSH = true",
            "OFFLINE_MODE = true",
            "DISABLE_REGISTRATION = true",
            "REQUIRE_SIGNIN_VIEW = true",
            "ENABLE_OPENID_SIGNIN = false",
            "ENABLE_OPENID_SIGNUP = false",
            "ALLOWED_HOST_LIST = loopback",
            "DEFAULT_INTERVAL = 10m",
            "MIN_INTERVAL = 10m",
            "LFS_START_SERVER = true",
            "INSTALL_LOCK = true",
            "DISABLE_MIGRATIONS = false",
            "MIN_PASSWORD_LENGTH = 12",
        ] {
            assert!(ini.contains(esperado), "falta «{esperado}» en:\n{ini}");
        }
        assert!(seccion(&ini, "actions").contains("ENABLED = false"));
        assert!(seccion(&ini, "packages").contains("ENABLED = false"));
        assert!(seccion(&ini, "cron.update_checker").contains("ENABLED = false"));
        assert!(!ini.contains("ADVERTENCIA"));
    }

    #[test]
    fn los_secretos_van_en_las_secciones_que_gitea_lee_no_en_las_habituales() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let ini = generar_app_ini(&parametros(&rutas, false)).expect("genera el ini");

        // LFS_JWT_SECRET va en [server], no en [lfs].
        assert!(seccion(&ini, "server").contains("LFS_JWT_SECRET = secreto-lfs-de-prueba"));
        // IMPORT_LOCAL_PATHS va en [security], no en [repository].
        assert!(seccion(&ini, "security").contains("IMPORT_LOCAL_PATHS"));
        assert!(!seccion(&ini, "repository").contains("IMPORT_LOCAL_PATHS"));
        assert!(seccion(&ini, "security").contains("SECRET_KEY = secreto-clave-de-prueba"));
        assert!(seccion(&ini, "security").contains("INTERNAL_TOKEN = secreto-interno-de-prueba"));
        assert!(seccion(&ini, "oauth2").contains("JWT_SECRET = secreto-jwt-de-prueba"));
    }

    #[test]
    fn modo_pruebas_activa_solo_los_dos_ajustes_de_origen_local_y_avisa() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let ini = generar_app_ini(&parametros(&rutas, true)).expect("genera el ini");

        assert!(ini.contains("ALLOW_LOCALNETWORKS = true"));
        assert!(ini.contains("IMPORT_LOCAL_PATHS = true"));
        assert!(ini.contains("ADVERTENCIA"));
        // El resto del endurecimiento no cambia con modo_pruebas.
        assert!(ini.contains("REQUIRE_SIGNIN_VIEW = true"));
        assert!(ini.contains("DISABLE_SSH = true"));
    }

    #[test]
    fn rechaza_un_run_user_con_salto_de_linea() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let mut p = parametros(&rutas, false);
        p.run_user = "jparga\n[security]\nINSTALL_LOCK = false";
        assert!(matches!(
            generar_app_ini(&p),
            Err(ErrorInstancia::EntradaInvalida(_))
        ));
    }

    #[test]
    fn rechaza_un_secreto_con_salto_de_linea() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let secreto_malicioso = Secreto::nuevo("x\nDISABLE_SSH = false");
        let mut p = parametros(&rutas, false);
        p.secret_key = &secreto_malicioso;
        assert!(matches!(
            generar_app_ini(&p),
            Err(ErrorInstancia::EntradaInvalida(_))
        ));
    }

    #[test]
    fn escribir_app_ini_deja_permisos_0600() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(directorio.path().join("cuenta"));
        let p = parametros(&rutas, false);
        let ini = generar_app_ini(&p).expect("genera el ini");

        escribir_app_ini(&rutas, &ini).expect("escribir_app_ini no falla");

        let permisos = std::fs::metadata(rutas.gitea_app_ini())
            .expect("metadata")
            .permissions();
        assert_eq!(permisos.mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(rutas.gitea_app_ini()).unwrap(), ini);
    }

    /// Extrae el cuerpo de `[nombre]` hasta la siguiente sección, para comprobar en
    /// qué sección cae cada clave sin depender de un parser de INI completo.
    fn seccion<'a>(ini: &'a str, nombre: &str) -> &'a str {
        let encabezado = format!("[{nombre}]");
        let inicio = ini.find(&encabezado).expect("la sección existe") + encabezado.len();
        let resto = &ini[inicio..];
        let fin = resto.find("\n[").unwrap_or(resto.len());
        &resto[..fin]
    }

    // --- acceso LAN ---------------------------------------------------

    fn host_de_prueba() -> crate::modelo::NombreHostInterno {
        crate::modelo::NombreHostInterno::nuevo("jparga.gitmereba.internal")
            .expect("host de prueba válido")
    }

    #[test]
    fn sin_acceso_lan_la_salida_no_cambia_salvo_min_password_length() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let ini = generar_app_ini(&parametros(&rutas, false)).expect("genera el ini");

        assert!(seccion(&ini, "server").contains("PROTOCOL = http\n"));
        assert!(seccion(&ini, "server").contains("HTTP_ADDR = 127.0.0.1\n"));
        assert!(seccion(&ini, "server").contains("DOMAIN = 127.0.0.1\n"));
        assert!(seccion(&ini, "server").contains("ROOT_URL = http://127.0.0.1:33042/\n"));
        assert!(!ini.contains("CERT_FILE"));
        assert!(!ini.contains("KEY_FILE"));
        assert!(!ini.contains("SSL_MIN_VERSION"));
    }

    #[test]
    fn con_acceso_lan_el_servidor_escucha_en_todas_las_interfaces_por_https() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let mut p = parametros(&rutas, false);
        let host = host_de_prueba();
        let lan = ParametrosLanAppIni {
            host: &host,
            ruta_certificado: Path::new("/cuentas/jparga/gitea/tls/cert.pem"),
            ruta_clave: Path::new("/cuentas/jparga/gitea/tls/key.pem"),
        };
        p.acceso_lan = Some(lan);
        let ini = generar_app_ini(&p).expect("genera el ini");

        let servidor = seccion(&ini, "server");
        assert!(servidor.contains("PROTOCOL = https\n"), "{servidor}");
        assert!(servidor.contains("HTTP_ADDR = 0.0.0.0\n"), "{servidor}");
        assert!(
            servidor.contains("DOMAIN = jparga.gitmereba.internal\n"),
            "{servidor}"
        );
        assert!(
            servidor.contains("ROOT_URL = https://jparga.gitmereba.internal:33042/\n"),
            "{servidor}"
        );
        assert!(
            servidor.contains("CERT_FILE = /cuentas/jparga/gitea/tls/cert.pem\n"),
            "{servidor}"
        );
        assert!(
            servidor.contains("KEY_FILE = /cuentas/jparga/gitea/tls/key.pem\n"),
            "{servidor}"
        );
        assert!(
            servidor.contains("SSL_MIN_VERSION = TLSv1.2\n"),
            "{servidor}"
        );
        // Nada más se relaja.
        assert!(ini.contains("DISABLE_SSH = true"));
        assert!(ini.contains("DISABLE_REGISTRATION = true"));
        assert!(ini.contains("MIN_PASSWORD_LENGTH = 12"));
    }

    #[test]
    fn rechaza_un_host_lan_con_salto_de_linea() {
        // No puede pasar de verdad por `NombreHostInterno::nuevo` (lo rechazaría antes),
        // pero `generar_app_ini` no debe fiarse solo de eso: defensa en profundidad.
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let mut p = parametros(&rutas, false);
        let ruta = Path::new("/x/cert.pem\n[security]\nINSTALL_LOCK = false");
        let host = host_de_prueba();
        p.acceso_lan = Some(ParametrosLanAppIni {
            host: &host,
            ruta_certificado: ruta,
            ruta_clave: Path::new("/x/key.pem"),
        });
        assert!(matches!(
            generar_app_ini(&p),
            Err(ErrorInstancia::EntradaInvalida(_))
        ));
    }

    #[test]
    fn leer_secretos_app_ini_extrae_los_cuatro_secretos() {
        let rutas = RutasCuenta::nueva("/cuentas/jparga");
        let ini = generar_app_ini(&parametros(&rutas, false)).expect("genera el ini");

        let secretos = leer_secretos_app_ini(&ini).expect("lee los secretos");
        assert_eq!(secretos.secret_key.exponer(), "secreto-clave-de-prueba");
        assert_eq!(
            secretos.internal_token.exponer(),
            "secreto-interno-de-prueba"
        );
        assert_eq!(secretos.jwt_secret.exponer(), "secreto-jwt-de-prueba");
        assert_eq!(secretos.lfs_jwt_secret.exponer(), "secreto-lfs-de-prueba");
    }

    #[test]
    fn leer_secretos_app_ini_falla_sin_incluir_el_contenido_en_el_error() {
        let error = leer_secretos_app_ini("APP_NAME = gitmereba\n").unwrap_err();
        assert!(error.to_string().contains("SECRET_KEY"));
    }

    #[test]
    fn regenerar_conserva_los_secretos_y_activa_el_acceso_lan() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(directorio.path().join("cuenta"));
        let ini_inicial =
            generar_app_ini(&parametros(&rutas, false)).expect("genera el ini inicial");
        escribir_app_ini(&rutas, &ini_inicial).expect("escribe el ini inicial");

        let host = host_de_prueba();
        let lan = ParametrosLanAppIni {
            host: &host,
            ruta_certificado: Path::new("/cuentas/jparga/gitea/tls/cert.pem"),
            ruta_clave: Path::new("/cuentas/jparga/gitea/tls/key.pem"),
        };
        regenerar_app_ini_conservando_secretos(&rutas, 33042, "jparga", Some(lan))
            .expect("regenerar no falla");

        let regenerado =
            std::fs::read_to_string(rutas.gitea_app_ini()).expect("leer el ini regenerado");
        assert!(regenerado.contains("PROTOCOL = https"));
        assert!(regenerado.contains("SECRET_KEY = secreto-clave-de-prueba"));
        assert!(regenerado.contains("INTERNAL_TOKEN = secreto-interno-de-prueba"));
        assert!(regenerado.contains("JWT_SECRET = secreto-jwt-de-prueba"));
        assert!(regenerado.contains("LFS_JWT_SECRET = secreto-lfs-de-prueba"));

        // Y se puede volver a desactivar sin perder los secretos.
        regenerar_app_ini_conservando_secretos(&rutas, 33042, "jparga", None)
            .expect("regenerar de vuelta a local no falla");
        let vuelto = std::fs::read_to_string(rutas.gitea_app_ini()).expect("leer");
        assert!(vuelto.contains("PROTOCOL = http\n"));
        assert!(vuelto.contains("SECRET_KEY = secreto-clave-de-prueba"));
    }
}
