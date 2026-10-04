//! Textos de `doctor`, en español.

use crate::cuentas::{MotivoAppIni, NombreComprobacion, ParteCuenta, TextoDoctor};

pub(crate) fn nombre(n: &NombreComprobacion) -> String {
    match n {
        NombreComprobacion::Git => "git".to_string(),
        NombreComprobacion::Gpgv => "gpgv".to_string(),
        NombreComprobacion::Llavero => "llavero".to_string(),
        NombreComprobacion::DirectorioDatos => "directorio-datos".to_string(),
        NombreComprobacion::Auditoria => "auditoria".to_string(),
        NombreComprobacion::AislamientoSystemd => "aislamiento-systemd".to_string(),
        NombreComprobacion::Cortafuegos => "cortafuegos".to_string(),
        NombreComprobacion::Cuentas => "cuentas".to_string(),
        NombreComprobacion::Idioma => "idioma".to_string(),
        NombreComprobacion::IdiomaTemporizador => "idioma-temporizador".to_string(),
        NombreComprobacion::Cuenta { login, parte } => {
            let parte = match parte {
                ParteCuenta::Carpeta => "carpeta",
                ParteCuenta::AppIni => "app.ini",
                ParteCuenta::BinarioGitea => "binario-gitea",
                ParteCuenta::Gitea => "gitea",
                ParteCuenta::Secretos => "secretos",
                ParteCuenta::Snapshots => "snapshots",
                ParteCuenta::Temporizador => "temporizador",
            };
            format!("cuenta:{login}:{parte}")
        }
    }
}

fn motivo(m: &MotivoAppIni) -> String {
    match m {
        MotivoAppIni::Permisos { modo } => format!("permisos {modo:o} (deberían ser 0600)"),
        MotivoAppIni::SinHttpAddrLocal => "no contiene «HTTP_ADDR = 127.0.0.1»: Gitea podría \
             escuchar en la red sin acceso LAN configurado en gitmereba.toml"
            .to_string(),
        MotivoAppIni::SinHttps => {
            "no contiene «PROTOCOL = https» pese a tener acceso LAN configurado".to_string()
        }
        MotivoAppIni::SinCertificado => "no se encuentra el certificado del acceso LAN".to_string(),
        MotivoAppIni::ClaveSinPermisos => {
            "la clave del certificado no tiene permisos 0600".to_string()
        }
    }
}

