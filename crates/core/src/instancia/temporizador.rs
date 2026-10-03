//! Unidades `systemd --user` del timer de sincronización periódica de una cuenta.
//!
//! Dos unidades por cuenta: `gitmereba-sync-<login>.service` (`Type=oneshot`, ejecuta
//! `gitmereba sync <login>`) y `gitmereba-sync-<login>.timer` (dispara el anterior cada
//! `intervalo_minutos`). A diferencia de la unidad de Gitea ([`super::servicio`]), este
//! proceso necesita salir a Internet (GitHub) y hablar con el bus de sesión de D-Bus
//! (llavero y notificaciones de escritorio): el endurecimiento aplicado es el mismo
//! subconjunto que se ha comprobado que funciona sin privilegios en unidades de
//! usuario, pero se evita cualquier directiva que bloquee red o D-Bus (en particular no
//! se toca `RestrictAddressFamilies`, que ya admite `AF_INET`/`AF_INET6`/`AF_UNIX`, ni se
//! añade `PrivateNetwork`).
//!
//! Nota de implementación: [`super::servicio`] mantiene `mostrar`/`escapar_valor` y el
//! trío `comando_systemctl`/`ejecutar_systemctl`/`TIEMPO_LIMITE_SYSTEMCTL` como privados
//! del módulo (sin `pub(super)`), así que este módulo no puede reutilizarlos y los
//! duplica. Bastaría con marcarlos `pub(super)` en
//! `servicio.rs` para eliminar la duplicación.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::config::Rutas;
use crate::modelo::Nombre;

use super::error::ErrorInstancia;
use super::fichero;
use super::nombre_unidad;
use super::proceso::mapear_error_spawn;

const TIEMPO_LIMITE_SYSTEMCTL: Duration = Duration::from_secs(30);

/// Parámetros para generar y gestionar el timer de sincronización de una cuenta.
pub struct ParametrosTemporizador<'a> {
    pub login: &'a Nombre,
    /// Ruta absoluta del ejecutable de `gitmereba` (`std::env::current_exe()`, a cargo
    /// del llamador: este módulo no decide cómo se localiza el propio binario).
    pub ruta_ejecutable: &'a Path,
    /// Carpeta raíz de la cuenta, para `ReadWritePaths`.
    pub carpeta_cuenta: &'a Path,
    /// Directorio de datos de la app (`Rutas::directorio_datos()`), también para
    /// `ReadWritePaths`: ahí viven el almacén SQLite y el estado de avisos.
    pub directorio_datos: &'a Path,
    /// Minutos entre sincronizaciones (`OnUnitActiveSec`).
    pub intervalo_minutos: u32,
}

/// Nombre de la unidad de servicio de sincronización de una cuenta.
pub fn nombre_servicio_sync(login: &Nombre) -> String {
    format!("gitmereba-sync-{login}.service")
}

/// Nombre de la unidad de timer de sincronización de una cuenta.
pub fn nombre_timer_sync(login: &Nombre) -> String {
    format!("gitmereba-sync-{login}.timer")
}

fn mostrar(ruta: &Path) -> String {
    ruta.display().to_string()
}

/// Mismas reglas de citado de systemd que [`super::servicio::generar_unidad`] (ver
/// `systemd.syntax`(7)): duplicado a propósito, ver la nota de módulo.
fn escapar_valor(valor: &str) -> String {
    if !valor.chars().any(char::is_whitespace) {
        return valor.to_string();
    }
    let escapado = valor.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escapado}\"")
}

