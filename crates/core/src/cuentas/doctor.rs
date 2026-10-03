//! `gitmereba doctor`: comprobaciones de salud del sistema y de cada cuenta.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::almacen::VerificacionAuditoria;
use crate::config::RutasCuenta;
use crate::git;
use crate::gitea::ApiGitea;
use crate::idioma::{self, LecturaPreferencia};
use crate::instancia::{
    SHA256_GITEA_1_27_3_LINUX_AMD64, VERSION_GITEA, entorno_gestor_systemd, nombre_servicio_sync,
};
use crate::modelo::{Cuenta, Nombre};
use crate::secretos::{ClaveSecreto, Llavero, Secreto};
use crate::snapshots;

use super::contexto::Contexto;
use super::doctor_textos::{
    MotivoAppIni, NombreComprobacion, ParteCuenta, TextoDoctor, TextoExterno,
};
use super::listar::listar;

const RUTA_GPGV: &str = "/usr/bin/gpgv";
/// Fichero de AppArmor que, a `1`, indica que los servicios `systemd --user` no pueden
/// aislar el sistema de ficheros (comprobación «aislamiento-systemd»).
const RUTA_APPARMOR_RESTRICT_USERNS: &str =
    "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";

/// Resultado de una comprobación de [`doctor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NivelComprobacion {
    Ok,
    Aviso,
    Fallo,
}

/// Una comprobación de `doctor`, con un consejo cuando no está todo bien. Los textos son
/// tipados: quien los muestra los traduce con [`crate::idioma::Localizable`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comprobacion {
    pub nombre: NombreComprobacion,
    pub nivel: NivelComprobacion,
    pub mensaje: TextoDoctor,
    pub consejo: Option<TextoDoctor>,
}

impl Comprobacion {
    fn ok(nombre: NombreComprobacion, mensaje: TextoDoctor) -> Self {
        Self {
            nombre,
            nivel: NivelComprobacion::Ok,
            mensaje,
            consejo: None,
        }
    }

    fn aviso(nombre: NombreComprobacion, mensaje: TextoDoctor, consejo: TextoDoctor) -> Self {
        Self {
            nombre,
            nivel: NivelComprobacion::Aviso,
            mensaje,
            consejo: Some(consejo),
        }
    }

    fn fallo(nombre: NombreComprobacion, mensaje: TextoDoctor, consejo: TextoDoctor) -> Self {
        Self {
            nombre,
            nivel: NivelComprobacion::Fallo,
            mensaje,
            consejo: Some(consejo),
        }
    }
}

fn nombre_cuenta(login: &Nombre, parte: ParteCuenta) -> NombreComprobacion {
    NombreComprobacion::Cuenta {
        login: login.to_string(),
        parte,
    }
}

/// Todas las comprobaciones de una ejecución de `doctor`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InformeDoctor {
    pub comprobaciones: Vec<Comprobacion>,
}

impl InformeDoctor {
    /// `true` si ninguna comprobación ha fallado (los avisos no cuentan).
    pub fn ok(&self) -> bool {
        !self
            .comprobaciones
            .iter()
            .any(|c| c.nivel == NivelComprobacion::Fallo)
    }
}

/// Ejecuta todas las comprobaciones de `doctor`. Nunca falla: cualquier problema se
/// convierte en una [`Comprobacion`] con nivel `Fallo` o `Aviso`.
pub async fn doctor<L: Llavero>(contexto: &Contexto<'_, L>) -> InformeDoctor {
    let mut comprobaciones = Vec::new();

    comprobaciones.push(comprobar_git().await);
    comprobaciones.push(comprobar_gpgv());
    comprobaciones.push(comprobar_llavero(contexto.llavero));
    comprobaciones.push(comprobar_directorio_datos(
        contexto.rutas.directorio_datos(),
    ));
    comprobaciones.push(comprobar_auditoria(contexto.almacen));
    comprobaciones.push(comprobar_aislamiento_systemd(leer_apparmor_restrict_userns));
    comprobaciones.push(comprobar_cortafuegos(existe_ufw));
    comprobaciones.push(comprobar_idioma(
        idioma::leer_preferencia(contexto.rutas),
        &contexto.rutas.fichero_preferencias(),
        &|nombre| std::env::var(nombre).ok(),
    ));

    let entorno_systemd = entorno_gestor_systemd().await.map(|s| parsear_entorno(&s));
    if let Some(c) = comprobar_idioma_temporizador(
        idioma::leer_preferencia(contexto.rutas),
        &|nombre| std::env::var(nombre).ok(),
        entorno_systemd.as_ref(),
    ) {
        comprobaciones.push(c);
    }

    let directorio_bin = contexto.rutas.directorio_bin();
    match listar(contexto) {
        Ok(cuentas) => {
            for cuenta in cuentas {
                comprobaciones
                    .extend(comprobar_cuenta(&directorio_bin, contexto.llavero, &cuenta).await);
                comprobaciones.push(comprobar_temporizador(
                    contexto.rutas.directorio_systemd_usuario(),
                    &cuenta.login,
                ));
            }
        }
        Err(error) => comprobaciones.push(Comprobacion::fallo(
            NombreComprobacion::Cuentas,
            TextoDoctor::CuentasIndiceIlegible {
                error: TextoExterno::de(&error),
            },
            TextoDoctor::ConsejoPermisosIndice,
        )),
    }

    InformeDoctor { comprobaciones }
}