pub(crate) fn texto(t: &TextoDoctor) -> String {
    match t {
        TextoDoctor::GitVersion { version } => version.clone(),
        TextoDoctor::GitNoDisponible { error } => {
            format!("git no está disponible: {error}", error = error.es)
        }
        TextoDoctor::ConsejoInstalarGit => {
            "instala git y asegúrate de que está en el PATH".to_string()
        }
        TextoDoctor::GpgvDisponible { ruta } => format!("disponible en {}", ruta.display()),
        TextoDoctor::GpgvNoEncontrado { ruta } => format!("no se encuentra {}", ruta.display()),
        TextoDoctor::ConsejoInstalarGnupg => {
            "instala el paquete gnupg (necesario para verificar el binario de Gitea)".to_string()
        }
        TextoDoctor::LlaveroErrorInterno { error } => {
            format!(
                "error interno al comprobar el llavero: {error}",
                error = error.es
            )
        }
        TextoDoctor::ConsejoRepetirComprobacion => {
            "repite la comprobación; si persiste, informa del error".to_string()
        }
        TextoDoctor::LlaveroAccesible => "accesible".to_string(),
        TextoDoctor::LlaveroNoAccesible { error } => {
            format!("el llavero no está accesible: {error}", error = error.es)
        }
        TextoDoctor::ConsejoSecretService => {
            "comprueba que hay un Secret Service en marcha (GNOME Keyring, KWallet)".to_string()
        }
        TextoDoctor::DirectorioDatosNoExiste => {
            "todavía no existe (no se ha dado de alta ninguna cuenta)".to_string()
        }
        TextoDoctor::ConsejoSeCreaConCuentaAdd => {
            "se creará automáticamente con «gitmereba cuenta add»".to_string()
        }
        TextoDoctor::PermisosCorrectos0700 => "permisos 0700".to_string(),
        TextoDoctor::PermisosIncorrectos0700 { modo } => {
            format!("permisos {modo:o}, deberían ser 0700")
        }
        TextoDoctor::ConsejoChmod700 { ruta } => format!("ejecuta: chmod 700 {}", ruta.display()),
        TextoDoctor::PermisosNoLegibles { error } => {
            format!("no se pudo leer sus permisos: {error}", error = error.es)
        }
        TextoDoctor::ConsejoDirectorioAccesible => {
            "comprueba que el directorio existe y es accesible".to_string()
        }
        TextoDoctor::AislamientoAviso => "Este sistema impide a los servicios de usuario aislar \
             el sistema de ficheros: las protecciones de montaje de las unidades no se aplican. \
             Siguen activas las de llamadas al sistema y red."
            .to_string(),
        TextoDoctor::ConsejoAislamiento => {
            "las protecciones de montaje (ProtectSystem, ProtectHome, ReadWritePaths, \
             PrivateTmp) de las unidades de usuario no se aplican en este sistema; las de \
             llamadas al sistema (seccomp) y red siguen activas"
                .to_string()
        }
        TextoDoctor::AislamientoOk => {
            "las protecciones de montaje de las unidades de usuario se aplican".to_string()
        }
        TextoDoctor::UfwInstalado => {
            "ufw está instalado: revisa que esté activo antes de exponer alguna cuenta a la LAN"
                .to_string()
        }
        TextoDoctor::ConsejoActivarUfw => {
            "actívalo con «sudo ufw enable» y, para cada cuenta expuesta, limita el acceso con \
             «sudo ufw allow from <red>/<prefijo> to any port <puerto> proto tcp»"
                .to_string()
        }
        TextoDoctor::UfwNoEncontrado => "no se encontró «ufw» en este sistema".to_string(),
        TextoDoctor::ConsejoInstalarUfw => {
            "instala ufw (u otro cortafuegos) antes de exponer alguna cuenta a la LAN con \
             «gitmereba cuenta lan --activar», y limita el acceso a tu red de confianza"
                .to_string()
        }
        TextoDoctor::AuditoriaIntegra => "cadena íntegra".to_string(),
        TextoDoctor::AuditoriaRota { id } => {
            format!("la cadena de auditoría está rota a partir de la entrada {id}")
        }
        TextoDoctor::ConsejoAuditoriaManipulada => {
            "investiga si el fichero de la base de datos se ha manipulado a mano".to_string()
        }
        TextoDoctor::AuditoriaNoVerificable { error } => {
            format!("no se pudo verificar: {error}", error = error.es)
        }
        TextoDoctor::ConsejoAccesoAlmacen => {
            "comprueba el acceso al almacén (~/.local/share/gitmereba/gitmereba.db)".to_string()
        }
        TextoDoctor::CuentasIndiceIlegible { error } => {
            format!(
                "no se pudo leer el índice de cuentas: {error}",
                error = error.es
            )
        }
        TextoDoctor::ConsejoPermisosIndice => {
            "revisa los permisos de ~/.local/share/gitmereba/cuentas.toml".to_string()
        }
        TextoDoctor::CarpetaNoExiste => "la carpeta de la cuenta no existe".to_string(),
        TextoDoctor::ConsejoRepetirAltaORestaurar => {
            "repite el alta o restaura la carpeta desde una copia".to_string()
        }
        TextoDoctor::CarpetaExisteCon0700 => "existe con permisos 0700".to_string(),
        TextoDoctor::ErrorSistema { error } => error.es.clone(),
        TextoDoctor::ConsejoRevisarPermisosAMano => "revisa los permisos a mano".to_string(),
        TextoDoctor::AppIniNoExiste => "app.ini no existe".to_string(),
        TextoDoctor::ConsejoRepetirAltaProvision => {
            "repite el alta: la provisión no llegó a completarse".to_string()
        }
        TextoDoctor::ConsejoRevisarFicheroLegible => "revisa que el fichero es legible".to_string(),
        TextoDoctor::ConsejoFicheroLegible => "comprueba que el fichero es legible".to_string(),
        TextoDoctor::AppIniSinLanCorrecto => "0600 y HTTP_ADDR = 127.0.0.1".to_string(),
        TextoDoctor::AppIniMotivos { motivos } => {
            motivos.iter().map(motivo).collect::<Vec<_>>().join("; ")
        }
        TextoDoctor::ConsejoAppIniSinLan => {
            "revisa app.ini a mano; sin acceso LAN nunca debe escuchar fuera de 127.0.0.1"
                .to_string()
        }
        TextoDoctor::AppIniExpuestoLan { host } => format!("expuesto a la LAN por HTTPS ({host})"),
        TextoDoctor::ConsejoCortafuegosLan => {
            "confirma que hay un cortafuegos limitando el acceso a tu LAN de confianza \
             (ver la comprobación «cortafuegos»)"
                .to_string()
        }
        TextoDoctor::ConsejoRegenerarLan => {
            "repite «gitmereba cuenta lan --activar» para regenerar el certificado y app.ini"
                .to_string()
        }
        TextoDoctor::BinarioNoEncontrado { ruta } => format!("no se encuentra {}", ruta.display()),
        TextoDoctor::ConsejoReinstalarBinario => {
            "ejecuta de nuevo el alta o «gitmereba doctor» tras reinstalar".to_string()
        }
        TextoDoctor::BinarioHashCorrecto { version } => format!("SHA-256 correcto ({version})"),
        TextoDoctor::BinarioHashDistinto => "el SHA-256 no coincide con el esperado".to_string(),
        TextoDoctor::ConsejoBorrarBinario => {
            "borra el binario y deja que la app lo vuelva a descargar y verificar".to_string()
        }
        TextoDoctor::BinarioHashError { error } => {
            format!("no se pudo calcular su SHA-256: {error}", error = error.es)
        }
        TextoDoctor::ConsejoRevisarUrl => "revisa la URL de la cuenta".to_string(),
        TextoDoctor::GiteaResponde => "responde".to_string(),
        TextoDoctor::GiteaNoResponde => "no responde".to_string(),
        TextoDoctor::ConsejoArrancarGitea => {
            "arráncalo con «systemctl --user start» o revisa el servicio".to_string()
        }
        TextoDoctor::ConsejoRevisarServicioGitea => "revisa el servicio de Gitea".to_string(),
        TextoDoctor::ConsejoRevisarLlavero => "revisa el llavero del sistema".to_string(),
        TextoDoctor::SecretosPresentes => "los tres secretos están presentes".to_string(),
        TextoDoctor::SecretosFaltan { faltan } => {
            format!("faltan en el llavero: {}", faltan.join(", "))
        }
        TextoDoctor::ConsejoRegenerarSecretos => "repite el alta para regenerarlos".to_string(),
        TextoDoctor::SnapshotsResumen { total, protegidas } => {
            let capturas = if *total == 1 { "captura" } else { "capturas" };
            let protegida = if *protegidas == 1 {
                "protegida"
            } else {
                "protegidas"
            };
            format!("{total} {capturas}, {protegidas} {protegida}")
        }
        TextoDoctor::ConsejoCapturasProtegidas => {
            "hay capturas protegidas por un cambio destructivo detectado en el \
             origen (historia reescrita, rama o tag borrado): revísalas antes de \
             que la retención normal pueda alcanzarlas"
                .to_string()
        }
        TextoDoctor::SnapshotsError { error } => {
            format!(
                "no se pudieron listar los snapshots: {error}",
                error = error.es
            )
        }
        TextoDoctor::ConsejoPermisosSnapshots => {
            "comprueba los permisos de la carpeta «snapshots/» de la cuenta".to_string()
        }
        TextoDoctor::TemporizadorNoInstalado => {
            "no hay temporizador de sincronización instalado".to_string()
        }
        TextoDoctor::ConsejoAbrirVentanaInstala => {
            "abre la ventana de gitmereba una vez: lo instala sola; hasta entonces solo se \
             sincroniza a mano"
                .to_string()
        }
        TextoDoctor::TemporizadorSinExecStart { ruta } => {
            format!("«{}» no tiene un ExecStart reconocible", ruta.display())
        }
        TextoDoctor::ConsejoAbrirVentanaReescribe => {
            "abre la ventana de gitmereba una vez: reescribe la unidad".to_string()
        }
        TextoDoctor::TemporizadorSincroniza { ejecutable } => {
            format!("sincroniza con «{ejecutable}»")
        }
        TextoDoctor::TemporizadorEjecutablePerdido { ejecutable } => {
            format!("el temporizador apunta a «{ejecutable}», que ya no existe o no es ejecutable")
        }
        TextoDoctor::ConsejoActualizarTemporizador => {
            "abre la ventana de gitmereba una vez (o guarda Ajustes): el temporizador pasa a \
             usar el ejecutable actual"
                .to_string()
        }
        TextoDoctor::IdiomaPreferencia { idioma, ruta } => {
            format!("{} (preferencia, {})", idioma.codigo(), ruta.display())
        }
        TextoDoctor::IdiomaVariable {
            idioma,
            variable,
            valor,
        } => format!("{} (de {variable}={valor})", idioma.codigo()),
        TextoDoctor::IdiomaPorDefecto { idioma } => format!(
            "{} (por defecto: no hay LC_ALL, LC_MESSAGES ni LANG)",
            idioma.codigo()
        ),
        TextoDoctor::ConsejoFijarIdioma => "fija el idioma en Ajustes".to_string(),
        TextoDoctor::IdiomaPreferenciaNoValida { idioma, ruta } => format!(
            "no se pudo interpretar «{}»; idioma en uso: {}",
            ruta.display(),
            idioma.codigo()
        ),
        TextoDoctor::TemporizadorIdiomaSinVariables => "el temporizador no tiene LC_ALL, \
             LC_MESSAGES ni LANG: sus avisos saldrán en inglés"
            .to_string(),
        TextoDoctor::TemporizadorIdiomaDistinto {
            temporizador,
            sesion,
        } => format!(
            "el temporizador usa el idioma {} y esta sesión {}",
            temporizador.codigo(),
            sesion.codigo()
        ),
        TextoDoctor::TemporizadorIdiomaCoincide { idioma } => {
            format!(
                "el temporizador usa el mismo idioma que la sesión ({})",
                idioma.codigo()
            )
        }
        TextoDoctor::TemporizadorIdiomaNoDisponible => {
            "no se pudo consultar el entorno del temporizador (systemctl no disponible o \
             sin sesión de usuario)"
                .to_string()
        }
        TextoDoctor::ConsejoCorregirPreferencias => {
            "corrige o borra el fichero, o fija el idioma en Ajustes".to_string()
        }
    }
}
