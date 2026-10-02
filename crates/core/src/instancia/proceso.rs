//! Invocación del binario de `gitea` como proceso hijo que termina solo: sin shell,
//! entorno mínimo, con tiempo límite. Ver `git::proceso` para el mismo patrón.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use super::error::ErrorInstancia;

/// Longitud máxima del `stderr` que se conserva en un error.
const LONGITUD_MAXIMA_STDERR: usize = 2000;

/// Salida de un comando `gitea` que terminó con código cero.
#[derive(Debug)]
pub(super) struct SalidaGitea {
    pub stdout: String,
}

/// Ejecuta `gitea` (en `binario`) con `args`, sin pasar por ninguna shell.
///
/// El entorno se limpia y se reconstruye con `PATH`, `HOME`, `USER` y
/// `GITEA_WORK_DIR=directorio_trabajo`. La entrada estándar es nula, el proceso se
/// mata si se supera `tiempo_limite` o si el futuro se descarta, y un código de
/// salida distinto de cero es un error con el `stderr` recortado.
///
/// No registra nunca el `stdout`: algunas invocaciones (`generate secret`,
/// `generate-access-token`) lo usan para devolver un secreto.
pub(super) async fn ejecutar_gitea(
    binario: &Path,
    args: &[&str],
    directorio_trabajo: &Path,
    tiempo_limite: Duration,
) -> Result<SalidaGitea, ErrorInstancia> {
    let mut comando = Command::new(binario);
    comando.args(args);
    comando.env_clear();
    for variable in ["PATH", "HOME", "USER"] {
        if let Ok(valor) = std::env::var(variable) {
            comando.env(variable, valor);
        }
    }
    comando.env("GITEA_WORK_DIR", directorio_trabajo);
    comando.stdin(Stdio::null());
    comando.stdout(Stdio::piped());
    comando.stderr(Stdio::piped());
    comando.kill_on_drop(true);

    let salida = timeout(tiempo_limite, comando.output())
        .await
        .map_err(|_| ErrorInstancia::Timeout)?
        .map_err(mapear_error_spawn)?;

    let stdout = String::from_utf8_lossy(&salida.stdout).into_owned();
    if salida.status.success() {
        Ok(SalidaGitea { stdout })
    } else {
        let stderr = String::from_utf8_lossy(&salida.stderr).into_owned();
        Err(ErrorInstancia::GiteaFallo {
            codigo: salida.status.code().unwrap_or(-1),
            stderr: recortar(&stderr),
        })
    }
}

/// Traduce el fallo al arrancar el proceso hijo: binario no encontrado o error de
/// sistema. Se reutiliza también para `gitea web` (arranque en primer plano).
pub(super) fn mapear_error_spawn(error: std::io::Error) -> ErrorInstancia {
    match error.kind() {
        std::io::ErrorKind::NotFound => ErrorInstancia::GiteaNoEncontrado,
        _ => ErrorInstancia::Io(error.to_string()),
    }
}

fn recortar(texto: &str) -> String {
    texto.chars().take(LONGITUD_MAXIMA_STDERR).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binario_inexistente_da_gitea_no_encontrado() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let resultado = ejecutar_gitea(
            Path::new("/no/existe/gitea"),
            &["--version"],
            directorio.path(),
            Duration::from_secs(5),
        )
        .await;
        assert!(matches!(resultado, Err(ErrorInstancia::GiteaNoEncontrado)));
    }

    #[tokio::test]
    async fn un_codigo_de_salida_distinto_de_cero_es_un_fallo_con_stderr_recortado() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let script = directorio.path().join("gitea-falso.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho -n '{}' >&2\nexit 3\n", "e".repeat(3000)),
        )
        .expect("escribir script");
        marcar_ejecutable(&script);

        let resultado =
            ejecutar_gitea(&script, &[], directorio.path(), Duration::from_secs(5)).await;
        match resultado {
            Err(ErrorInstancia::GiteaFallo { codigo, stderr }) => {
                assert_eq!(codigo, 3);
                assert_eq!(stderr.chars().count(), LONGITUD_MAXIMA_STDERR);
            }
            otro => panic!("se esperaba GiteaFallo, se obtuvo {otro:?}"),
        }
    }

    #[tokio::test]
    async fn agota_el_tiempo_limite_de_forma_deterministica() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let script = directorio.path().join("gitea-lento.sh");
        std::fs::write(&script, "#!/bin/sh\nsleep 5\n").expect("escribir script");
        marcar_ejecutable(&script);

        let resultado =
            ejecutar_gitea(&script, &[], directorio.path(), Duration::from_millis(100)).await;
        assert!(matches!(resultado, Err(ErrorInstancia::Timeout)));
    }

    #[tokio::test]
    async fn el_stdout_llega_intacto_en_exito() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let script = directorio.path().join("gitea-eco.sh");
        std::fs::write(&script, "#!/bin/sh\necho -n \"$GITEA_WORK_DIR\"\n")
            .expect("escribir script");
        marcar_ejecutable(&script);

        let salida = ejecutar_gitea(&script, &[], directorio.path(), Duration::from_secs(5))
            .await
            .expect("el script no falla");
        assert_eq!(salida.stdout, directorio.path().display().to_string());
    }

    fn marcar_ejecutable(ruta: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable");
    }
}
