//! Unidad `systemd --user` de una cuenta y arranque en primer plano para pruebas y
//! `doctor`.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::time::{Instant, sleep, timeout};

use crate::config::Rutas;
use crate::modelo::Nombre;

use super::error::ErrorInstancia;
use super::fichero;
use super::proceso::mapear_error_spawn;

const TIEMPO_LIMITE_SYSTEMCTL: Duration = Duration::from_secs(30);
const TIEMPO_LIMITE_ARRANQUE: Duration = Duration::from_secs(30);

/// Parámetros para [`generar_unidad`].
pub struct ParametrosUnidad<'a> {
    pub login: &'a Nombre,
    pub binario_gitea: &'a Path,
    pub app_ini: &'a Path,
    /// `GITEA_WORK_DIR`: la carpeta `gitea/` de la cuenta.
    pub directorio_trabajo: &'a Path,
    /// Carpeta raíz de la cuenta, para `ReadWritePaths`.
    pub carpeta_cuenta: &'a Path,
}

/// Nombre de la unidad de una cuenta: una unidad concreta por cuenta (no una
/// plantilla `@.service`), porque cada una necesita una carpeta distinta.
pub fn nombre_unidad(login: &Nombre) -> String {
    format!("gitmereba-gitea-{login}.service")
}

/// Texto de la unidad `systemd --user` de una cuenta, con el endurecimiento que
/// se ha comprobado que funciona sin privilegios (`NoNewPrivileges`, `ProtectSystem`,
/// ...). Omite deliberadamente `PrivateDevices`, `ProtectKernelModules` y
/// `CapabilityBoundingSet=`: fallan en unidades `--user`.
///
/// Qué protege de verdad: en Ubuntu, AppArmor restringe los espacios de nombres sin
/// privilegios y el gestor `systemd --user` no puede montar nada, así que `ProtectSystem`,
/// `ProtectHome`, `ReadWritePaths` y `PrivateTmp` se IGNORAN en silencio (comprobado con
/// unidades transitorias). Sí se aplican las directivas basadas en seccomp y prctl:
/// `NoNewPrivileges`, `SystemCallFilter`, `RestrictAddressFamilies`, `RestrictNamespaces`,
/// `MemoryDenyWriteExecute`, `LockPersonality`, `UMask`. Las de montaje se conservan porque
/// son efectivas donde el sistema lo permite; `ProtectHome` es `read-only` y no `yes`
/// porque `yes` dejaría inaccesible una carpeta de cuenta situada bajo `/home`.
pub fn generar_unidad(parametros: &ParametrosUnidad<'_>) -> String {
    let exec_start = format!(
        "{} web --config {}",
        escapar_valor(&mostrar(parametros.binario_gitea)),
        escapar_valor(&mostrar(parametros.app_ini)),
    );
    let entorno = escapar_valor(&format!(
        "GITEA_WORK_DIR={}",
        mostrar(parametros.directorio_trabajo)
    ));
    let read_write_paths = escapar_valor(&mostrar(parametros.carpeta_cuenta));

    format!(
        "[Unit]\n\
         Description=Gitea nativo de gitmereba ({login})\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         Environment={entorno}\n\
         ExecStart={exec_start}\n\
         Restart=on-failure\n\
         NoNewPrivileges=yes\n\
         SystemCallFilter=@system-service\n\
         SystemCallFilter=~@privileged @resources\n\
         SystemCallArchitectures=native\n\
         SystemCallErrorNumber=EPERM\n\
         ProtectSystem=strict\n\
         ReadWritePaths={read_write_paths}\n\
         PrivateTmp=yes\n\
         ProtectHome=read-only\n\
         RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX\n\
         LockPersonality=yes\n\
         UMask=0077\n\
         RestrictSUIDSGID=yes\n\
         ProtectKernelTunables=yes\n\
         ProtectControlGroups=yes\n\
         RestrictNamespaces=yes\n\
         MemoryDenyWriteExecute=yes\n\
         RestrictRealtime=yes\n\
         RemoveIPC=yes\n\
         KeyringMode=private\n\
         ProtectHostname=yes\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        login = parametros.login,
    )
}

fn mostrar(ruta: &Path) -> String {
    ruta.display().to_string()
}

/// Aplica las reglas de citado de systemd (`systemd.syntax`(7)): un valor con
/// espacios se envuelve entre comillas dobles, escapando `\` y `"` dentro de ellas.
fn escapar_valor(valor: &str) -> String {
    if !valor.chars().any(char::is_whitespace) {
        return valor.to_string();
    }
    let escapado = valor.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escapado}\"")
}

/// Escribe la unidad de `login` en `rutas.directorio_systemd_usuario()` (0600) y
/// recarga systemd (`daemon-reload`).
pub async fn instalar_unidad(
    rutas: &Rutas,
    login: &Nombre,
    texto: &str,
) -> Result<std::path::PathBuf, ErrorInstancia> {
    let ruta = rutas
        .directorio_systemd_usuario()
        .join(nombre_unidad(login));
    fichero::escribir_privado(&ruta, texto.as_bytes())?;
    ejecutar_systemctl(&["--user", "daemon-reload"]).await?;
    Ok(ruta)
}