/// Idioma en uso y de dónde sale. Informativa, salvo dos avisos: `preferencias.toml`
/// existe pero no se pudo interpretar, o la preferencia es `auto` y el entorno no define
/// `LC_ALL`, `LC_MESSAGES` ni `LANG` (se usa inglés). `entorno` es un parámetro para poder
/// probarla sin el entorno real del proceso.
pub fn comprobar_idioma(
    lectura: LecturaPreferencia,
    ruta_preferencias: &Path,
    entorno: &dyn Fn(&str) -> Option<String>,
) -> Comprobacion {
    let nombre = NombreComprobacion::Idioma;
    let preferencia = lectura.preferencia();
    let idioma = idioma::resolver(preferencia, entorno);
    if lectura == LecturaPreferencia::NoValida {
        return Comprobacion::aviso(
            nombre,
            TextoDoctor::IdiomaPreferenciaNoValida {
                idioma,
                ruta: ruta_preferencias.to_path_buf(),
            },
            TextoDoctor::ConsejoCorregirPreferencias,
        );
    }
    match idioma::origen(preferencia, entorno) {
        idioma::OrigenIdioma::Preferencia => Comprobacion::ok(
            nombre,
            TextoDoctor::IdiomaPreferencia {
                idioma,
                ruta: ruta_preferencias.to_path_buf(),
            },
        ),
        idioma::OrigenIdioma::Variable {
            nombre: variable,
            valor,
        } => Comprobacion::ok(
            nombre,
            TextoDoctor::IdiomaVariable {
                idioma,
                variable: variable.to_string(),
                valor,
            },
        ),
        idioma::OrigenIdioma::PorDefecto => Comprobacion::aviso(
            nombre,
            TextoDoctor::IdiomaPorDefecto { idioma },
            TextoDoctor::ConsejoFijarIdioma,
        ),
    }
}

/// Interpreta la salida de `systemctl --user show-environment` (líneas `CLAVE=valor`).
pub fn parsear_entorno(salida: &str) -> HashMap<String, String> {
    salida
        .lines()
        .filter_map(|linea| linea.split_once('='))
        .filter(|(clave, _)| !clave.is_empty())
        .map(|(clave, valor)| (clave.to_string(), valor.to_string()))
        .collect()
}

/// Idioma que verá el temporizador (que hereda el entorno de `systemd --user`, no el de
/// la sesión). Solo aplica con preferencia `auto`; con idioma fijado no devuelve nada.
/// Aviso si ese entorno no define `LC_ALL`/`LC_MESSAGES`/`LANG` o da un idioma distinto
/// del de la sesión; informativa si no se pudo consultar (`entorno_systemd` a `None`).
pub fn comprobar_idioma_temporizador(
    lectura: LecturaPreferencia,
    entorno_sesion: &dyn Fn(&str) -> Option<String>,
    entorno_systemd: Option<&HashMap<String, String>>,
) -> Option<Comprobacion> {
    if lectura.preferencia() != idioma::Preferencia::Auto {
        return None;
    }
    let nombre = NombreComprobacion::IdiomaTemporizador;
    let Some(entorno_systemd) = entorno_systemd else {
        return Some(Comprobacion::ok(
            nombre,
            TextoDoctor::TemporizadorIdiomaNoDisponible,
        ));
    };
    let del_gestor = |clave: &str| entorno_systemd.get(clave).cloned();
    if matches!(
        idioma::origen(idioma::Preferencia::Auto, &del_gestor),
        idioma::OrigenIdioma::PorDefecto
    ) {
        return Some(Comprobacion::aviso(
            nombre,
            TextoDoctor::TemporizadorIdiomaSinVariables,
            TextoDoctor::ConsejoFijarIdioma,
        ));
    }
    let temporizador = idioma::resolver(idioma::Preferencia::Auto, &del_gestor);
    let sesion = idioma::resolver(idioma::Preferencia::Auto, entorno_sesion);
    Some(if temporizador == sesion {
        Comprobacion::ok(
            nombre,
            TextoDoctor::TemporizadorIdiomaCoincide { idioma: sesion },
        )
    } else {
        Comprobacion::aviso(
            nombre,
            TextoDoctor::TemporizadorIdiomaDistinto {
                temporizador,
                sesion,
            },
            TextoDoctor::ConsejoFijarIdioma,
        )
    })
}

async fn comprobar_git() -> Comprobacion {
    match git::version().await {
        Ok(version) => {
            Comprobacion::ok(NombreComprobacion::Git, TextoDoctor::GitVersion { version })
        }
        Err(error) => Comprobacion::fallo(
            NombreComprobacion::Git,
            TextoDoctor::GitNoDisponible {
                error: TextoExterno::de(&error),
            },
            TextoDoctor::ConsejoInstalarGit,
        ),
    }
}

fn comprobar_gpgv() -> Comprobacion {
    let ruta = Path::new(RUTA_GPGV).to_path_buf();
    if ruta.exists() {
        Comprobacion::ok(
            NombreComprobacion::Gpgv,
            TextoDoctor::GpgvDisponible { ruta },
        )
    } else {
        Comprobacion::fallo(
            NombreComprobacion::Gpgv,
            TextoDoctor::GpgvNoEncontrado { ruta },
            TextoDoctor::ConsejoInstalarGnupg,
        )
    }
}

fn comprobar_llavero<L: Llavero>(llavero: &L) -> Comprobacion {
    let nombre = NombreComprobacion::Llavero;
    let sonda = match Nombre::nuevo("gitmereba-doctor-sonda") {
        Ok(nombre_sonda) => nombre_sonda,
        Err(error) => {
            return Comprobacion::fallo(
                nombre,
                TextoDoctor::LlaveroErrorInterno {
                    error: TextoExterno::de(&error),
                },
                TextoDoctor::ConsejoRepetirComprobacion,
            );
        }
    };
    let resultado = llavero
        .guardar(&sonda, ClaveSecreto::TokenGithub, &Secreto::nuevo("sonda"))
        .and_then(|()| llavero.borrar(&sonda, ClaveSecreto::TokenGithub));
    match resultado {
        Ok(()) => Comprobacion::ok(nombre, TextoDoctor::LlaveroAccesible),
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::LlaveroNoAccesible {
                error: TextoExterno::de(&error),
            },
            TextoDoctor::ConsejoSecretService,
        ),
    }
}

