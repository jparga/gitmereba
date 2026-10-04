//! Textos de `doctor`, en inglés.

use crate::cuentas::{MotivoAppIni, NombreComprobacion, ParteCuenta, TextoDoctor};

pub(crate) fn nombre(n: &NombreComprobacion) -> String {
    match n {
        NombreComprobacion::Git => "git".to_string(),
        NombreComprobacion::Gpgv => "gpgv".to_string(),
        NombreComprobacion::Llavero => "keyring".to_string(),
        NombreComprobacion::DirectorioDatos => "data-directory".to_string(),
        NombreComprobacion::Auditoria => "audit".to_string(),
        NombreComprobacion::AislamientoSystemd => "systemd-isolation".to_string(),
        NombreComprobacion::Cortafuegos => "firewall".to_string(),
        NombreComprobacion::Cuentas => "accounts".to_string(),
        NombreComprobacion::Idioma => "language".to_string(),
        NombreComprobacion::IdiomaTemporizador => "timer-language".to_string(),
        NombreComprobacion::Cuenta { login, parte } => {
            let parte = match parte {
                ParteCuenta::Carpeta => "folder",
                ParteCuenta::AppIni => "app.ini",
                ParteCuenta::BinarioGitea => "gitea-binary",
                ParteCuenta::Gitea => "gitea",
                ParteCuenta::Secretos => "secrets",
                ParteCuenta::Snapshots => "snapshots",
                ParteCuenta::Temporizador => "timer",
            };
            format!("account:{login}:{parte}")
        }
    }
}

fn motivo(m: &MotivoAppIni) -> String {
    match m {
        MotivoAppIni::Permisos { modo } => format!("permissions {modo:o} (should be 0600)"),
        MotivoAppIni::SinHttpAddrLocal => "does not contain “HTTP_ADDR = 127.0.0.1”: Gitea \
             might listen on the network without LAN access configured in gitmereba.toml"
            .to_string(),
        MotivoAppIni::SinHttps => {
            "does not contain “PROTOCOL = https” despite having LAN access configured".to_string()
        }
        MotivoAppIni::SinCertificado => "the LAN access certificate was not found".to_string(),
        MotivoAppIni::ClaveSinPermisos => {
            "the certificate key does not have permissions 0600".to_string()
        }
    }
}