/// Texto de `gitmereba-sync-<login>.service`: `Type=oneshot` que ejecuta `gitmereba sync
/// <login>`, ordenado después de la unidad de Gitea de la cuenta (`After=`/`Wants=`), con
/// prioridad de CPU y E/S baja (`Nice=10`, `IOSchedulingClass=idle`) y un límite de
/// arranque generoso (`TimeoutStartSec=30min`, para cuentas con muchos repos).
pub fn generar_servicio_sync(parametros: &ParametrosTemporizador<'_>) -> String {
    let gitea_unit = nombre_unidad(parametros.login);
    let exec_start = format!(
        "{} sync {}",
        escapar_valor(&mostrar(parametros.ruta_ejecutable)),
        parametros.login,
    );
    let read_write_paths = format!(
        "{} {}",
        escapar_valor(&mostrar(parametros.carpeta_cuenta)),
        escapar_valor(&mostrar(parametros.directorio_datos)),
    );

    format!(
        "[Unit]\n\
         Description=Sincronización periódica de gitmereba ({login})\n\
         After=network.target {gitea_unit}\n\
         Wants={gitea_unit}\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={exec_start}\n\
         Nice=10\n\
         IOSchedulingClass=idle\n\
         TimeoutStartSec=30min\n\
         NoNewPrivileges=yes\n\
         SystemCallFilter=@system-service\n\
         SystemCallFilter=~@privileged\n\
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
         ProtectHostname=yes\n",
        login = parametros.login,
    )
}

/// Texto de `gitmereba-sync-<login>.timer`: arranca 5 minutos tras el arranque de la
/// sesión, luego cada `intervalo_minutos`, con un margen aleatorio de hasta 2 minutos
/// (para no golpear GitHub a la vez desde muchas cuentas/máquinas) y `Persistent=true`
/// (recupera una pasada perdida si el ordenador estaba apagado).
pub fn generar_timer_sync(parametros: &ParametrosTemporizador<'_>) -> String {
    format!(
        "[Unit]\n\
         Description=Timer de sincronización periódica de gitmereba ({login})\n\
         \n\
         [Timer]\n\
         OnBootSec=5min\n\
         OnUnitActiveSec={intervalo}min\n\
         RandomizedDelaySec=2min\n\
         Persistent=true\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n",
        login = parametros.login,
        intervalo = parametros.intervalo_minutos,
    )
}

/// Escribe las dos unidades (0600) en `rutas.directorio_systemd_usuario()` y recarga
/// systemd (`daemon-reload`).
pub async fn instalar_temporizador(
    rutas: &Rutas,
    login: &Nombre,
    texto_service: &str,
    texto_timer: &str,
) -> Result<(), ErrorInstancia> {
    let ruta_service = rutas
        .directorio_systemd_usuario()
        .join(nombre_servicio_sync(login));
    let ruta_timer = rutas
        .directorio_systemd_usuario()
        .join(nombre_timer_sync(login));
    fichero::escribir_privado(&ruta_service, texto_service.as_bytes())?;
    fichero::escribir_privado(&ruta_timer, texto_timer.as_bytes())?;
    ejecutar_systemctl(&["--user", "daemon-reload"]).await?;
    Ok(())
}

/// `systemctl --user enable --now <timer>`.
pub async fn habilitar_temporizador(login: &Nombre) -> Result<(), ErrorInstancia> {
    ejecutar_systemctl(&["--user", "enable", "--now", &nombre_timer_sync(login)])
        .await
        .map(|_| ())
}

/// `systemctl --user restart <timer>`: necesario para que un timer ya activo aplique un
/// `OnUnitActiveSec` nuevo. También lo arranca si estaba parado.
pub async fn reiniciar_temporizador(login: &Nombre) -> Result<(), ErrorInstancia> {
    ejecutar_systemctl(&["--user", "enable", &nombre_timer_sync(login)]).await?;
    ejecutar_systemctl(&["--user", "restart", &nombre_timer_sync(login)])
        .await
        .map(|_| ())
}