fn comprobar_directorio_datos(directorio: &Path) -> Comprobacion {
    let nombre = NombreComprobacion::DirectorioDatos;
    if !directorio.exists() {
        return Comprobacion::aviso(
            nombre,
            TextoDoctor::DirectorioDatosNoExiste,
            TextoDoctor::ConsejoSeCreaConCuentaAdd,
        );
    }
    match permisos_de(directorio) {
        Ok(0o700) => Comprobacion::ok(nombre, TextoDoctor::PermisosCorrectos0700),
        Ok(modo) => Comprobacion::fallo(
            nombre,
            TextoDoctor::PermisosIncorrectos0700 { modo },
            TextoDoctor::ConsejoChmod700 {
                ruta: directorio.to_path_buf(),
            },
        ),
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::PermisosNoLegibles {
                error: TextoExterno::literal(error.to_string()),
            },
            TextoDoctor::ConsejoDirectorioAccesible,
        ),
    }
}

/// Lee `RUTA_APPARMOR_RESTRICT_USERNS` tal cual está en este sistema; `None` si no
/// existe o no se puede leer (interpretado como «AppArmor no lo restringe»).
fn leer_apparmor_restrict_userns() -> Option<String> {
    std::fs::read_to_string(RUTA_APPARMOR_RESTRICT_USERNS).ok()
}

/// Comprueba si AppArmor impide a `systemd --user` aislar el sistema de ficheros.
/// No ejecuta systemd: solo lee el fichero de configuración de
/// AppArmor a través de `leer`, un parámetro para poder probar los tres casos (vale `1`,
/// vale `0`, no existe) sin depender de la máquina que ejecuta los tests.
fn comprobar_aislamiento_systemd(leer: impl Fn() -> Option<String>) -> Comprobacion {
    let nombre = NombreComprobacion::AislamientoSystemd;
    match leer().as_deref().map(str::trim) {
        Some("1") => Comprobacion::aviso(
            nombre,
            TextoDoctor::AislamientoAviso,
            TextoDoctor::ConsejoAislamiento,
        ),
        _ => Comprobacion::ok(nombre, TextoDoctor::AislamientoOk),
    }
}

/// Rutas habituales del binario `ufw` en Debian/Ubuntu.
const RUTAS_UFW: [&str; 2] = ["/usr/sbin/ufw", "/usr/bin/ufw"];

fn existe_ufw() -> bool {
    RUTAS_UFW.iter().any(|ruta| Path::new(ruta).exists())
}

/// Comprobación informativa de cortafuegos: exponer una cuenta a la LAN pone
/// Gitea a escuchar en `0.0.0.0`, así que conviene un cortafuegos que limite quién
/// llega al puerto. Sin privilegios no se puede confirmar de forma fiable que `ufw`
/// esté activo (`ufw status` normalmente exige root), así que basta con detectar el
/// binario y dar la sugerencia (nunca es un `Fallo`: es solo un recordatorio).
fn comprobar_cortafuegos(existe: impl Fn() -> bool) -> Comprobacion {
    let nombre = NombreComprobacion::Cortafuegos;
    if existe() {
        Comprobacion::aviso(
            nombre,
            TextoDoctor::UfwInstalado,
            TextoDoctor::ConsejoActivarUfw,
        )
    } else {
        Comprobacion::aviso(
            nombre,
            TextoDoctor::UfwNoEncontrado,
            TextoDoctor::ConsejoInstalarUfw,
        )
    }
}

fn comprobar_auditoria(almacen: &crate::almacen::Almacen) -> Comprobacion {
    let nombre = NombreComprobacion::Auditoria;
    match almacen.verificar_auditoria() {
        Ok(VerificacionAuditoria::Integra) => {
            Comprobacion::ok(nombre, TextoDoctor::AuditoriaIntegra)
        }
        Ok(VerificacionAuditoria::Rota { id }) => Comprobacion::fallo(
            nombre,
            TextoDoctor::AuditoriaRota { id },
            TextoDoctor::ConsejoAuditoriaManipulada,
        ),
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::AuditoriaNoVerificable {
                error: TextoExterno::de(&error),
            },
            TextoDoctor::ConsejoAccesoAlmacen,
        ),
    }
}

async fn comprobar_cuenta<L: Llavero>(
    directorio_bin: &Path,
    llavero: &L,
    cuenta: &Cuenta,
) -> Vec<Comprobacion> {
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let mut comprobaciones = Vec::new();

    comprobaciones.push(comprobar_carpeta_cuenta(
        &cuenta.login,
        rutas_cuenta.carpeta(),
    ));
    comprobaciones.push(comprobar_app_ini(cuenta, &rutas_cuenta));
    comprobaciones.push(comprobar_binario_en(&cuenta.login, directorio_bin).await);
    comprobaciones.push(comprobar_gitea_responde(cuenta, llavero).await);
    comprobaciones.push(comprobar_secretos(llavero, &cuenta.login));
    comprobaciones.push(comprobar_snapshots(&cuenta.login, &rutas_cuenta));

    comprobaciones
}

/// Ejecutable del `ExecStart=` de una unidad, deshaciendo el citado de systemd que aplica
/// `instancia::temporizador` a las rutas con espacios.
fn ejecutable_de_exec_start(unidad: &str) -> Option<String> {
    let valor = unidad
        .lines()
        .find_map(|linea| linea.trim().strip_prefix("ExecStart="))?;
    let Some(citado) = valor.strip_prefix('"') else {
        return valor.split_whitespace().next().map(str::to_string);
    };
    let mut ruta = String::new();
    let mut caracteres = citado.chars();
    while let Some(caracter) = caracteres.next() {
        match caracter {
            '"' => return Some(ruta),
            '\\' => ruta.push(caracteres.next()?),
            otro => ruta.push(otro),
        }
    }
    None
}