/// `systemctl --user enable --now <unidad>`: arranca ya y en cada inicio de sesión.
pub async fn arrancar(login: &Nombre) -> Result<(), ErrorInstancia> {
    ejecutar_systemctl(&["--user", "enable", "--now", &nombre_unidad(login)])
        .await
        .map(|_| ())
}

/// `systemctl --user disable --now <unidad>`: para y deja de arrancar con la sesión.
pub async fn parar(login: &Nombre) -> Result<(), ErrorInstancia> {
    ejecutar_systemctl(&["--user", "disable", "--now", &nombre_unidad(login)])
        .await
        .map(|_| ())
}

/// `systemctl --user is-active <unidad>`, recortado. Un código de salida distinto de
/// cero no es un error aquí: `is-active` lo usa precisamente para decir «inactive» o
/// «failed».
pub async fn estado(login: &Nombre) -> Result<String, ErrorInstancia> {
    let mut comando = comando_systemctl(&["--user", "is-active", &nombre_unidad(login)]);
    let salida = timeout(TIEMPO_LIMITE_SYSTEMCTL, comando.output())
        .await
        .map_err(|_| ErrorInstancia::Timeout)?
        .map_err(mapear_error_spawn)?;
    Ok(String::from_utf8_lossy(&salida.stdout).trim().to_string())
}

fn comando_systemctl(args: &[&str]) -> Command {
    let mut comando = Command::new("systemctl");
    comando.args(args);
    comando.env_clear();
    for variable in [
        "PATH",
        "HOME",
        "USER",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        if let Ok(valor) = std::env::var(variable) {
            comando.env(variable, valor);
        }
    }
    comando.stdin(Stdio::null());
    comando.stdout(Stdio::piped());
    comando.stderr(Stdio::piped());
    comando.kill_on_drop(true);
    comando
}

