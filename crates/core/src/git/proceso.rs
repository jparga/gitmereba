//! Invocación de `git` como proceso hijo: sin shell, entorno mínimo, con tiempo límite.

use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::secretos::Secreto;

/// Tiempo límite por defecto de un comando `git`.
const TIEMPO_LIMITE_POR_DEFECTO: Duration = Duration::from_secs(600);

/// Longitud máxima del `stderr` que se conserva en [`ErrorGit::Fallo`].
const LONGITUD_MAXIMA_STDERR: usize = 2000;

/// Error al invocar `git`. Ningún mensaje incluye secretos ni credenciales de URL.
#[derive(Debug, thiserror::Error)]
pub enum ErrorGit {
    #[error("git no está instalado o no se encuentra en el PATH")]
    NoEncontrado,
    #[error("el comando de git superó el tiempo límite")]
    Timeout,
    #[error("git terminó con código {codigo}: {stderr}")]
    Fallo { codigo: i32, stderr: String },
    #[error("entrada inválida: {0}")]
    EntradaInvalida(String),
    #[error("error del sistema al invocar git: {0}")]
    Sistema(String),
}

/// Credenciales para una operación puntual, enviadas como cabecera HTTP.
///
/// Nunca se incrustan en la URL ni en los argumentos: viajan al proceso hijo como
/// variables `GIT_CONFIG_*` que fijan `http.<origen>.extraHeader`, de modo que git solo
/// envía la cabecera a ese origen y no a otros hosts ni tras una redirección.
#[derive(Debug, Clone)]
pub struct Credencial {
    /// Origen al que se limita la cabecera, p. ej. `https://github.com/`.
    pub origen: String,
    pub usuario: String,
    pub token: Secreto,
}

/// Certificado CA de confianza para una operación puntual, acotado a un origen
/// (hablar por HTTPS con el Gitea local expuesto a la LAN, cuyo certificado es
/// autofirmado). Viaja al proceso hijo como `http.<origen>.sslCAInfo`, con las mismas
/// variables `GIT_CONFIG_*` que [`Credencial`] y el mismo origen validado por
/// [`validar_origen`]. Nunca se usa `GIT_SSL_CAINFO` (afectaría a cualquier origen,
/// incluido github.com) ni `http.sslVerify = false`.
#[derive(Debug, Clone)]
pub struct CertificadoCa {
    /// Origen al que se limita la confianza en este certificado, p. ej.
    /// `https://127.0.0.1:3000/`.
    pub origen: String,
    pub ruta: PathBuf,
}

/// Opciones para [`ejecutar`].
#[derive(Debug, Clone)]
pub struct Opciones {
    /// Directorio de trabajo del proceso hijo; `None` usa el directorio actual.
    pub directorio: Option<PathBuf>,
    /// Tiempo máximo antes de matar el proceso y devolver [`ErrorGit::Timeout`].
    pub tiempo_limite: Duration,
    /// Credenciales a enviar como cabecera HTTP, si la operación las necesita.
    pub credencial: Option<Credencial>,
    /// Certificado CA de confianza, acotado a un origen, si la operación habla
    /// con un Gitea local expuesto a la LAN por HTTPS.
    pub certificado_ca: Option<CertificadoCa>,
    /// Transportes permitidos (`GIT_ALLOW_PROTOCOL`). Por defecto excluye `ext` y `ssh`.
    pub protocolos: String,
}

/// Transportes permitidos por defecto.
const PROTOCOLOS_POR_DEFECTO: &str = "http:https:file";

impl Default for Opciones {
    fn default() -> Self {
        Self {
            directorio: None,
            tiempo_limite: TIEMPO_LIMITE_POR_DEFECTO,
            credencial: None,
            certificado_ca: None,
            protocolos: PROTOCOLOS_POR_DEFECTO.to_string(),
        }
    }
}

/// Comprueba que `origen` es `http(s)://host[:puerto]/`, sin credenciales, ruta ni consulta.
fn validar_origen(origen: &str) -> Result<(), ErrorGit> {
    let invalido = || ErrorGit::EntradaInvalida("origen de la credencial no válido".to_string());
    let url = url::Url::parse(origen).map_err(|_| invalido())?;
    let limpio = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && origen.ends_with('/');
    if limpio { Ok(()) } else { Err(invalido()) }
}

/// Salida de un comando `git` que terminó con código cero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Salida {
    pub stdout: String,
    pub stderr: String,
}