/// La sincronización periódica depende de una unidad de usuario cuyo `ExecStart` lleva la
/// ruta del ejecutable que la instaló. Si ese ejecutable ya no está (se desinstaló el
/// paquete, se limpió `target/`), el temporizador falla en silencio cada vez que salta.
fn comprobar_temporizador(directorio_systemd: &Path, login: &Nombre) -> Comprobacion {
    let nombre = nombre_cuenta(login, ParteCuenta::Temporizador);
    let ruta_unidad = directorio_systemd.join(nombre_servicio_sync(login));
    let Ok(unidad) = std::fs::read_to_string(&ruta_unidad) else {
        return Comprobacion::aviso(
            nombre,
            TextoDoctor::TemporizadorNoInstalado,
            TextoDoctor::ConsejoAbrirVentanaInstala,
        );
    };
    let Some(ejecutable) = ejecutable_de_exec_start(&unidad) else {
        return Comprobacion::fallo(
            nombre,
            TextoDoctor::TemporizadorSinExecStart { ruta: ruta_unidad },
            TextoDoctor::ConsejoAbrirVentanaReescribe,
        );
    };
    let es_ejecutable = std::fs::metadata(&ejecutable)
        .is_ok_and(|datos| datos.is_file() && datos.permissions().mode() & 0o111 != 0);
    if es_ejecutable {
        Comprobacion::ok(nombre, TextoDoctor::TemporizadorSincroniza { ejecutable })
    } else {
        Comprobacion::fallo(
            nombre,
            TextoDoctor::TemporizadorEjecutablePerdido { ejecutable },
            TextoDoctor::ConsejoActualizarTemporizador,
        )
    }
}

/// Cuántas capturas hay y cuántas de ellas están protegidas (pendientes de revisar: una
/// captura solo se protege cuando se ha detectado un cambio destructivo frente a ella,
/// ver `cuentas::proteccion`). Aviso si hay alguna protegida.
fn comprobar_snapshots(login: &Nombre, rutas_cuenta: &RutasCuenta) -> Comprobacion {
    let nombre = nombre_cuenta(login, ParteCuenta::Snapshots);
    match snapshots::listar_cuenta(rutas_cuenta) {
        Ok(capturas) => {
            let protegidas = capturas.iter().filter(|c| c.protegida).count();
            let mensaje = TextoDoctor::SnapshotsResumen {
                total: capturas.len(),
                protegidas,
            };
            if protegidas > 0 {
                Comprobacion::aviso(nombre, mensaje, TextoDoctor::ConsejoCapturasProtegidas)
            } else {
                Comprobacion::ok(nombre, mensaje)
            }
        }
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::SnapshotsError {
                error: TextoExterno::de(&error),
            },
            TextoDoctor::ConsejoPermisosSnapshots,
        ),
    }
}

fn comprobar_carpeta_cuenta(login: &Nombre, carpeta: &Path) -> Comprobacion {
    let nombre = nombre_cuenta(login, ParteCuenta::Carpeta);
    if !carpeta.exists() {
        return Comprobacion::fallo(
            nombre,
            TextoDoctor::CarpetaNoExiste,
            TextoDoctor::ConsejoRepetirAltaORestaurar,
        );
    }
    match permisos_de(carpeta) {
        Ok(0o700) => Comprobacion::ok(nombre, TextoDoctor::CarpetaExisteCon0700),
        Ok(modo) => Comprobacion::fallo(
            nombre,
            TextoDoctor::PermisosIncorrectos0700 { modo },
            TextoDoctor::ConsejoChmod700 {
                ruta: carpeta.to_path_buf(),
            },
        ),
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::ErrorSistema {
                error: TextoExterno::literal(error.to_string()),
            },
            TextoDoctor::ConsejoRevisarPermisosAMano,
        ),
    }
}

/// Comprueba `app.ini` (dos perfiles válidos según `cuenta.lan`).
///
/// Sin acceso LAN configurado, exige exactamente lo de siempre: 0600 y
/// `HTTP_ADDR = 127.0.0.1`; `0.0.0.0` sin `lan` en `gitmereba.toml` es un **error**
/// (Gitea escucharía en la red sin que la app lo sepa). Con acceso LAN activo, exige
/// `PROTOCOL = https`, el certificado presente y su clave con permisos 0600, y lo
/// informa como un **aviso** (no un error): «expuesto a la LAN por HTTPS».
fn comprobar_app_ini(cuenta: &Cuenta, rutas_cuenta: &RutasCuenta) -> Comprobacion {
    let nombre = nombre_cuenta(&cuenta.login, ParteCuenta::AppIni);
    let app_ini = rutas_cuenta.gitea_app_ini();
    if !app_ini.exists() {
        return Comprobacion::fallo(
            nombre,
            TextoDoctor::AppIniNoExiste,
            TextoDoctor::ConsejoRepetirAltaProvision,
        );
    }
    let modo = match permisos_de(&app_ini) {
        Ok(modo) => modo,
        Err(error) => {
            return Comprobacion::fallo(
                nombre,
                TextoDoctor::ErrorSistema {
                    error: TextoExterno::literal(error.to_string()),
                },
                TextoDoctor::ConsejoRevisarPermisosAMano,
            );
        }
    };
    let contenido = match std::fs::read_to_string(&app_ini) {
        Ok(contenido) => contenido,
        Err(error) => {
            return Comprobacion::fallo(
                nombre,
                TextoDoctor::ErrorSistema {
                    error: TextoExterno::literal(error.to_string()),
                },
                TextoDoctor::ConsejoRevisarFicheroLegible,
            );
        }
    };
    let permisos_ok = modo == 0o600;

    match &cuenta.lan {
        None => comprobar_app_ini_sin_lan(nombre, permisos_ok, modo, &contenido),
        Some(acceso) => comprobar_app_ini_con_lan(
            nombre,
            permisos_ok,
            modo,
            &contenido,
            &acceso.host,
            rutas_cuenta,
        ),
    }
}

fn comprobar_app_ini_sin_lan(
    nombre: NombreComprobacion,
    permisos_ok: bool,
    modo: u32,
    contenido: &str,
) -> Comprobacion {
    let escucha_local = contenido.contains("HTTP_ADDR = 127.0.0.1");

    if permisos_ok && escucha_local {
        Comprobacion::ok(nombre, TextoDoctor::AppIniSinLanCorrecto)
    } else {
        let mut motivos = Vec::new();
        if !permisos_ok {
            motivos.push(MotivoAppIni::Permisos { modo });
        }
        if !escucha_local {
            motivos.push(MotivoAppIni::SinHttpAddrLocal);
        }
        Comprobacion::fallo(
            nombre,
            TextoDoctor::AppIniMotivos { motivos },
            TextoDoctor::ConsejoAppIniSinLan,
        )
    }
}

