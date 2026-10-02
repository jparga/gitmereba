//! `gitmereba doctor`: comprobaciones de salud del sistema y de cada cuenta.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::almacen::VerificacionAuditoria;
use crate::config::RutasCuenta;
use crate::git;
use crate::gitea::ApiGitea;
use crate::instancia::{SHA256_GITEA_1_27_3_LINUX_AMD64, VERSION_GITEA, nombre_servicio_sync};
use crate::modelo::{Cuenta, Nombre};
use crate::secretos::{ClaveSecreto, Llavero, Secreto};
use crate::snapshots;

use super::contexto::Contexto;
use super::listar::listar;

const RUTA_GPGV: &str = "/usr/bin/gpgv";
/// Fichero de AppArmor que, a `1`, indica que los servicios `systemd --user` no pueden
/// aislar el sistema de ficheros (comprobación
/// «aislamiento-systemd»).
const RUTA_APPARMOR_RESTRICT_USERNS: &str =
    "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
/// Texto exacto del aviso cuando AppArmor restringe los espacios de nombres de usuario
/// sin privilegios: `systemd --user` no puede montar nada aislado, así que
/// `ProtectSystem`/`ProtectHome`/`ReadWritePaths`/`PrivateTmp` se ignoran en silencio.
const AVISO_AISLAMIENTO_SYSTEMD: &str = "Este sistema impide a los servicios de usuario aislar \
     el sistema de ficheros: las protecciones de montaje de las unidades no se aplican. \
     Siguen activas las de llamadas al sistema y red.";

/// Resultado de una comprobación de [`doctor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NivelComprobacion {
    Ok,
    Aviso,
    Fallo,
}

/// Una comprobación de `doctor`, con un consejo en español cuando no está todo bien.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comprobacion {
    pub nombre: String,
    pub nivel: NivelComprobacion,
    pub mensaje: String,
    pub consejo: Option<String>,
}

impl Comprobacion {
    fn ok(nombre: &str, mensaje: impl Into<String>) -> Self {
        Self {
            nombre: nombre.to_string(),
            nivel: NivelComprobacion::Ok,
            mensaje: mensaje.into(),
            consejo: None,
        }
    }

    fn aviso(nombre: &str, mensaje: impl Into<String>, consejo: impl Into<String>) -> Self {
        Self {
            nombre: nombre.to_string(),
            nivel: NivelComprobacion::Aviso,
            mensaje: mensaje.into(),
            consejo: Some(consejo.into()),
        }
    }

    fn fallo(nombre: &str, mensaje: impl Into<String>, consejo: impl Into<String>) -> Self {
        Self {
            nombre: nombre.to_string(),
            nivel: NivelComprobacion::Fallo,
            mensaje: mensaje.into(),
            consejo: Some(consejo.into()),
        }
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
            "cuentas",
            format!("no se pudo leer el índice de cuentas: {error}"),
            "revisa los permisos de ~/.local/share/gitmereba/cuentas.toml",
        )),
    }

    InformeDoctor { comprobaciones }
}

async fn comprobar_git() -> Comprobacion {
    match git::version().await {
        Ok(version) => Comprobacion::ok("git", version),
        Err(error) => Comprobacion::fallo(
            "git",
            format!("git no está disponible: {error}"),
            "instala git y asegúrate de que está en el PATH",
        ),
    }
}

fn comprobar_gpgv() -> Comprobacion {
    if Path::new(RUTA_GPGV).exists() {
        Comprobacion::ok("gpgv", format!("disponible en {RUTA_GPGV}"))
    } else {
        Comprobacion::fallo(
            "gpgv",
            format!("no se encuentra {RUTA_GPGV}"),
            "instala el paquete gnupg (necesario para verificar el binario de Gitea)",
        )
    }
}

fn comprobar_llavero<L: Llavero>(llavero: &L) -> Comprobacion {
    let sonda = match Nombre::nuevo("gitmereba-doctor-sonda") {
        Ok(nombre) => nombre,
        Err(error) => {
            return Comprobacion::fallo(
                "llavero",
                format!("error interno al comprobar el llavero: {error}"),
                "repite la comprobación; si persiste, informa del error",
            );
        }
    };
    let resultado = llavero
        .guardar(&sonda, ClaveSecreto::TokenGithub, &Secreto::nuevo("sonda"))
        .and_then(|()| llavero.borrar(&sonda, ClaveSecreto::TokenGithub));
    match resultado {
        Ok(()) => Comprobacion::ok("llavero", "accesible"),
        Err(error) => Comprobacion::fallo(
            "llavero",
            format!("el llavero no está accesible: {error}"),
            "comprueba que hay un Secret Service en marcha (GNOME Keyring, KWallet)",
        ),
    }
}