/// `systemctl --user disable --now <timer>`. Que la unidad no exista (p. ej. una cuenta
/// que nunca llegó a tener el timer instalado) no se trata como un error: se detecta por
/// el mensaje de `systemctl` (heurística: no hay forma más fiable sin parsear el estado
/// de systemd, y esta función no debe fallar la baja de una cuenta por esto).
pub async fn deshabilitar_temporizador(login: &Nombre) -> Result<(), ErrorInstancia> {
    match ejecutar_systemctl(&["--user", "disable", "--now", &nombre_timer_sync(login)]).await {
        Ok(_) => Ok(()),
        Err(ErrorInstancia::SystemctlFallo { stderr, .. }) if unidad_no_existe(&stderr) => Ok(()),
        Err(error) => Err(error),
    }
}

fn unidad_no_existe(stderr: &str) -> bool {
    let mensaje = stderr.to_lowercase();
    mensaje.contains("no such file")
        || mensaje.contains("does not exist")
        || mensaje.contains("not loaded")
        || mensaje.contains("no encontrada")
        || mensaje.contains("no se encontr")
}

/// Borra los dos ficheros de unidad de `login` de disco, si existen. No ejecuta ningún
/// `systemctl` (ni `daemon-reload`): es un borrado puro de ficheros para que se pueda
/// llamar de forma segura desde `cuentas::baja` sin tocar systemd real en las pruebas
/// (que usan un [`Rutas::con_raiz`] temporal). Que systemd siga recordando la unidad
/// hasta el próximo `daemon-reload` (que ocurrirá, como muy tarde, en la siguiente alta o
/// arranque de sesión) es una limitación conocida; ver el informe de la tarea.
pub fn borrar_unidades(rutas: &Rutas, login: &Nombre) -> Result<(), ErrorInstancia> {
    for ruta in [
        rutas
            .directorio_systemd_usuario()
            .join(nombre_servicio_sync(login)),
        rutas
            .directorio_systemd_usuario()
            .join(nombre_timer_sync(login)),
    ] {
        match std::fs::remove_file(&ruta) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ErrorInstancia::Io(error.to_string())),
        }
    }
    Ok(())
}

/// Entorno del gestor `systemd --user` (`systemctl --user show-environment`): el que
/// heredan los servicios del temporizador. `None` si `systemctl` no está o falla.
pub async fn entorno_gestor_systemd() -> Option<String> {
    ejecutar_systemctl(&["--user", "show-environment"])
        .await
        .ok()
}