fn comprobar_app_ini_con_lan(
    nombre: NombreComprobacion,
    permisos_ok: bool,
    modo: u32,
    contenido: &str,
    host: &crate::modelo::NombreHostInterno,
    rutas_cuenta: &RutasCuenta,
) -> Comprobacion {
    let https_ok = contenido.contains("PROTOCOL = https");
    let certificado_ok = rutas_cuenta.gitea_tls_cert().exists();
    let clave_ok = matches!(permisos_de(&rutas_cuenta.gitea_tls_key()), Ok(0o600));

    if permisos_ok && https_ok && certificado_ok && clave_ok {
        Comprobacion::aviso(
            nombre,
            TextoDoctor::AppIniExpuestoLan {
                host: host.to_string(),
            },
            TextoDoctor::ConsejoCortafuegosLan,
        )
    } else {
        let mut motivos = Vec::new();
        if !permisos_ok {
            motivos.push(MotivoAppIni::Permisos { modo });
        }
        if !https_ok {
            motivos.push(MotivoAppIni::SinHttps);
        }
        if !certificado_ok {
            motivos.push(MotivoAppIni::SinCertificado);
        }
        if !clave_ok {
            motivos.push(MotivoAppIni::ClaveSinPermisos);
        }
        Comprobacion::fallo(
            nombre,
            TextoDoctor::AppIniMotivos { motivos },
            TextoDoctor::ConsejoRegenerarLan,
        )
    }
}

/// Comprueba el binario de Gitea de `directorio_bin` (compartido entre cuentas) contra el SHA-256 fijado en el código. Se repite por cuenta para que
/// cada tarjeta de diagnóstico esté completa.
async fn comprobar_binario_en(login: &Nombre, directorio_bin: &Path) -> Comprobacion {
    let nombre = nombre_cuenta(login, ParteCuenta::BinarioGitea);
    let ruta = directorio_bin.join(format!("gitea-{VERSION_GITEA}"));
    if !ruta.exists() {
        return Comprobacion::fallo(
            nombre,
            TextoDoctor::BinarioNoEncontrado { ruta },
            TextoDoctor::ConsejoReinstalarBinario,
        );
    }
    match sha256_de(&ruta).await {
        Ok(hash) if hash == SHA256_GITEA_1_27_3_LINUX_AMD64 => Comprobacion::ok(
            nombre,
            TextoDoctor::BinarioHashCorrecto {
                version: VERSION_GITEA.to_string(),
            },
        ),
        Ok(_) => Comprobacion::fallo(
            nombre,
            TextoDoctor::BinarioHashDistinto,
            TextoDoctor::ConsejoBorrarBinario,
        ),
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::BinarioHashError {
                error: TextoExterno::literal(error.to_string()),
            },
            TextoDoctor::ConsejoFicheroLegible,
        ),
    }
}

async fn sha256_de(ruta: &Path) -> std::io::Result<String> {
    let contenido = tokio::fs::read(ruta).await?;
    let mut hasher = Sha256::new();
    hasher.update(&contenido);
    let hash = hasher.finalize();
    let mut texto = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write;
        let _ = write!(texto, "{byte:02x}");
    }
    Ok(texto)
}

async fn comprobar_gitea_responde<L: Llavero>(cuenta: &Cuenta, llavero: &L) -> Comprobacion {
    let nombre = nombre_cuenta(&cuenta.login, ParteCuenta::Gitea);
    let token = llavero
        .leer(&cuenta.login, ClaveSecreto::TokenGitea)
        .ok()
        .flatten()
        .unwrap_or_else(|| Secreto::nuevo(""));
    let cliente = match super::comun::cliente_gitea_de_cuenta(cuenta, token) {
        Ok(cliente) => cliente,
        Err(error) => {
            return Comprobacion::fallo(
                nombre,
                TextoDoctor::ErrorSistema {
                    error: TextoExterno::de(&error),
                },
                TextoDoctor::ConsejoRevisarUrl,
            );
        }
    };
    match cliente.salud().await {
        Ok(true) => Comprobacion::ok(nombre, TextoDoctor::GiteaResponde),
        Ok(false) => Comprobacion::aviso(
            nombre,
            TextoDoctor::GiteaNoResponde,
            TextoDoctor::ConsejoArrancarGitea,
        ),
        Err(error) => Comprobacion::fallo(
            nombre,
            TextoDoctor::ErrorSistema {
                error: TextoExterno::de(&error),
            },
            TextoDoctor::ConsejoRevisarServicioGitea,
        ),
    }
}

fn comprobar_secretos<L: Llavero>(llavero: &L, login: &Nombre) -> Comprobacion {
    let nombre = nombre_cuenta(login, ParteCuenta::Secretos);
    let claves = [
        ClaveSecreto::TokenGithub,
        ClaveSecreto::PasswordAdminGitea,
        ClaveSecreto::TokenGitea,
    ];
    let mut faltan = Vec::new();
    for clave in claves {
        match llavero.leer(login, clave) {
            Ok(Some(secreto)) if !secreto.esta_vacio() => {}
            Ok(_) => faltan.push(clave.etiqueta().to_string()),
            Err(error) => {
                return Comprobacion::fallo(
                    nombre,
                    TextoDoctor::ErrorSistema {
                        error: TextoExterno::de(&error),
                    },
                    TextoDoctor::ConsejoRevisarLlavero,
                );
            }
        }
    }
    if faltan.is_empty() {
        Comprobacion::ok(nombre, TextoDoctor::SecretosPresentes)
    } else {
        Comprobacion::fallo(
            nombre,
            TextoDoctor::SecretosFaltan { faltan },
            TextoDoctor::ConsejoRegenerarSecretos,
        )
    }
}