/// Ejecuta `git` con `args` sin pasar por ninguna shell.
///
/// El entorno del proceso hijo se limpia y se reconstruye con lo mínimo necesario
/// (`PATH`, `HOME`, `LANG=C`, `GIT_TERMINAL_PROMPT=0`, sin configuración de sistema ni
/// global, `GIT_ALLOW_PROTOCOL`), más las
/// variables `GIT_CONFIG_*` de la credencial cuando se pide explícitamente. La entrada
/// estándar es nula, el proceso se mata si se supera `opciones.tiempo_limite` o si el
/// futuro se descarta, y un código de salida distinto de cero es un error.
pub async fn ejecutar(args: &[&str], opciones: &Opciones) -> Result<Salida, ErrorGit> {
    let mut comando = Command::new("git");
    comando.args(args);
    comando.env_clear();
    if let Ok(valor) = env::var("PATH") {
        comando.env("PATH", valor);
    }
    if let Ok(valor) = env::var("HOME") {
        comando.env("HOME", valor);
    }
    comando.env("LANG", "C");
    comando.env("GIT_TERMINAL_PROMPT", "0");
    comando.env("GIT_CONFIG_NOSYSTEM", "1");
    // Ni el ~/.gitconfig del usuario (helpers de credenciales, `insteadOf`, alias) ni
    // transportes que ejecutan comandos (`ext`) intervienen en nuestras operaciones.
    comando.env("GIT_CONFIG_GLOBAL", "/dev/null");
    comando.env("GIT_ALLOW_PROTOCOL", &opciones.protocolos);

    let mut entradas_config: Vec<(String, String)> = Vec::new();
    if let Some(credencial) = &opciones.credencial {
        validar_origen(&credencial.origen)?;
        entradas_config.push((
            format!("http.{}.extraHeader", credencial.origen),
            cabecera_autorizacion(&credencial.usuario, credencial.token.exponer()),
        ));
    }
    if let Some(certificado) = &opciones.certificado_ca {
        validar_origen(&certificado.origen)?;
        let ruta = certificado.ruta.to_str().ok_or_else(|| {
            ErrorGit::EntradaInvalida("la ruta del certificado no es UTF-8".to_string())
        })?;
        entradas_config.push((
            format!("http.{}.sslCAInfo", certificado.origen),
            ruta.to_string(),
        ));
    }
    if !entradas_config.is_empty() {
        comando.env("GIT_CONFIG_COUNT", entradas_config.len().to_string());
        for (indice, (clave, valor)) in entradas_config.iter().enumerate() {
            comando.env(format!("GIT_CONFIG_KEY_{indice}"), clave);
            comando.env(format!("GIT_CONFIG_VALUE_{indice}"), valor);
        }
    }

    if let Some(directorio) = &opciones.directorio {
        comando.current_dir(directorio);
    }
    comando.stdin(Stdio::null());
    comando.stdout(Stdio::piped());
    comando.stderr(Stdio::piped());
    comando.kill_on_drop(true);

    let salida = timeout(opciones.tiempo_limite, comando.output())
        .await
        .map_err(|_| ErrorGit::Timeout)?
        .map_err(mapear_error_spawn)?;

    let stdout = String::from_utf8_lossy(&salida.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&salida.stderr).into_owned();

    if salida.status.success() {
        Ok(Salida { stdout, stderr })
    } else {
        Err(ErrorGit::Fallo {
            codigo: salida.status.code().unwrap_or(-1),
            stderr: sanear_stderr(stderr),
        })
    }
}

/// Traduce el fallo al arrancar el proceso hijo: sin binario o error de sistema.
fn mapear_error_spawn(error: std::io::Error) -> ErrorGit {
    match error.kind() {
        std::io::ErrorKind::NotFound => ErrorGit::NoEncontrado,
        _ => ErrorGit::Sistema(error.to_string()),
    }
}

/// Cabecera `Authorization: Basic ...` para `usuario` y `token`, en Base64 estándar.
fn cabecera_autorizacion(usuario: &str, token: &str) -> String {
    let credencial = format!("{usuario}:{token}");
    format!(
        "Authorization: Basic {}",
        base64_estandar(credencial.as_bytes())
    )
}