fn comprobar_directorio_datos(directorio: &Path) -> Comprobacion {
    if !directorio.exists() {
        return Comprobacion::aviso(
            "directorio-datos",
            "todavía no existe (no se ha dado de alta ninguna cuenta)",
            "se creará automáticamente con «gitmereba cuenta add»",
        );
    }
    match permisos_de(directorio) {
        Ok(0o700) => Comprobacion::ok("directorio-datos", "permisos 0700"),
        Ok(modo) => Comprobacion::fallo(
            "directorio-datos",
            format!("permisos {modo:o}, deberían ser 0700"),
            format!("ejecuta: chmod 700 {}", directorio.display()),
        ),
        Err(error) => Comprobacion::fallo(
            "directorio-datos",
            format!("no se pudo leer sus permisos: {error}"),
            "comprueba que el directorio existe y es accesible",
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
    let nombre = "aislamiento-systemd";
    match leer().as_deref().map(str::trim) {
        Some("1") => Comprobacion::aviso(
            nombre,
            AVISO_AISLAMIENTO_SYSTEMD,
            "las protecciones de montaje (ProtectSystem, ProtectHome, ReadWritePaths, \
             PrivateTmp) de las unidades de usuario no se aplican en este sistema; las de \
             llamadas al sistema (seccomp) y red siguen activas",
        ),
        _ => Comprobacion::ok(
            nombre,
            "las protecciones de montaje de las unidades de usuario se aplican",
        ),
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
    let nombre = "cortafuegos";
    if existe() {
        Comprobacion::aviso(
            nombre,
            "ufw está instalado: revisa que esté activo antes de exponer alguna cuenta a la LAN",
            "actívalo con «sudo ufw enable» y, para cada cuenta expuesta, limita el acceso con \
             «sudo ufw allow from <red>/<prefijo> to any port <puerto> proto tcp»",
        )
    } else {
        Comprobacion::aviso(
            nombre,
            "no se encontró «ufw» en este sistema",
            "instala ufw (u otro cortafuegos) antes de exponer alguna cuenta a la LAN con \
             «gitmereba cuenta lan --activar», y limita el acceso a tu red de confianza",
        )
    }
}

fn comprobar_auditoria(almacen: &crate::almacen::Almacen) -> Comprobacion {
    match almacen.verificar_auditoria() {
        Ok(VerificacionAuditoria::Integra) => Comprobacion::ok("auditoria", "cadena íntegra"),
        Ok(VerificacionAuditoria::Rota { id }) => Comprobacion::fallo(
            "auditoria",
            format!("la cadena de auditoría está rota a partir de la entrada {id}"),
            "investiga si el fichero de la base de datos se ha manipulado a mano",
        ),
        Err(error) => Comprobacion::fallo(
            "auditoria",
            format!("no se pudo verificar: {error}"),
            "comprueba el acceso al almacén (~/.local/share/gitmereba/gitmereba.db)",
        ),
    }
}

async fn comprobar_cuenta<L: Llavero>(
    directorio_bin: &Path,
    llavero: &L,
    cuenta: &Cuenta,
) -> Vec<Comprobacion> {
    let prefijo = format!("cuenta:{}", cuenta.login);
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let mut comprobaciones = Vec::new();

    comprobaciones.push(comprobar_carpeta_cuenta(&prefijo, rutas_cuenta.carpeta()));
    comprobaciones.push(comprobar_app_ini(&prefijo, cuenta, &rutas_cuenta));
    comprobaciones.push(comprobar_binario_en(&prefijo, directorio_bin).await);
    comprobaciones.push(comprobar_gitea_responde(&prefijo, cuenta, llavero).await);
    comprobaciones.push(comprobar_secretos(&prefijo, llavero, &cuenta.login));
    comprobaciones.push(comprobar_snapshots(&prefijo, &rutas_cuenta));

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
    let nombre = format!("cuenta:{login}:temporizador");
    let ruta_unidad = directorio_systemd.join(nombre_servicio_sync(login));
    let Ok(unidad) = std::fs::read_to_string(&ruta_unidad) else {
        return Comprobacion::aviso(
            &nombre,
            "no hay temporizador de sincronización instalado",
            "abre la ventana de gitmereba una vez: lo instala sola; hasta entonces solo se \
             sincroniza a mano",
        );
    };
    let Some(ejecutable) = ejecutable_de_exec_start(&unidad) else {
        return Comprobacion::fallo(
            &nombre,
            format!(
                "«{}» no tiene un ExecStart reconocible",
                ruta_unidad.display()
            ),
            "abre la ventana de gitmereba una vez: reescribe la unidad",
        );
    };
    let es_ejecutable = std::fs::metadata(&ejecutable)
        .is_ok_and(|datos| datos.is_file() && datos.permissions().mode() & 0o111 != 0);
    if es_ejecutable {
        Comprobacion::ok(&nombre, format!("sincroniza con «{ejecutable}»"))
    } else {
        Comprobacion::fallo(
            &nombre,
            format!("el temporizador apunta a «{ejecutable}», que ya no existe o no es ejecutable"),
            "abre la ventana de gitmereba una vez (o guarda Ajustes): el temporizador pasa a \
             usar el ejecutable actual",
        )
    }
}

/// Cuántas capturas hay y cuántas de ellas están protegidas (pendientes de revisar: una
/// captura solo se protege cuando se ha detectado un cambio destructivo frente a ella,
/// ver `cuentas::proteccion`). Aviso si hay alguna protegida.
fn comprobar_snapshots(prefijo: &str, rutas_cuenta: &RutasCuenta) -> Comprobacion {
    let nombre = format!("{prefijo}:snapshots");
    match snapshots::listar_cuenta(rutas_cuenta) {
        Ok(capturas) => {
            let protegidas = capturas.iter().filter(|c| c.protegida).count();
            let mensaje = format!("{} captura(s), {protegidas} protegida(s)", capturas.len());
            if protegidas > 0 {
                Comprobacion::aviso(
                    &nombre,
                    mensaje,
                    "hay capturas protegidas por un cambio destructivo detectado en el \
                     origen (historia reescrita, rama o tag borrado): revísalas antes de \
                     que la retención normal pueda alcanzarlas",
                )
            } else {
                Comprobacion::ok(&nombre, mensaje)
            }
        }
        Err(error) => Comprobacion::fallo(
            &nombre,
            format!("no se pudieron listar los snapshots: {error}"),
            "comprueba los permisos de la carpeta «snapshots/» de la cuenta",
        ),
    }
}

fn comprobar_carpeta_cuenta(prefijo: &str, carpeta: &Path) -> Comprobacion {
    let nombre = format!("{prefijo}:carpeta");
    if !carpeta.exists() {
        return Comprobacion::fallo(
            &nombre,
            "la carpeta de la cuenta no existe",
            "repite el alta o restaura la carpeta desde una copia",
        );
    }
    match permisos_de(carpeta) {
        Ok(0o700) => Comprobacion::ok(&nombre, "existe con permisos 0700"),
        Ok(modo) => Comprobacion::fallo(
            &nombre,
            format!("permisos {modo:o}, deberían ser 0700"),
            format!("ejecuta: chmod 700 {}", carpeta.display()),
        ),
        Err(error) => Comprobacion::fallo(&nombre, error.to_string(), "revisa los permisos a mano"),
    }
}

/// Comprueba `app.ini` (dos perfiles válidos según `cuenta.lan`).
///
/// Sin acceso LAN configurado, exige exactamente lo de siempre: 0600 y
/// `HTTP_ADDR = 127.0.0.1`; `0.0.0.0` sin `lan` en `gitmereba.toml` es un **error**
/// (Gitea escucharía en la red sin que la app lo sepa). Con acceso LAN activo, exige
/// `PROTOCOL = https`, el certificado presente y su clave con permisos 0600, y lo
/// informa como un **aviso** (no un error): «expuesto a la LAN por HTTPS».
fn comprobar_app_ini(prefijo: &str, cuenta: &Cuenta, rutas_cuenta: &RutasCuenta) -> Comprobacion {
    let nombre = format!("{prefijo}:app.ini");
    let app_ini = rutas_cuenta.gitea_app_ini();
    if !app_ini.exists() {
        return Comprobacion::fallo(
            &nombre,
            "app.ini no existe",
            "repite el alta: la provisión no llegó a completarse",
        );
    }
    let modo = match permisos_de(&app_ini) {
        Ok(modo) => modo,
        Err(error) => {
            return Comprobacion::fallo(&nombre, error.to_string(), "revisa los permisos a mano");
        }
    };
    let contenido = match std::fs::read_to_string(&app_ini) {
        Ok(contenido) => contenido,
        Err(error) => {
            return Comprobacion::fallo(
                &nombre,
                error.to_string(),
                "revisa que el fichero es legible",
            );
        }
    };
    let permisos_ok = modo == 0o600;

    match &cuenta.lan {
        None => comprobar_app_ini_sin_lan(&nombre, permisos_ok, modo, &contenido),
        Some(acceso) => comprobar_app_ini_con_lan(
            &nombre,
            permisos_ok,
            modo,
            &contenido,
            &acceso.host,
            rutas_cuenta,
        ),
    }
}

fn comprobar_app_ini_sin_lan(
    nombre: &str,
    permisos_ok: bool,
    modo: u32,
    contenido: &str,
) -> Comprobacion {
    let escucha_local = contenido.contains("HTTP_ADDR = 127.0.0.1");

    if permisos_ok && escucha_local {
        Comprobacion::ok(nombre, "0600 y HTTP_ADDR = 127.0.0.1")
    } else {
        let mut motivos = Vec::new();
        if !permisos_ok {
            motivos.push(format!("permisos {modo:o} (deberían ser 0600)"));
        }
        if !escucha_local {
            motivos.push(
                "no contiene «HTTP_ADDR = 127.0.0.1»: Gitea podría escuchar en la red \
                 sin acceso LAN configurado en gitmereba.toml"
                    .to_string(),
            );
        }
        Comprobacion::fallo(
            nombre,
            motivos.join("; "),
            "revisa app.ini a mano; sin acceso LAN nunca debe escuchar fuera de 127.0.0.1",
        )
    }
}

fn comprobar_app_ini_con_lan(
    nombre: &str,
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
            format!("expuesto a la LAN por HTTPS ({host})"),
            "confirma que hay un cortafuegos limitando el acceso a tu LAN de confianza \
             (ver la comprobación «cortafuegos»)",
        )
    } else {
        let mut motivos = Vec::new();
        if !permisos_ok {
            motivos.push(format!("permisos {modo:o} (deberían ser 0600)"));
        }
        if !https_ok {
            motivos.push(
                "no contiene «PROTOCOL = https» pese a tener acceso LAN configurado".to_string(),
            );
        }
        if !certificado_ok {
            motivos.push("no se encuentra el certificado del acceso LAN".to_string());
        }
        if !clave_ok {
            motivos.push("la clave del certificado no tiene permisos 0600".to_string());
        }
        Comprobacion::fallo(
            nombre,
            motivos.join("; "),
            "repite «gitmereba cuenta lan --activar» para regenerar el certificado y app.ini",
        )
    }
}

/// Comprueba el binario de Gitea de `directorio_bin` (compartido entre cuentas) contra el SHA-256 fijado en el código. Se repite por cuenta para que
/// cada tarjeta de diagnóstico esté completa.
async fn comprobar_binario_en(prefijo: &str, directorio_bin: &Path) -> Comprobacion {
    let nombre = format!("{prefijo}:binario-gitea");
    let ruta = directorio_bin.join(format!("gitea-{VERSION_GITEA}"));
    if !ruta.exists() {
        return Comprobacion::fallo(
            &nombre,
            format!("no se encuentra {}", ruta.display()),
            "ejecuta de nuevo el alta o «gitmereba doctor» tras reinstalar",
        );
    }
    match sha256_de(&ruta).await {
        Ok(hash) if hash == SHA256_GITEA_1_27_3_LINUX_AMD64 => {
            Comprobacion::ok(&nombre, format!("SHA-256 correcto ({VERSION_GITEA})"))
        }
        Ok(_) => Comprobacion::fallo(
            &nombre,
            "el SHA-256 no coincide con el esperado",
            "borra el binario y deja que la app lo vuelva a descargar y verificar",
        ),
        Err(error) => Comprobacion::fallo(
            &nombre,
            format!("no se pudo calcular su SHA-256: {error}"),
            "comprueba que el fichero es legible",
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

async fn comprobar_gitea_responde<L: Llavero>(
    prefijo: &str,
    cuenta: &Cuenta,
    llavero: &L,
) -> Comprobacion {
    let nombre = format!("{prefijo}:gitea");
    let token = llavero
        .leer(&cuenta.login, ClaveSecreto::TokenGitea)
        .ok()
        .flatten()
        .unwrap_or_else(|| Secreto::nuevo(""));
    let cliente = match super::comun::cliente_gitea_de_cuenta(cuenta, token) {
        Ok(cliente) => cliente,
        Err(error) => {
            return Comprobacion::fallo(&nombre, error.to_string(), "revisa la URL de la cuenta");
        }
    };
    match cliente.salud().await {
        Ok(true) => Comprobacion::ok(&nombre, "responde"),
        Ok(false) => Comprobacion::aviso(
            &nombre,
            "no responde",
            "arráncalo con «systemctl --user start» o revisa el servicio",
        ),
        Err(error) => {
            Comprobacion::fallo(&nombre, error.to_string(), "revisa el servicio de Gitea")
        }
    }
}

fn comprobar_secretos<L: Llavero>(prefijo: &str, llavero: &L, login: &Nombre) -> Comprobacion {
    let nombre = format!("{prefijo}:secretos");
    let claves = [
        ClaveSecreto::TokenGithub,
        ClaveSecreto::PasswordAdminGitea,
        ClaveSecreto::TokenGitea,
    ];
    let mut faltan = Vec::new();
    for clave in claves {
        match llavero.leer(login, clave) {
            Ok(Some(secreto)) if !secreto.esta_vacio() => {}
            Ok(_) => faltan.push(clave.etiqueta()),
            Err(error) => {
                return Comprobacion::fallo(
                    &nombre,
                    error.to_string(),
                    "revisa el llavero del sistema",
                );
            }
        }
    }
    if faltan.is_empty() {
        Comprobacion::ok(&nombre, "los tres secretos están presentes")
    } else {
        Comprobacion::fallo(
            &nombre,
            format!("faltan en el llavero: {}", faltan.join(", ")),
            "repite el alta para regenerarlos",
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
            .find(|c| c.nombre == "cuenta:jparga:carpeta")
            .expect("existe la comprobación de carpeta");
        assert_eq!(carpeta_check.nivel, NivelComprobacion::Fallo);

        let app_ini_check = informe
            .comprobaciones
            .iter()
            .find(|c| c.nombre == "cuenta:jparga:app.ini")
            .expect("existe la comprobación de app.ini");
        assert_eq!(app_ini_check.nivel, NivelComprobacion::Fallo);
        assert!(
            app_ini_check.mensaje.contains("127.0.0.1") || app_ini_check.mensaje.contains("red")
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

        assert!(informe.comprobaciones.iter().any(|c| c.nombre == "git"));
        assert!(informe.comprobaciones.iter().any(|c| c.nombre == "llavero"));
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == "auditoria")
        );
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == "aislamiento-systemd")
        );
        assert!(
            informe
                .comprobaciones
                .iter()
                .any(|c| c.nombre == "cortafuegos")
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

        let comprobacion = comprobar_app_ini("cuenta:jparga", &cuenta, &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(comprobacion.mensaje.contains("LAN"));
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

        let comprobacion = comprobar_app_ini("cuenta:jparga", &cuenta, &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Fallo);
        assert!(comprobacion.mensaje.contains("certificado"));
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

        let comprobacion = comprobar_app_ini("cuenta:jparga", &cuenta, &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Fallo);
    }

    #[test]
    fn cortafuegos_avisa_pero_no_falla_si_no_hay_ufw() {
        let comprobacion = comprobar_cortafuegos(|| false);
        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(comprobacion.mensaje.contains("ufw"));
    }

    #[test]
    fn cortafuegos_avisa_recordando_activarlo_si_esta_instalado() {
        let comprobacion = comprobar_cortafuegos(|| true);
        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(comprobacion.mensaje.contains("instalado"));
    }

    #[test]
    fn aislamiento_systemd_avisa_si_apparmor_vale_uno() {
        let comprobacion = comprobar_aislamiento_systemd(|| Some("1\n".to_string()));
        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert_eq!(comprobacion.mensaje, AVISO_AISLAMIENTO_SYSTEMD);
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
        assert!(fallo.mensaje.contains("no-existe"));

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

        let comprobacion = comprobar_snapshots("cuenta:jparga", &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Ok);
        assert!(comprobacion.mensaje.contains('0'));
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

        let comprobacion = comprobar_snapshots("cuenta:jparga", &rutas_cuenta);

        assert_eq!(comprobacion.nivel, NivelComprobacion::Aviso);
        assert!(comprobacion.mensaje.contains('1'));
    }
}