pub(crate) fn texto(t: &TextoDoctor) -> String {
    match t {
        TextoDoctor::GitVersion { version } => version.clone(),
        TextoDoctor::GitNoDisponible { error } => {
            format!("git is not available: {error}", error = error.en)
        }
        TextoDoctor::ConsejoInstalarGit => {
            "install git and make sure it is on the PATH".to_string()
        }
        TextoDoctor::GpgvDisponible { ruta } => format!("available at {}", ruta.display()),
        TextoDoctor::GpgvNoEncontrado { ruta } => format!("{} not found", ruta.display()),
        TextoDoctor::ConsejoInstalarGnupg => {
            "install the gnupg package (needed to verify the Gitea binary)".to_string()
        }
        TextoDoctor::LlaveroErrorInterno { error } => {
            format!(
                "internal error while checking the keyring: {error}",
                error = error.en
            )
        }
        TextoDoctor::ConsejoRepetirComprobacion => {
            "repeat the check; if it persists, report the error".to_string()
        }
        TextoDoctor::LlaveroAccesible => "accessible".to_string(),
        TextoDoctor::LlaveroNoAccesible { error } => {
            format!("the keyring is not accessible: {error}", error = error.en)
        }
        TextoDoctor::ConsejoSecretService => {
            "check that a Secret Service is running (GNOME Keyring, KWallet)".to_string()
        }
        TextoDoctor::DirectorioDatosNoExiste => {
            "does not exist yet (no account has been added)".to_string()
        }
        TextoDoctor::ConsejoSeCreaConCuentaAdd => {
            "it will be created automatically by “gitmereba cuenta add”".to_string()
        }
        TextoDoctor::PermisosCorrectos0700 => "permissions 0700".to_string(),
        TextoDoctor::PermisosIncorrectos0700 { modo } => {
            format!("permissions {modo:o}, should be 0700")
        }
        TextoDoctor::ConsejoChmod700 { ruta } => format!("run: chmod 700 {}", ruta.display()),
        TextoDoctor::PermisosNoLegibles { error } => {
            format!("could not read its permissions: {error}", error = error.en)
        }
        TextoDoctor::ConsejoDirectorioAccesible => {
            "check that the directory exists and is accessible".to_string()
        }
        TextoDoctor::AislamientoAviso => "This system prevents user services from isolating \
             the file system: the mount protections of the units are not applied. System call \
             and network protections remain active."
            .to_string(),
        TextoDoctor::ConsejoAislamiento => {
            "the mount protections (ProtectSystem, ProtectHome, ReadWritePaths, \
             PrivateTmp) of user units are not applied on this system; the system call \
             (seccomp) and network ones remain active"
                .to_string()
        }
        TextoDoctor::AislamientoOk => "the mount protections of user units are applied".to_string(),
        TextoDoctor::UfwInstalado => {
            "ufw is installed: check that it is active before exposing any account to the LAN"
                .to_string()
        }
        TextoDoctor::ConsejoActivarUfw => {
            "enable it with “sudo ufw enable” and, for each exposed account, limit access with \
             “sudo ufw allow from <network>/<prefix> to any port <port> proto tcp”"
                .to_string()
        }
        TextoDoctor::UfwNoEncontrado => "“ufw” was not found on this system".to_string(),
        TextoDoctor::ConsejoInstalarUfw => {
            "install ufw (or another firewall) before exposing any account to the LAN with \
             “gitmereba cuenta lan --activar”, and limit access to your trusted network"
                .to_string()
        }
        TextoDoctor::AuditoriaIntegra => "chain intact".to_string(),
        TextoDoctor::AuditoriaRota { id } => {
            format!("the audit chain is broken from entry {id}")
        }
        TextoDoctor::ConsejoAuditoriaManipulada => {
            "investigate whether the database file has been tampered with by hand".to_string()
        }
        TextoDoctor::AuditoriaNoVerificable { error } => {
            format!("could not be verified: {error}", error = error.en)
        }
        TextoDoctor::ConsejoAccesoAlmacen => {
            "check access to the store (~/.local/share/gitmereba/gitmereba.db)".to_string()
        }
        TextoDoctor::CuentasIndiceIlegible { error } => {
            format!(
                "could not read the account index: {error}",
                error = error.en
            )
        }
        TextoDoctor::ConsejoPermisosIndice => {
            "check the permissions of ~/.local/share/gitmereba/cuentas.toml".to_string()
        }
        TextoDoctor::CarpetaNoExiste => "the account folder does not exist".to_string(),
        TextoDoctor::ConsejoRepetirAltaORestaurar => {
            "repeat the account setup or restore the folder from a backup".to_string()
        }
        TextoDoctor::CarpetaExisteCon0700 => "exists with permissions 0700".to_string(),
        TextoDoctor::ErrorSistema { error } => error.en.clone(),
        TextoDoctor::ConsejoRevisarPermisosAMano => "check the permissions by hand".to_string(),
        TextoDoctor::AppIniNoExiste => "app.ini does not exist".to_string(),
        TextoDoctor::ConsejoRepetirAltaProvision => {
            "repeat the account setup: provisioning did not complete".to_string()
        }
        TextoDoctor::ConsejoRevisarFicheroLegible => "check that the file is readable".to_string(),
        TextoDoctor::ConsejoFicheroLegible => "check that the file is readable".to_string(),
        TextoDoctor::AppIniSinLanCorrecto => "0600 and HTTP_ADDR = 127.0.0.1".to_string(),
        TextoDoctor::AppIniMotivos { motivos } => {
            motivos.iter().map(motivo).collect::<Vec<_>>().join("; ")
        }
        TextoDoctor::ConsejoAppIniSinLan => {
            "check app.ini by hand; without LAN access it must never listen outside 127.0.0.1"
                .to_string()
        }
        TextoDoctor::AppIniExpuestoLan { host } => {
            format!("exposed to the LAN over HTTPS ({host})")
        }
        TextoDoctor::ConsejoCortafuegosLan => {
            "make sure a firewall limits access to your trusted LAN (see the “firewall” check)"
                .to_string()
        }
        TextoDoctor::ConsejoRegenerarLan => {
            "repeat “gitmereba cuenta lan --activar” to regenerate the certificate and app.ini"
                .to_string()
        }
        TextoDoctor::BinarioNoEncontrado { ruta } => format!("{} not found", ruta.display()),
        TextoDoctor::ConsejoReinstalarBinario => {
            "run the account setup again or “gitmereba doctor” after reinstalling".to_string()
        }
        TextoDoctor::BinarioHashCorrecto { version } => format!("SHA-256 correct ({version})"),
        TextoDoctor::BinarioHashDistinto => {
            "the SHA-256 does not match the expected one".to_string()
        }
        TextoDoctor::ConsejoBorrarBinario => {
            "delete the binary and let the app download and verify it again".to_string()
        }
        TextoDoctor::BinarioHashError { error } => {
            format!("could not compute its SHA-256: {error}", error = error.en)
        }
        TextoDoctor::ConsejoRevisarUrl => "check the account URL".to_string(),
        TextoDoctor::GiteaResponde => "responding".to_string(),
        TextoDoctor::GiteaNoResponde => "not responding".to_string(),
        TextoDoctor::ConsejoArrancarGitea => {
            "start it with “systemctl --user start” or check the service".to_string()
        }
        TextoDoctor::ConsejoRevisarServicioGitea => "check the Gitea service".to_string(),
        TextoDoctor::ConsejoRevisarLlavero => "check the system keyring".to_string(),
        TextoDoctor::SecretosPresentes => "all three secrets are present".to_string(),
        TextoDoctor::SecretosFaltan { faltan } => {
            format!("missing from the keyring: {}", faltan.join(", "))
        }
        TextoDoctor::ConsejoRegenerarSecretos => {
            "repeat the account setup to regenerate them".to_string()
        }
        TextoDoctor::SnapshotsResumen { total, protegidas } => {
            let snapshots = if *total == 1 { "snapshot" } else { "snapshots" };
            format!("{total} {snapshots}, {protegidas} protected")
        }
        TextoDoctor::ConsejoCapturasProtegidas => {
            "there are snapshots protected by a destructive change detected at the origin \
             (rewritten history, deleted branch or tag): review them before normal \
             retention can reach them"
                .to_string()
        }
        TextoDoctor::SnapshotsError { error } => {
            format!("could not list the snapshots: {error}", error = error.en)
        }
        TextoDoctor::ConsejoPermisosSnapshots => {
            "check the permissions of the account's “snapshots/” folder".to_string()
        }
        TextoDoctor::TemporizadorNoInstalado => "no sync timer is installed".to_string(),
        TextoDoctor::ConsejoAbrirVentanaInstala => {
            "open the gitmereba window once: it installs itself; until then it only syncs \
             by hand"
                .to_string()
        }
        TextoDoctor::TemporizadorSinExecStart { ruta } => {
            format!("“{}” has no recognizable ExecStart", ruta.display())
        }
        TextoDoctor::ConsejoAbrirVentanaReescribe => {
            "open the gitmereba window once: it rewrites the unit".to_string()
        }
        TextoDoctor::TemporizadorSincroniza { ejecutable } => {
            format!("syncs with “{ejecutable}”")
        }
        TextoDoctor::TemporizadorEjecutablePerdido { ejecutable } => format!(
            "the timer points to “{ejecutable}”, which no longer exists or is not executable"
        ),
        TextoDoctor::ConsejoActualizarTemporizador => {
            "open the gitmereba window once (or save Settings): the timer switches to the \
             current executable"
                .to_string()
        }
        TextoDoctor::IdiomaPreferencia { idioma, ruta } => {
            format!("{} (preference, {})", idioma.codigo(), ruta.display())
        }
        TextoDoctor::IdiomaVariable {
            idioma,
            variable,
            valor,
        } => format!("{} (from {variable}={valor})", idioma.codigo()),
        TextoDoctor::IdiomaPorDefecto { idioma } => format!(
            "{} (default: LC_ALL, LC_MESSAGES and LANG are not set)",
            idioma.codigo()
        ),
        TextoDoctor::ConsejoFijarIdioma => "set the language in Settings".to_string(),
        TextoDoctor::IdiomaPreferenciaNoValida { idioma, ruta } => format!(
            "could not parse “{}”; language in use: {}",
            ruta.display(),
            idioma.codigo()
        ),
        TextoDoctor::TemporizadorIdiomaSinVariables => "the timer has no LC_ALL, \
             LC_MESSAGES or LANG: its notices will be in English"
            .to_string(),
        TextoDoctor::TemporizadorIdiomaDistinto {
            temporizador,
            sesion,
        } => format!(
            "the timer uses language {} and this session {}",
            temporizador.codigo(),
            sesion.codigo()
        ),
        TextoDoctor::TemporizadorIdiomaCoincide { idioma } => {
            format!(
                "the timer uses the same language as the session ({})",
                idioma.codigo()
            )
        }
        TextoDoctor::TemporizadorIdiomaNoDisponible => {
            "could not query the timer environment (systemctl not available or no user \
             session)"
                .to_string()
        }
        TextoDoctor::ConsejoCorregirPreferencias => {
            "fix or delete the file, or set the language in Settings".to_string()
        }
    }
}