fn permisos_de(ruta: &Path) -> std::io::Result<u32> {
    Ok(std::fs::metadata(ruta)?.permissions().mode() & 0o777)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::config::{self, Rutas, RutasCuenta};
    use crate::idioma::{Idioma, Localizable, Preferencia};
    use crate::modelo::Alcance;
    use crate::secretos::LlaveroEnMemoria;
    use std::os::unix::fs::PermissionsExt;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    #[tokio::test]
    async fn detecta_permisos_0755_y_http_addr_0_0_0_0() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let carpeta = temporal.path().join("cuenta-jparga");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);

        let cuenta = Cuenta {
            login: nombre("jparga"),
            carpeta: carpeta.clone(),
            puerto: 33077,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        };
        config::escribir_cuenta(&rutas_cuenta, &cuenta).expect("escribir gitmereba.toml");
        // `escribir_cuenta` deja la carpeta en 0700: la forzamos a 0755 para la prueba.
        std::fs::set_permissions(&carpeta, std::fs::Permissions::from_mode(0o755))
            .expect("forzar permisos 0755");

        std::fs::create_dir_all(rutas_cuenta.gitea_app_ini().parent().unwrap())
            .expect("crear conf/");
        std::fs::write(
            rutas_cuenta.gitea_app_ini(),
            "[server]\nHTTP_ADDR = 0.0.0.0\n",
        )
        .expect("escribir app.ini de prueba");
        std::fs::set_permissions(
            rutas_cuenta.gitea_app_ini(),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("permisos del app.ini");

        let mut indice = config::leer_indice_cuentas(&rutas).expect("leer índice");
        indice.cuentas.insert(
            "jparga".to_string(),
            config::EntradaCuenta {
                carpeta: carpeta.clone(),
                puerto: 33077,
            },
        );
        config::escribir_indice_cuentas(&rutas, &indice).expect("escribir índice");

        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        let informe = doctor(&contexto).await;

        let carpeta_check = informe
            .comprobaciones
            .iter()
            .find(|c| c.nombre == nombre_cuenta(&nombre("jparga"), ParteCuenta::Carpeta))
            .expect("existe la comprobación de carpeta");
        assert_eq!(carpeta_check.nivel, NivelComprobacion::Fallo);

        let app_ini_check = informe
            .comprobaciones
            .iter()
            .find(|c| c.nombre == nombre_cuenta(&nombre("jparga"), ParteCuenta::AppIni))
            .expect("existe la comprobación de app.ini");
        assert_eq!(app_ini_check.nivel, NivelComprobacion::Fallo);
        assert!(
            app_ini_check
                .mensaje
                .localizar(Idioma::Es)
                .contains("127.0.0.1")
        );
    }

    #[tokio::test]
    async fn sin_cuentas_las_comprobaciones_globales_funcionan() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        let informe = doctor(&contexto).await;

        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == NombreComprobacion::Git)
        );
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == NombreComprobacion::Llavero)
        );
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == NombreComprobacion::Auditoria)
        );
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == NombreComprobacion::AislamientoSystemd)
        );
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == NombreComprobacion::Cortafuegos)
        );
    }

    // --- acceso LAN ---------------------------------------------------

    fn cuenta_lan_de_prueba(carpeta: &std::path::Path) -> Cuenta {
        Cuenta {
            login: nombre("jparga"),
            carpeta: carpeta.to_path_buf(),
            puerto: 33078,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: Some(crate::modelo::AccesoLan {
                host: crate::modelo::NombreHostInterno::nuevo("jparga.gitmereba.internal")
                    .expect("host válido"),
            }),
        }
    }

    #[test]
    fn app_ini_con_lan_completo_y_correcto_es_un_aviso_no_un_error() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let carpeta = temporal.path().join("cuenta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        let cuenta = cuenta_lan_de_prueba(&carpeta);

        std::fs::create_dir_all(rutas_cuenta.gitea_app_ini().parent().unwrap())
            .expect("crear conf/");
        std::fs::write(rutas_cuenta.gitea_app_ini(), "[server]\nPROTOCOL = https\n")
            .expect("escribir app.ini de prueba");
        std::fs::set_permissions(
            rutas_cuenta.gitea_app_ini(),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("permisos del app.ini");

        std::fs::create_dir_all(rutas_cuenta.gitea_tls()).expect("crear tls/");
        std::fs::write(rutas_cuenta.gitea_tls_cert(), "cert").expect("escribir certificado");
        std::fs::write(rutas_cuenta.gitea_tls_key(), "clave").expect("escribir clave");
        std::fs::set_permissions(
            rutas_cuenta.gitea_tls_key(),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("permisos de la clave");

        let comprobacion = comprobar_app_ini(&cuenta, &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(comprobacion.mensaje.localizar(Idioma::Es).contains("LAN"));
    }

    #[test]
    fn app_ini_con_lan_pero_sin_certificado_es_un_fallo() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let carpeta = temporal.path().join("cuenta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        let cuenta = cuenta_lan_de_prueba(&carpeta);

        std::fs::create_dir_all(rutas_cuenta.gitea_app_ini().parent().unwrap())
            .expect("crear conf/");
        std::fs::write(rutas_cuenta.gitea_app_ini(), "[server]\nPROTOCOL = https\n")
            .expect("escribir app.ini de prueba");
        std::fs::set_permissions(
            rutas_cuenta.gitea_app_ini(),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("permisos del app.ini");
        // Sin generar el certificado.

        let comprobacion = comprobar_app_ini(&cuenta, &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Fallo);
        assert!(
            comprobacion
                .mensaje
                .localizar(Idioma::Es)
                .contains("certificado")
        );
    }

    #[test]
    fn app_ini_sin_lan_pero_escuchando_en_0_0_0_0_es_un_fallo() {
        // «0.0.0.0 con HTTP o sin lan en la config» debe ser ERROR, no aviso.
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let carpeta = temporal.path().join("cuenta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        let cuenta = Cuenta {
            login: nombre("jparga"),
            carpeta: carpeta.clone(),
            puerto: 33078,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        };

        std::fs::create_dir_all(rutas_cuenta.gitea_app_ini().parent().unwrap())
            .expect("crear conf/");
        std::fs::write(
            rutas_cuenta.gitea_app_ini(),
            "[server]\nHTTP_ADDR = 0.0.0.0\n",
        )
        .expect("escribir app.ini de prueba");
        std::fs::set_permissions(
            rutas_cuenta.gitea_app_ini(),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("permisos del app.ini");

        let comprobacion = comprobar_app_ini(&cuenta, &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Fallo);
    }

    #[test]
    fn cortafuegos_avisa_pero_no_falla_si_no_hay_ufw() {
        let comprobacion = comprobar_cortafuegos(|| false);
        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(comprobacion.mensaje.localizar(Idioma::Es).contains("ufw"));
    }

    #[test]
    fn cortafuegos_avisa_recordando_activarlo_si_esta_instalado() {
        let comprobacion = comprobar_cortafuegos(|| true);
        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(
            comprobacion
                .mensaje
                .localizar(Idioma::Es)
                .contains("instalado")
        );
    }

    #[test]
    fn aislamiento_systemd_avisa_si_apparmor_vale_uno() {
        let comprobacion = comprobar_aislamiento_systemd(|| Some("1\n".to_string()));
        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert_eq!(comprobacion.mensaje, TextoDoctor::AislamientoAviso);
    }

    #[test]
    fn aislamiento_systemd_ok_si_apparmor_vale_cero() {
        let comprobacion = comprobar_aislamiento_systemd(|| Some("0\n".to_string()));
        assert_eq!(comprobacion.nivel, NivelComprobacion::Ok);
    }

    #[test]
    fn aislamiento_systemd_ok_si_el_fichero_no_existe() {
        let comprobacion = comprobar_aislamiento_systemd(|| None);
        assert_eq!(comprobacion.nivel, NivelComprobacion::Ok);
    }

    #[test]
    fn exec_start_se_lee_con_y_sin_comillas() {
        assert_eq!(
            ejecutable_de_exec_start("[Service]\nExecStart=/usr/bin/gitmereba sync jparga\n"),
            Some("/usr/bin/gitmereba".to_string())
        );
        assert_eq!(
            ejecutable_de_exec_start("ExecStart=\"/home/u/Mis \\\"Clones\\\"/gitmereba\" sync x\n"),
            Some("/home/u/Mis \"Clones\"/gitmereba".to_string())
        );
        assert_eq!(ejecutable_de_exec_start("[Service]\nType=oneshot\n"), None);
    }

    #[test]
    fn temporizador_sin_unidad_es_aviso_y_con_ejecutable_perdido_es_fallo() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let login = nombre("jparga");
        let sin_unidad = comprobar_temporizador(temporal.path(), &login);
        assert_eq!(sin_unidad.nivel, NivelComprobacion::Aviso);

        let unidad = temporal.path().join(nombre_servicio_sync(&login));
        let perdido = temporal.path().join("no-existe");
        std::fs::write(
            &unidad,
            format!("ExecStart={} sync jparga\n", perdido.display()),
        )
        .expect("escribir unidad");
        let fallo = comprobar_temporizador(temporal.path(), &login);
        assert_eq!(fallo.nivel, NivelComprobacion::Fallo);
        assert!(fallo.mensaje.localizar(Idioma::Es).contains("no-existe"));

        let binario = temporal.path().join("gitmereba");
        std::fs::write(&binario, "#!/bin/sh\n").expect("escribir binario");
        std::fs::set_permissions(&binario, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable");
        std::fs::write(
            &unidad,
            format!("ExecStart={} sync jparga\n", binario.display()),
        )
        .expect("escribir unidad");
        assert_eq!(
            comprobar_temporizador(temporal.path(), &login).nivel,
            NivelComprobacion::Ok
        );
    }

    #[test]
    fn comprobar_snapshots_sin_capturas_es_ok() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas_cuenta = RutasCuenta::nueva(raiz.path().join("cuenta"));

        let comprobacion = comprobar_snapshots(&nombre("jparga"), &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Ok);
        assert!(
            comprobacion.mensaje
                == (TextoDoctor::SnapshotsResumen {
                    total: 0,
                    protegidas: 0
                })
        );
    }

    #[tokio::test]
    async fn comprobar_snapshots_con_una_captura_protegida_es_aviso() {
        use crate::modelo::{IdRepo, Nombre};

        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas_cuenta = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let id = IdRepo {
            dueno: Nombre::nuevo("jparga").expect("dueño válido"),
            nombre: Nombre::nuevo("repo1").expect("nombre válido"),
        };

        // Crea un bare de origen mínimo y captúralo, para poder protegerlo después.
        let trabajo = tempfile::tempdir().expect("árbol de trabajo");
        let opciones = crate::git::Opciones {
            directorio: Some(trabajo.path().to_path_buf()),
            ..Default::default()
        };
        crate::git::ejecutar(&["-c", "init.defaultBranch=main", "init", "-q"], &opciones)
            .await
            .expect("git init");
        crate::git::ejecutar(
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test.invalid",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "inicial",
            ],
            &opciones,
        )
        .await
        .expect("git commit");
        let bare = rutas_cuenta
            .gitea_repositorios()
            .join("jparga")
            .join("repo1.git");
        std::fs::create_dir_all(bare.parent().expect("padre")).expect("crear carpeta del dueño");
        crate::git::ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                trabajo.path().to_str().expect("utf8"),
                bare.to_str().expect("utf8"),
            ],
            &crate::git::Opciones::default(),
        )
        .await
        .expect("clonar en bare");

        let captura =
            crate::snapshots::capturar(&rutas_cuenta, &id, time::OffsetDateTime::now_utc())
                .await
                .expect("capturar no falla");
        let crate::snapshots::Captura::Nueva(manifiesto) = captura else {
            panic!("se esperaba una captura nueva")
        };
        crate::snapshots::proteger(&rutas_cuenta, &id, &manifiesto.id)
            .await
            .expect("proteger no falla");

        let comprobacion = comprobar_snapshots(&nombre("jparga"), &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(
            comprobacion.mensaje
                == (TextoDoctor::SnapshotsResumen {
                    total: 1,
                    protegidas: 1
                })
        );
    }

    // --- idioma ---

    fn entorno<'a>(pares: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |clave| {
            pares
                .iter()
                .find(|(nombre, _)| *nombre == clave)
                .map(|(_, valor)| valor.to_string())
        }
    }

    #[test]
    fn idioma_con_preferencia_explicita_es_informativo_e_incluye_la_ruta() {
        let ruta = Path::new("/c/preferencias.toml");
        let c = comprobar_idioma(
            LecturaPreferencia::Valida(Preferencia::En),
            ruta,
            &entorno(&[("LANG", "es_ES.UTF-8")]),
        );
        assert_eq!(c.nivel, NivelComprobacion::Ok);
        assert_eq!(
            c.mensaje,
            TextoDoctor::IdiomaPreferencia {
                idioma: Idioma::En,
                ruta: ruta.to_path_buf()
            }
        );
        assert_eq!(c.consejo, None);
    }

    #[test]
    fn idioma_auto_con_lang_es_indica_la_variable() {
        let c = comprobar_idioma(
            LecturaPreferencia::Ausente,
            Path::new("/c/p.toml"),
            &entorno(&[("LANG", "es_ES.UTF-8")]),
        );
        assert_eq!(c.nivel, NivelComprobacion::Ok);
        assert_eq!(c.mensaje.localizar(Idioma::Es), "es (de LANG=es_ES.UTF-8)");
    }

    #[test]
    fn idioma_auto_sin_variables_avisa_y_aconseja_fijarlo_en_ajustes() {
        let c = comprobar_idioma(
            LecturaPreferencia::Ausente,
            Path::new("/c/p.toml"),
            &entorno(&[]),
        );
        assert_eq!(c.nivel, NivelComprobacion::Aviso);
        assert_eq!(
            c.mensaje,
            TextoDoctor::IdiomaPorDefecto { idioma: Idioma::En }
        );
        assert_eq!(c.consejo, Some(TextoDoctor::ConsejoFijarIdioma));
    }

    #[test]
    fn idioma_con_preferencias_no_validas_avisa_e_incluye_la_ruta() {
        let ruta = Path::new("/c/preferencias.toml");
        let c = comprobar_idioma(
            LecturaPreferencia::NoValida,
            ruta,
            &entorno(&[("LANG", "es_ES")]),
        );
        assert_eq!(c.nivel, NivelComprobacion::Aviso);
        assert_eq!(
            c.mensaje,
            TextoDoctor::IdiomaPreferenciaNoValida {
                idioma: Idioma::Es,
                ruta: ruta.to_path_buf()
            }
        );
        assert_eq!(c.consejo, Some(TextoDoctor::ConsejoCorregirPreferencias));
    }

    fn systemd(pares: &[(&str, &str)]) -> HashMap<String, String> {
        pares
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn temporizador_sin_variables_avisa() {
        let c = comprobar_idioma_temporizador(
            LecturaPreferencia::Ausente,
            &entorno(&[("LANG", "es_ES.UTF-8")]),
            Some(&systemd(&[("PATH", "/usr/bin")])),
        )
        .expect("comprobación");
        assert_eq!(c.nivel, NivelComprobacion::Aviso);
        assert_eq!(c.mensaje, TextoDoctor::TemporizadorIdiomaSinVariables);
        assert_eq!(c.consejo, Some(TextoDoctor::ConsejoFijarIdioma));
    }

    #[test]
    fn temporizador_con_el_mismo_idioma_es_ok() {
        let c = comprobar_idioma_temporizador(
            LecturaPreferencia::Ausente,
            &entorno(&[("LANG", "es_ES.UTF-8")]),
            Some(&systemd(&[("LANG", "es_ES.UTF-8")])),
        )
        .expect("comprobación");
        assert_eq!(c.nivel, NivelComprobacion::Ok);
        assert_eq!(
            c.mensaje,
            TextoDoctor::TemporizadorIdiomaCoincide { idioma: Idioma::Es }
        );
    }

    #[test]
    fn temporizador_con_otro_idioma_avisa() {
        let c = comprobar_idioma_temporizador(
            LecturaPreferencia::Ausente,
            &entorno(&[("LANG", "es_ES.UTF-8")]),
            Some(&systemd(&[("LANG", "en_US.UTF-8")])),
        )
        .expect("comprobación");
        assert_eq!(c.nivel, NivelComprobacion::Aviso);
        assert_eq!(
            c.mensaje,
            TextoDoctor::TemporizadorIdiomaDistinto {
                temporizador: Idioma::En,
                sesion: Idioma::Es
            }
        );
        assert_eq!(c.consejo, Some(TextoDoctor::ConsejoFijarIdioma));
    }

    #[test]
    fn temporizador_no_disponible_es_informativa() {
        let c = comprobar_idioma_temporizador(
            LecturaPreferencia::Ausente,
            &entorno(&[("LANG", "es_ES.UTF-8")]),
            None,
        )
        .expect("comprobación");
        assert_eq!(c.nivel, NivelComprobacion::Ok);
        assert_eq!(c.mensaje, TextoDoctor::TemporizadorIdiomaNoDisponible);
    }

    #[test]
    fn temporizador_con_idioma_fijado_no_se_comprueba() {
        assert!(
            comprobar_idioma_temporizador(
                LecturaPreferencia::Valida(Preferencia::En),
                &entorno(&[]),
                None
            )
            .is_none()
        );
    }

    #[test]
    fn parsear_entorno_lee_clave_valor() {
        let e = parsear_entorno("LANG=es_ES.UTF-8\nPATH=/usr/bin\nbasura\n");
        assert_eq!(e.get("LANG").map(String::as_str), Some("es_ES.UTF-8"));
        assert_eq!(e.len(), 2);
    }
}