async fn ejecutar_systemctl(args: &[&str]) -> Result<String, ErrorInstancia> {
    let mut comando = comando_systemctl(args);
    let salida = timeout(TIEMPO_LIMITE_SYSTEMCTL, comando.output())
        .await
        .map_err(|_| ErrorInstancia::Timeout)?
        .map_err(mapear_error_spawn)?;
    if salida.status.success() {
        Ok(String::from_utf8_lossy(&salida.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&salida.stderr).into_owned();
        Err(ErrorInstancia::SystemctlFallo {
            codigo: salida.status.code().unwrap_or(-1),
            stderr: stderr.chars().take(2000).collect(),
        })
    }
}

/// `gitea web` arrancado como proceso hijo, para los tests de integración y
/// `doctor`: sin pasar por systemd.
pub struct ProcesoGitea {
    hijo: Child,
}

impl ProcesoGitea {
    /// Arranca `gitea web --config app_ini` con `GITEA_WORK_DIR=directorio_trabajo` y
    /// espera hasta 30 s a que `{url_base}/api/healthz` responda con éxito.
    ///
    /// `url_base` puede ser `http://127.0.0.1:<puerto>` (`certificado_pem` se ignora) o,
    /// con acceso LAN activo, `https://127.0.0.1:<puerto>`: en ese caso
    /// `certificado_pem` es obligatorio y es el **único** certificado en el que confía
    /// la sonda, nunca las CA del sistema.
    pub async fn arrancar_en_primer_plano(
        binario: &Path,
        app_ini: &Path,
        directorio_trabajo: &Path,
        url_base: &str,
        certificado_pem: Option<&[u8]>,
    ) -> Result<Self, ErrorInstancia> {
        let mut comando = Command::new(binario);
        comando.arg("web").arg("--config").arg(app_ini);
        comando.env_clear();
        for variable in ["PATH", "HOME", "USER"] {
            if let Ok(valor) = std::env::var(variable) {
                comando.env(variable, valor);
            }
        }
        comando.env("GITEA_WORK_DIR", directorio_trabajo);
        comando.stdin(Stdio::null());
        comando.stdout(Stdio::null());
        comando.stderr(Stdio::null());
        comando.kill_on_drop(true);

        let hijo = comando.spawn().map_err(mapear_error_spawn)?;
        let proceso = Self { hijo };
        esperar_salud(url_base, certificado_pem).await?;
        Ok(proceso)
    }

    /// Mata el proceso y espera a que termine.
    pub async fn parar(mut self) -> Result<(), ErrorInstancia> {
        let _ = self.hijo.start_kill();
        let _ = self.hijo.wait().await;
        Ok(())
    }
}

async fn esperar_salud(
    url_base: &str,
    certificado_pem: Option<&[u8]>,
) -> Result<(), ErrorInstancia> {
    let mut constructor = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy();
    if let Some(pem) = certificado_pem {
        let certificado = reqwest::Certificate::from_pem(pem)
            .map_err(|e| ErrorInstancia::Red(e.without_url().to_string()))?;
        // Igual que `gitea::ClienteGitea::nuevo_con_certificado`: confianza acotada a
        // este único certificado autofirmado, nunca a las CA del sistema.
        constructor = constructor.tls_certs_only([certificado]);
    }
    let cliente = constructor
        .build()
        .map_err(|e| ErrorInstancia::Red(e.without_url().to_string()))?;
    let url = format!("{url_base}/api/healthz");
    let limite = Instant::now() + TIEMPO_LIMITE_ARRANQUE;

    loop {
        if let Ok(respuesta) = cliente.get(&url).send().await
            && respuesta.status().is_success()
        {
            return Ok(());
        }
        if Instant::now() >= limite {
            return Err(ErrorInstancia::ArranqueTimeout);
        }
        sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn parametros<'a>(
        login: &'a Nombre,
        binario: &'a Path,
        app_ini: &'a Path,
        trabajo: &'a Path,
        carpeta: &'a Path,
    ) -> ParametrosUnidad<'a> {
        ParametrosUnidad {
            login,
            binario_gitea: binario,
            app_ini,
            directorio_trabajo: trabajo,
            carpeta_cuenta: carpeta,
        }
    }

    #[test]
    fn nombre_unidad_incluye_el_login() {
        assert_eq!(
            nombre_unidad(&nombre("jparga")),
            "gitmereba-gitea-jparga.service"
        );
    }

    #[test]
    fn incluye_las_directivas_de_endurecimiento_que_funcionan_sin_privilegios() {
        let login = nombre("jparga");
        let unidad = generar_unidad(&parametros(
            &login,
            Path::new("/bin/gitea"),
            Path::new("/cuentas/jparga/gitea/custom/conf/app.ini"),
            Path::new("/cuentas/jparga/gitea"),
            Path::new("/cuentas/jparga"),
        ));

        for esperado in [
            "NoNewPrivileges=yes",
            "SystemCallFilter=@system-service",
            "SystemCallArchitectures=native",
            "ProtectSystem=strict",
            "ReadWritePaths=/cuentas/jparga",
            "PrivateTmp=yes",
            "ProtectHome=read-only",
            "RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX",
            "LockPersonality=yes",
            "UMask=0077",
            "RestrictSUIDSGID=yes",
            "ProtectKernelTunables=yes",
            "ProtectControlGroups=yes",
            "RestrictNamespaces=yes",
            "MemoryDenyWriteExecute=yes",
            "RestrictRealtime=yes",
            "RemoveIPC=yes",
            "KeyringMode=private",
            "ProtectHostname=yes",
            "Restart=on-failure",
            "Environment=GITEA_WORK_DIR=/cuentas/jparga/gitea",
            "ExecStart=/bin/gitea web --config /cuentas/jparga/gitea/custom/conf/app.ini",
            "WantedBy=default.target",
        ] {
            assert!(
                unidad.contains(esperado),
                "falta «{esperado}» en:\n{unidad}"
            );
        }
    }

    #[test]
    fn omite_las_directivas_que_fallan_en_unidades_de_usuario() {
        let login = nombre("jparga");
        let unidad = generar_unidad(&parametros(
            &login,
            Path::new("/bin/gitea"),
            Path::new("/x/app.ini"),
            Path::new("/x/gitea"),
            Path::new("/x"),
        ));
        for prohibido in [
            "PrivateDevices",
            "ProtectKernelModules",
            "CapabilityBoundingSet",
        ] {
            assert!(
                !unidad.contains(prohibido),
                "no debería contener «{prohibido}»"
            );
        }
    }

    #[test]
    fn escapa_rutas_con_espacios_segun_las_reglas_de_systemd() {
        let login = nombre("jparga");
        let unidad = generar_unidad(&parametros(
            &login,
            Path::new("/home/a b/bin/gitea"),
            Path::new("/home/a b/gitea/custom/conf/app.ini"),
            Path::new("/home/a b/gitea"),
            Path::new("/home/a b"),
        ));

        assert!(unidad.contains("ReadWritePaths=\"/home/a b\""));
        assert!(unidad.contains("Environment=\"GITEA_WORK_DIR=/home/a b/gitea\""));
        assert!(unidad.contains(
            "ExecStart=\"/home/a b/bin/gitea\" web --config \"/home/a b/gitea/custom/conf/app.ini\""
        ));
    }

    #[test]
    fn no_escapa_rutas_sin_espacios() {
        let login = nombre("jparga");
        let unidad = generar_unidad(&parametros(
            &login,
            Path::new("/bin/gitea"),
            Path::new("/x/app.ini"),
            Path::new("/x/gitea"),
            Path::new("/x"),
        ));
        assert!(unidad.contains("ReadWritePaths=/x\n"));
        assert!(!unidad.contains("ReadWritePaths=\""));
    }

    #[test]
    fn escapar_valor_escapa_comillas_y_barras_invertidas_internas() {
        assert_eq!(escapar_valor(r#"/a "raro"\b"#), "\"/a \\\"raro\\\"\\\\b\"");
    }
}