/// Copia deliberada de `servicio::comando_systemctl` (privada en su módulo): mismo
/// entorno mínimo (`PATH`, `HOME`, `USER`, `XDG_RUNTIME_DIR`,
/// `DBUS_SESSION_BUS_ADDRESS`).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn parametros<'a>(
        login: &'a Nombre,
        ejecutable: &'a Path,
        carpeta: &'a Path,
        datos: &'a Path,
        intervalo_minutos: u32,
    ) -> ParametrosTemporizador<'a> {
        ParametrosTemporizador {
            login,
            ruta_ejecutable: ejecutable,
            carpeta_cuenta: carpeta,
            directorio_datos: datos,
            intervalo_minutos,
        }
    }

    #[test]
    fn nombres_de_unidad_incluyen_el_login() {
        let login = nombre("jparga");
        assert_eq!(
            nombre_servicio_sync(&login),
            "gitmereba-sync-jparga.service"
        );
        assert_eq!(nombre_timer_sync(&login), "gitmereba-sync-jparga.timer");
    }

    #[test]
    fn el_servicio_ejecuta_sync_del_login_con_la_ruta_del_ejecutable() {
        let login = nombre("jparga");
        let unidad = generar_servicio_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            30,
        ));
        assert!(unidad.contains("ExecStart=/usr/bin/gitmereba sync jparga"));
        assert!(unidad.contains("Type=oneshot"));
    }

    #[test]
    fn el_servicio_depende_de_la_unidad_de_gitea_de_la_cuenta() {
        let login = nombre("jparga");
        let unidad = generar_servicio_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            30,
        ));
        assert!(unidad.contains("After=network.target gitmereba-gitea-jparga.service"));
        assert!(unidad.contains("Wants=gitmereba-gitea-jparga.service"));
    }

    #[test]
    fn el_servicio_incluye_las_directivas_pedidas() {
        let login = nombre("jparga");
        let unidad = generar_servicio_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            30,
        ));
        for esperado in [
            "Nice=10",
            "IOSchedulingClass=idle",
            "TimeoutStartSec=30min",
            "NoNewPrivileges=yes",
            "SystemCallFilter=@system-service",
            "SystemCallArchitectures=native",
            "ProtectSystem=strict",
            "ReadWritePaths=/cuentas/jparga /datos/gitmereba",
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
        ] {
            assert!(
                unidad.contains(esperado),
                "falta «{esperado}» en:\n{unidad}"
            );
        }
    }

    #[test]
    fn el_servicio_no_bloquea_red_ni_dbus() {
        let login = nombre("jparga");
        let unidad = generar_servicio_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            30,
        ));
        for prohibido in [
            "PrivateNetwork",
            "IPAddressDeny",
            "PrivateDevices",
            "ProtectKernelModules",
            "CapabilityBoundingSet",
        ] {
            assert!(
                !unidad.contains(prohibido),
                "no debería contener «{prohibido}»"
            );
        }
        // AF_UNIX (D-Bus de sesión) y AF_INET/AF_INET6 (GitHub) siguen permitidos.
        assert!(unidad.contains("AF_UNIX"));
        assert!(unidad.contains("AF_INET"));
    }

    #[test]
    fn escapa_rutas_con_espacios() {
        let login = nombre("jparga");
        let unidad = generar_servicio_sync(&parametros(
            &login,
            Path::new("/home/u/Mis Clones/bin/gitmereba"),
            Path::new("/home/u/Mis Clones/jparga"),
            Path::new("/home/u/.local/share/gitmereba"),
            30,
        ));
        assert!(unidad.contains("ExecStart=\"/home/u/Mis Clones/bin/gitmereba\" sync jparga"));
        assert!(unidad.contains(
            "ReadWritePaths=\"/home/u/Mis Clones/jparga\" /home/u/.local/share/gitmereba"
        ));
    }

    #[test]
    fn no_escapa_rutas_sin_espacios() {
        let login = nombre("jparga");
        let unidad = generar_servicio_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/x"),
            Path::new("/y"),
            30,
        ));
        assert!(!unidad.contains("ReadWritePaths=\""));
        assert!(!unidad.contains("ExecStart=\""));
    }

    #[test]
    fn el_timer_incluye_el_intervalo_y_las_directivas_pedidas() {
        let login = nombre("jparga");
        let unidad = generar_timer_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            30,
        ));
        for esperado in [
            "OnBootSec=5min",
            "OnUnitActiveSec=30min",
            "RandomizedDelaySec=2min",
            "Persistent=true",
            "[Install]",
            "WantedBy=timers.target",
        ] {
            assert!(
                unidad.contains(esperado),
                "falta «{esperado}» en:\n{unidad}"
            );
        }
    }

    #[test]
    fn el_timer_usa_el_intervalo_de_la_cuenta() {
        let login = nombre("jparga");
        let unidad = generar_timer_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            90,
        ));
        assert!(unidad.contains("OnUnitActiveSec=90min"));
    }

    #[test]
    fn el_timer_menciona_el_login_en_la_descripcion() {
        let login = nombre("jparga");
        let unidad = generar_timer_sync(&parametros(
            &login,
            Path::new("/usr/bin/gitmereba"),
            Path::new("/cuentas/jparga"),
            Path::new("/datos/gitmereba"),
            30,
        ));
        assert!(unidad.contains("jparga"));
    }

    #[test]
    fn deshabilitar_temporizador_trata_la_ausencia_de_la_unidad_como_exito() {
        assert!(unidad_no_existe(
            "Failed to disable unit: Unit file gitmereba-sync-jparga.timer does not exist."
        ));
        assert!(unidad_no_existe(
            "Failed to disable unit: No such file or directory"
        ));
        assert!(!unidad_no_existe("Access denied"));
    }
}