/// Codifica `datos` en Base64 estándar (RFC 4648 §4), con relleno `=`.
///
/// Implementación propia para no añadir una dependencia solo para esto.
fn base64_estandar(datos: &[u8]) -> String {
    const TABLA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut salida = String::with_capacity(datos.len().div_ceil(3) * 4);
    for bloque in datos.chunks(3) {
        let b0 = bloque[0];
        let b1 = bloque.get(1).copied().unwrap_or(0);
        let b2 = bloque.get(2).copied().unwrap_or(0);

        salida.push(TABLA[(b0 >> 2) as usize] as char);
        salida.push(TABLA[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        salida.push(if bloque.len() > 1 {
            TABLA[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        });
        salida.push(if bloque.len() > 2 {
            TABLA[(b2 & 0b0011_1111) as usize] as char
        } else {
            '='
        });
    }
    salida
}

/// Quita credenciales embebidas en URLs (`://usuario:clave@` o `://token@`) y recorta
/// el texto a un tamaño razonable para guardarlo en un error.
fn sanear_stderr(texto: String) -> String {
    let saneado = sanear_credenciales_url(&texto);
    saneado.chars().take(LONGITUD_MAXIMA_STDERR).collect()
}

/// Sustituye la autoridad de usuario de cualquier URL `esquema://...@` por `***`.
fn sanear_credenciales_url(texto: &str) -> String {
    const MARCADOR: &str = "://";
    let mut resultado = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(posicion) = resto.find(MARCADOR) {
        let (antes, con_marcador) = resto.split_at(posicion);
        resultado.push_str(antes);
        resultado.push_str(MARCADOR);
        let despues = &con_marcador[MARCADOR.len()..];

        let fin_autoridad = despues
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(despues.len());
        let autoridad = &despues[..fin_autoridad];

        if let Some(arroba) = autoridad.find('@') {
            resultado.push_str("***@");
            resto = &despues[arroba + 1..];
        } else {
            resto = despues;
        }
    }
    resultado.push_str(resto);
    resultado
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_coincide_con_los_vectores_de_la_rfc_4648() {
        let vectores: &[(&[u8], &str)] = &[
            (b"", ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];
        for (entrada, esperado) in vectores {
            assert_eq!(base64_estandar(entrada), *esperado, "{entrada:?}");
        }
    }

    #[test]
    fn cabecera_autorizacion_codifica_usuario_y_token() {
        assert_eq!(
            cabecera_autorizacion("Aladdin", "open sesame"),
            "Authorization: Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    #[test]
    fn sanea_usuario_y_clave_en_una_url() {
        let entrada = "fatal: no autorizado: https://usuario:s3cr3t@github.com/x.git";
        let saneado = sanear_stderr(entrada.to_string());
        assert!(!saneado.contains("s3cr3t"));
        assert!(!saneado.contains("usuario:s3cr3t"));
        assert!(saneado.contains("https://***@github.com/x.git"));
    }

    #[test]
    fn sanea_un_token_suelto_en_una_url() {
        let entrada = "error: https://ghp_secreto123@github.com/x.git no responde";
        let saneado = sanear_stderr(entrada.to_string());
        assert!(!saneado.contains("ghp_secreto123"));
        assert!(saneado.contains("https://***@github.com/x.git"));
    }

    #[test]
    fn sanea_varias_urls_en_el_mismo_texto() {
        let entrada = "https://a:b@x.com y también http://c:d@y.com";
        let saneado = sanear_stderr(entrada.to_string());
        assert!(!saneado.contains("a:b"));
        assert!(!saneado.contains("c:d"));
        assert!(saneado.contains("https://***@x.com"));
        assert!(saneado.contains("http://***@y.com"));
    }

    #[test]
    fn no_toca_una_url_sin_credenciales() {
        let entrada = "clonando de https://github.com/x/y.git";
        assert_eq!(sanear_stderr(entrada.to_string()), entrada);
    }

    #[test]
    fn recorta_el_stderr_a_dos_mil_caracteres() {
        let entrada = "a".repeat(5000);
        let saneado = sanear_stderr(entrada);
        assert_eq!(saneado.chars().count(), LONGITUD_MAXIMA_STDERR);
    }

    #[test]
    fn mapea_binario_no_encontrado() {
        let error = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(matches!(mapear_error_spawn(error), ErrorGit::NoEncontrado));
    }

    #[test]
    fn mapea_otros_errores_de_e_s_a_sistema() {
        let error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(matches!(mapear_error_spawn(error), ErrorGit::Sistema(_)));
    }

    #[tokio::test]
    async fn ejecutar_agota_el_tiempo_limite_de_forma_deterministica() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        ejecutar(
            &["init", "--bare", "-q"],
            &Opciones {
                directorio: Some(directorio.path().to_path_buf()),
                ..Opciones::default()
            },
        )
        .await
        .expect("git init no falla");

        let opciones = Opciones {
            directorio: Some(directorio.path().to_path_buf()),
            tiempo_limite: Duration::from_millis(200),
            protocolos: "ext".to_string(),
            ..Opciones::default()
        };
        // El transporte «ext» ejecuta el comando local que se le da; con esto se
        // consigue un `git` que se queda colgado sin depender de la red. La espera es
        // muy superior al tiempo límite (200 ms) para que el resultado sea
        // determinista, pero corta para no dejar procesos huérfanos mucho tiempo si
        // `kill_on_drop` no llega a matar al nieto (`git-remote-ext` → `sleep`).
        let resultado = ejecutar(
            &[
                "-c",
                "protocol.ext.allow=always",
                "ls-remote",
                "ext::sleep 5",
            ],
            &opciones,
        )
        .await;
        assert!(matches!(resultado, Err(ErrorGit::Timeout)));
    }

    #[tokio::test]
    async fn ejecutar_devuelve_stdout_en_exito() {
        let salida = ejecutar(&["--version"], &Opciones::default())
            .await
            .expect("git --version no falla");
        assert!(salida.stdout.starts_with("git version"));
    }

    #[test]
    fn validar_origen_solo_acepta_origenes_limpios() {
        for valido in ["https://github.com/", "http://127.0.0.1:3000/"] {
            assert!(validar_origen(valido).is_ok(), "{valido}");
        }
        for invalido in [
            "https://github.com",
            "https://u:p@github.com/",
            "https://github.com/ruta/",
            "https://github.com/?x=1",
            "ext::sh -c id",
            "file:///tmp/",
            "",
        ] {
            assert!(validar_origen(invalido).is_err(), "{invalido}");
        }
    }

    #[tokio::test]
    async fn el_transporte_ext_esta_prohibido_por_defecto() {
        let resultado = ejecutar(
            &["-c", "protocol.ext.allow=always", "ls-remote", "ext::true"],
            &Opciones::default(),
        )
        .await;
        assert!(matches!(resultado, Err(ErrorGit::Fallo { .. })));
    }

    #[tokio::test]
    async fn una_credencial_con_origen_invalido_no_llega_a_ejecutar() {
        let opciones = Opciones {
            credencial: Some(Credencial {
                origen: "https://u:p@github.com/".to_string(),
                usuario: "x".to_string(),
                token: Secreto::nuevo("t"),
            }),
            ..Opciones::default()
        };
        let resultado = ejecutar(&["--version"], &opciones).await;
        assert!(matches!(resultado, Err(ErrorGit::EntradaInvalida(_))));
    }

    // --- certificado_ca (acceso LAN) -----------------------------------

    #[tokio::test]
    async fn un_certificado_ca_fija_sslcainfo_acotado_al_origen() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let opciones = Opciones {
            directorio: Some(directorio.path().to_path_buf()),
            certificado_ca: Some(CertificadoCa {
                origen: "https://127.0.0.1:3000/".to_string(),
                ruta: PathBuf::from("/cuentas/jparga/gitea/tls/cert.pem"),
            }),
            ..Opciones::default()
        };
        let salida = ejecutar(&["config", "--list", "--show-origin"], &opciones)
            .await
            .expect("git config --list no falla");
        assert!(
            salida.stdout.contains(
                "http.https://127.0.0.1:3000/.sslcainfo=/cuentas/jparga/gitea/tls/cert.pem"
            ),
            "{}",
            salida.stdout
        );
    }

    #[tokio::test]
    async fn un_certificado_ca_con_origen_invalido_no_llega_a_ejecutar() {
        let opciones = Opciones {
            certificado_ca: Some(CertificadoCa {
                origen: "https://u:p@127.0.0.1:3000/".to_string(),
                ruta: PathBuf::from("/x/cert.pem"),
            }),
            ..Opciones::default()
        };
        let resultado = ejecutar(&["--version"], &opciones).await;
        assert!(matches!(resultado, Err(ErrorGit::EntradaInvalida(_))));
    }

    #[tokio::test]
    async fn credencial_y_certificado_ca_conviven_en_la_misma_llamada() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let opciones = Opciones {
            directorio: Some(directorio.path().to_path_buf()),
            credencial: Some(Credencial {
                origen: "https://github.com/".to_string(),
                usuario: "x".to_string(),
                token: Secreto::nuevo("t"),
            }),
            certificado_ca: Some(CertificadoCa {
                origen: "https://127.0.0.1:3000/".to_string(),
                ruta: PathBuf::from("/x/cert.pem"),
            }),
            ..Opciones::default()
        };
        let salida = ejecutar(&["config", "--list", "--show-origin"], &opciones)
            .await
            .expect("git config --list no falla");
        assert!(
            salida
                .stdout
                .contains("http.https://github.com/.extraheader=")
        );
        assert!(
            salida
                .stdout
                .contains("http.https://127.0.0.1:3000/.sslcainfo=/x/cert.pem")
        );
    }
}
