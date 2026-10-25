//! Datos del entorno para la pantalla «Acerca de»: nombre del sistema, rutas acortadas,
//! enlaces del proyecto y versión de Gitea de una instancia frente a la incluida.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use url::Url;

use crate::gitea::ApiGitea;
use crate::instancia::VERSION_GITEA;

/// Enlaces del proyecto que la interfaz puede pedir abrir. Es un conjunto cerrado: la
/// interfaz nunca manda una URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enlace {
    Repositorio,
    Release,
    Seguridad,
    Marcas,
}

/// Cómo está la versión de Gitea de una cuenta frente a la incluida en el binario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoVersion {
    AlDia,
    Desfasada,
    Desconocida,
    NoDisponible,
}

impl EstadoVersion {
    pub fn codigo(self) -> &'static str {
        match self {
            EstadoVersion::AlDia => "al_dia",
            EstadoVersion::Desfasada => "desfasada",
            EstadoVersion::Desconocida => "desconocida",
            EstadoVersion::NoDisponible => "no_disponible",
        }
    }
}

/// Ruta del ejecutable que debe recordarse fuera del proceso (unidades de systemd, enlaces
/// de lanzador). Dentro de un AppImage `current_exe()` apunta a un montaje temporal
/// (`/tmp/.mount_*`) que desaparece al cerrar; ahí vale la variable `APPIMAGE`, que
/// apunta al propio fichero `.AppImage`. Fuera de un AppImage, `current_exe()`.
pub fn ruta_ejecutable_persistente() -> std::io::Result<PathBuf> {
    ruta_ejecutable_persistente_con(std::env::var_os("APPIMAGE"), std::env::current_exe)
}

/// Núcleo de [`ruta_ejecutable_persistente`] con el entorno inyectado. `APPIMAGE` solo se
/// acepta si es absoluta, sin caracteres de control (acabaría en una unidad de systemd)
/// y apunta a un fichero regular existente; si no, se usa `actual`.
fn ruta_ejecutable_persistente_con(
    appimage: Option<OsString>,
    actual: impl FnOnce() -> std::io::Result<PathBuf>,
) -> std::io::Result<PathBuf> {
    if let Some(valor) = appimage {
        let ruta = PathBuf::from(valor);
        let sin_control = !ruta.to_string_lossy().chars().any(char::is_control);
        let es_fichero = std::fs::metadata(&ruta).is_ok_and(|m| m.is_file());
        if ruta.is_absolute() && sin_control && es_fichero {
            return Ok(ruta);
        }
    }
    actual()
}

/// Auxiliar de WebKitGTK que delata una compilación AppImage, relativo a `usr`. Es la ruta
/// que `scripts/empaquetar.sh` deja dentro del AppDir.
const AUXILIAR_WEBKIT_RELATIVO: &str = "lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess";

/// Qué hacer con el directorio de trabajo antes de abrir la ventana.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanDirectorioVentana {
    /// `.deb`, `.tar.gz` o cargo: no se toca.
    SinCambios,
    /// AppImage: entrar en este `usr` (la `libwebkit2gtk` empaquetada busca sus procesos
    /// auxiliares con una ruta relativa a él).
    EntrarEn(PathBuf),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErrorEntornoAppImage {
    #[error("no se pudo localizar el ejecutable: {0}")]
    Ejecutable(String),
    #[error(
        "las variables APPIMAGE o APPDIR están definidas, pero {0} no es el ejecutable de un \
         AppImage de gitmereba: no se abre la ventana (WebKit podría ejecutar procesos del \
         directorio actual)"
    )]
    Incoherente(PathBuf),
    #[error("no se pudo entrar en {ruta} para abrir la ventana: {causa}")]
    CambioDeDirectorio { ruta: PathBuf, causa: String },
}

/// Decide el directorio de trabajo de la ventana. `ejecutable` es la ruta real
/// (canonicalizada) del binario, no una variable de entorno: solo un binario que cuelga de
/// `<usr>/bin/` con `<usr>` distinto de `/usr` y con el auxiliar de WebKit empaquetado en
/// `<usr>/lib/x86_64-linux-gnu/webkit2gtk-4.1/` es una compilación AppImage. Si, sin serlo,
/// hay variables de AppImage definidas (`hay_variables_appimage`), el estado es incoherente.
pub fn planificar_directorio_ventana(
    ejecutable: &Path,
    hay_variables_appimage: bool,
    existe: impl Fn(&Path) -> bool,
) -> Result<PlanDirectorioVentana, ErrorEntornoAppImage> {
    let usr = ejecutable
        .parent()
        .filter(|bin| bin.file_name().is_some_and(|n| n == "bin"))
        .and_then(Path::parent)
        .filter(|usr| *usr != Path::new("/usr") && usr.file_name().is_some());
    if let Some(usr) = usr
        && usr.is_absolute()
        && existe(&usr.join(AUXILIAR_WEBKIT_RELATIVO))
    {
        return Ok(PlanDirectorioVentana::EntrarEn(usr.to_path_buf()));
    }
    if hay_variables_appimage {
        return Err(ErrorEntornoAppImage::Incoherente(ejecutable.to_path_buf()));
    }
    Ok(PlanDirectorioVentana::SinCambios)
}

/// Aplica [`planificar_directorio_ventana`] al proceso real: ejecutable canonicalizado,
/// entorno y sistema de ficheros reales. Si hay que entrar en el `usr` del AppImage y no se
/// puede, devuelve error (la ventana no debe abrirse con un directorio de trabajo ajeno).
pub fn preparar_directorio_ventana() -> Result<(), ErrorEntornoAppImage> {
    let ejecutable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| ErrorEntornoAppImage::Ejecutable(error.to_string()))?;
    let hay_variables =
        std::env::var_os("APPIMAGE").is_some() || std::env::var_os("APPDIR").is_some();
    let plan = planificar_directorio_ventana(&ejecutable, hay_variables, |ruta| {
        std::fs::metadata(ruta).is_ok_and(|m| m.is_file())
    })?;
    match plan {
        PlanDirectorioVentana::SinCambios => Ok(()),
        PlanDirectorioVentana::EntrarEn(usr) => std::env::set_current_dir(&usr).map_err(|error| {
            ErrorEntornoAppImage::CambioDeDirectorio {
                ruta: usr,
                causa: error.to_string(),
            }
        }),
    }
}

/// Tope de lectura de `os-release`: el real ocupa menos de 1 KiB.
const TAMANO_MAXIMO_OS_RELEASE: u64 = 64 * 1024;

/// `PRETTY_NAME` de un fichero con el formato de `/etc/os-release`, sin comillas ni
/// caracteres de control. `None` si no es un fichero normal (un FIFO colgaría la lectura),
/// pasa del tope, no se puede leer o no tiene la clave.
pub fn nombre_sistema(ruta: &Path) -> Option<String> {
    let metadatos = std::fs::metadata(ruta).ok()?;
    if !metadatos.is_file() || metadatos.len() > TAMANO_MAXIMO_OS_RELEASE {
        return None;
    }
    let mut contenido = String::new();
    std::fs::File::open(ruta)
        .ok()?
        .take(TAMANO_MAXIMO_OS_RELEASE + 1)
        .read_to_string(&mut contenido)
        .ok()?;
    if contenido.len() as u64 > TAMANO_MAXIMO_OS_RELEASE {
        return None;
    }
    let valor = contenido
        .lines()
        .find_map(|linea| linea.trim().strip_prefix("PRETTY_NAME="))?;
    let valor = valor.trim();
    let valor = valor
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| valor.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(valor);
    let limpio: String = valor.chars().filter(|c| !c.is_control()).collect();
    let limpio = limpio.trim();
    (!limpio.is_empty()).then(|| limpio.to_string())
}

/// `ruta` con el directorio personal sustituido por `~`, para mostrarla y copiarla sin el
/// nombre de usuario. Compara por componentes: `/home/anabel` no está bajo `/home/ana`.
pub fn acortar_home(ruta: &Path, home: &Path) -> String {
    match ruta.strip_prefix(home) {
        Ok(resto) if resto.as_os_str().is_empty() => "~".to_string(),
        Ok(resto) => format!("~/{}", resto.display()),
        Err(_) => ruta.display().to_string(),
    }
}

/// URL de `enlace` a partir del repositorio del proyecto y de la versión en uso.
pub fn url_enlace(
    enlace: Enlace,
    repositorio: &str,
    version: &str,
) -> Result<Url, url::ParseError> {
    let base = repositorio.trim_end_matches('/');
    let url = match enlace {
        Enlace::Repositorio => base.to_string(),
        Enlace::Release => format!("{base}/releases/tag/v{version}"),
        Enlace::Seguridad => format!("{base}/blob/main/SECURITY.md"),
        Enlace::Marcas => format!("{base}/blob/main/TRADEMARKS.md"),
    };
    Url::parse(&url)
}

/// Componentes numéricos de una versión (`1.27.3+dev` → `[1, 27, 3]`), completados con
/// ceros hasta tres. `None` si alguno no es un número.
fn componentes(version: &str) -> Option<Vec<u64>> {
    let nucleo = version.trim().split(['+', '-']).next()?;
    let mut partes = nucleo
        .split('.')
        .map(|parte| parte.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    while partes.len() < 3 {
        partes.push(0);
    }
    Some(partes)
}

/// `instalada` frente a `incluida`: al día si es igual o más nueva, desfasada si es más
/// vieja, desconocida si no se puede leer como versión.
pub fn comparar_version(instalada: &str, incluida: &str) -> EstadoVersion {
    match (componentes(instalada), componentes(incluida)) {
        (Some(instalada), Some(incluida)) if instalada < incluida => EstadoVersion::Desfasada,
        (Some(_), Some(_)) => EstadoVersion::AlDia,
        _ => EstadoVersion::Desconocida,
    }
}

/// Versión que dice la instancia y su estado frente a [`VERSION_GITEA`]. Si no responde en
/// `espera`, o responde con error, no está disponible.
pub async fn version_gitea<G: ApiGitea>(
    gitea: &G,
    espera: Duration,
) -> (Option<String>, EstadoVersion) {
    match tokio::time::timeout(espera, gitea.version()).await {
        Ok(Ok(version)) => {
            let estado = comparar_version(&version, VERSION_GITEA);
            (Some(version), estado)
        }
        _ => (None, EstadoVersion::NoDisponible),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::gitea::ClienteGitea;
    use crate::secretos::Secreto;

    fn actual_falso() -> std::io::Result<PathBuf> {
        Ok(PathBuf::from("/usr/bin/gitmereba"))
    }

    #[test]
    fn ruta_persistente_sin_appimage_es_el_ejecutable_actual() {
        let ruta = ruta_ejecutable_persistente_con(None, actual_falso).unwrap();
        assert_eq!(ruta, PathBuf::from("/usr/bin/gitmereba"));
    }

    #[test]
    fn ruta_persistente_usa_appimage_si_es_fichero_regular_absoluto() {
        let carpeta = tempfile::tempdir().unwrap();
        let imagen = carpeta.path().join("gitmereba.AppImage");
        std::fs::write(&imagen, b"x").unwrap();
        let ruta =
            ruta_ejecutable_persistente_con(Some(imagen.clone().into_os_string()), actual_falso)
                .unwrap();
        assert_eq!(ruta, imagen);
    }

    #[test]
    fn ruta_persistente_ignora_appimage_relativa() {
        let ruta = ruta_ejecutable_persistente_con(Some("gitmereba.AppImage".into()), actual_falso)
            .unwrap();
        assert_eq!(ruta, PathBuf::from("/usr/bin/gitmereba"));
    }

    #[test]
    fn ruta_persistente_ignora_appimage_inexistente_o_directorio() {
        let carpeta = tempfile::tempdir().unwrap();
        let inexistente = carpeta.path().join("no-existe.AppImage");
        let ruta =
            ruta_ejecutable_persistente_con(Some(inexistente.into_os_string()), actual_falso)
                .unwrap();
        assert_eq!(ruta, PathBuf::from("/usr/bin/gitmereba"));
        let ruta = ruta_ejecutable_persistente_con(
            Some(carpeta.path().to_path_buf().into_os_string()),
            actual_falso,
        )
        .unwrap();
        assert_eq!(ruta, PathBuf::from("/usr/bin/gitmereba"));
    }

    #[test]
    fn ruta_persistente_ignora_appimage_vacia_o_con_caracteres_de_control() {
        let carpeta = tempfile::tempdir().unwrap();
        let con_salto = carpeta.path().join("a\nb.AppImage");
        std::fs::write(&con_salto, b"x").unwrap();
        for valor in [std::ffi::OsString::new(), con_salto.into_os_string()] {
            let ruta = ruta_ejecutable_persistente_con(Some(valor), actual_falso).unwrap();
            assert_eq!(ruta, PathBuf::from("/usr/bin/gitmereba"));
        }
    }

    fn existe_webkit_en(usr: &str) -> impl Fn(&Path) -> bool {
        let esperado = PathBuf::from(usr).join(AUXILIAR_WEBKIT_RELATIVO);
        move |ruta| ruta == esperado
    }

    #[test]
    fn plan_ventana_appimage_entra_en_el_usr_del_ejecutable() {
        let plan = planificar_directorio_ventana(
            Path::new("/tmp/.mount_abc/usr/bin/gitmereba"),
            false,
            existe_webkit_en("/tmp/.mount_abc/usr"),
        )
        .unwrap();
        assert_eq!(
            plan,
            PlanDirectorioVentana::EntrarEn(PathBuf::from("/tmp/.mount_abc/usr"))
        );
    }

    #[test]
    fn plan_ventana_appimage_no_depende_de_las_variables() {
        // Lanzado a mano desde el AppDir extraído: sigue entrando en su usr.
        let plan = planificar_directorio_ventana(
            Path::new("/home/u/squashfs-root/usr/bin/gitmereba"),
            true,
            existe_webkit_en("/home/u/squashfs-root/usr"),
        )
        .unwrap();
        assert!(matches!(plan, PlanDirectorioVentana::EntrarEn(_)));
    }

    #[test]
    fn plan_ventana_deb_y_tar_no_cambian_nada() {
        for exe in ["/usr/bin/gitmereba", "/home/u/.local/bin/gitmereba"] {
            let plan = planificar_directorio_ventana(Path::new(exe), false, |_| false).unwrap();
            assert_eq!(plan, PlanDirectorioVentana::SinCambios);
        }
    }

    #[test]
    fn plan_ventana_usr_del_sistema_nunca_es_appimage() {
        // Aunque /usr tenga WebKit instalado (un .deb), no es una compilación AppImage.
        let plan = planificar_directorio_ventana(Path::new("/usr/bin/gitmereba"), false, |_| true)
            .unwrap();
        assert_eq!(plan, PlanDirectorioVentana::SinCambios);
    }

    #[test]
    fn plan_ventana_rechaza_variables_de_appimage_fuera_de_un_appimage() {
        for exe in ["/usr/bin/gitmereba", "/home/u/.local/bin/gitmereba"] {
            let error = planificar_directorio_ventana(Path::new(exe), true, |_| false).unwrap_err();
            assert!(matches!(error, ErrorEntornoAppImage::Incoherente(_)));
        }
    }

    #[test]
    fn plan_ventana_ignora_ejecutables_que_no_son_usr_bin() {
        // Sin carpeta `bin` o con otro nombre de carpeta no hay `usr` que deducir.
        for exe in ["/gitmereba", "/opt/x/gitmereba", "/opt/usr/sbin/gitmereba"] {
            let plan = planificar_directorio_ventana(Path::new(exe), false, |_| true).unwrap();
            assert_eq!(plan, PlanDirectorioVentana::SinCambios, "{exe}");
        }
    }

    #[test]
    fn ruta_persistente_propaga_el_error_de_current_exe() {
        let resultado =
            ruta_ejecutable_persistente_con(None, || Err(std::io::Error::other("sin ejecutable")));
        assert!(resultado.is_err());
    }

    fn os_release(contenido: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("os-release");
        std::fs::write(&ruta, contenido).unwrap();
        (dir, ruta)
    }

    // --- nombre_sistema -------------------------------------------------------

    #[test]
    fn nombre_sistema_lee_pretty_name_entre_comillas() {
        let (_dir, ruta) =
            os_release("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 26.04 LTS\"\nID=ubuntu\n");
        assert_eq!(nombre_sistema(&ruta).as_deref(), Some("Ubuntu 26.04 LTS"));
    }

    #[test]
    fn nombre_sistema_admite_valor_sin_comillas() {
        let (_dir, ruta) = os_release("PRETTY_NAME=Debian\n");
        assert_eq!(nombre_sistema(&ruta).as_deref(), Some("Debian"));
    }

    #[test]
    fn nombre_sistema_sin_la_clave_es_none() {
        let (_dir, ruta) = os_release("NAME=Arch\n");
        assert_eq!(nombre_sistema(&ruta), None);
    }

    #[test]
    fn nombre_sistema_sin_fichero_es_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(nombre_sistema(&dir.path().join("no-existe")), None);
    }

    #[test]
    fn nombre_sistema_quita_caracteres_de_control() {
        let (_dir, ruta) = os_release("PRETTY_NAME=\"Fedora\u{1b}[31m 44\"\n");
        assert_eq!(nombre_sistema(&ruta).as_deref(), Some("Fedora[31m 44"));
    }

    #[test]
    fn nombre_sistema_rechaza_un_fichero_enorme() {
        let mut contenido = String::from("PRETTY_NAME=\"Grande\"\n");
        contenido.push_str(&"#".repeat(70 * 1024));
        let (_dir, ruta) = os_release(&contenido);
        assert_eq!(nombre_sistema(&ruta), None);
    }

    #[test]
    fn nombre_sistema_no_se_cuelga_con_un_fifo() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("fifo");
        let estado = std::process::Command::new("mkfifo")
            .arg(&ruta)
            .status()
            .unwrap();
        assert!(estado.success());
        assert_eq!(nombre_sistema(&ruta), None);
    }

    // --- acortar_home ---------------------------------------------------------

    #[test]
    fn acortar_home_sustituye_el_home_por_virgulilla() {
        let home = Path::new("/home/ana");
        assert_eq!(
            acortar_home(Path::new("/home/ana/.local/share/gitmereba"), home),
            "~/.local/share/gitmereba"
        );
        assert_eq!(acortar_home(Path::new("/home/ana"), home), "~");
    }

    #[test]
    fn acortar_home_no_confunde_un_prefijo_de_texto() {
        let home = Path::new("/home/ana");
        assert_eq!(
            acortar_home(Path::new("/home/anabel/x"), home),
            "/home/anabel/x"
        );
        assert_eq!(
            acortar_home(Path::new("/usr/bin/gitmereba"), home),
            "/usr/bin/gitmereba"
        );
    }

    // --- url_enlace -----------------------------------------------------------

    #[test]
    fn url_enlace_construye_los_cuatro_destinos() {
        let repo = "https://github.com/jparga/gitmereba";
        let url = |enlace| url_enlace(enlace, repo, "0.8.0").unwrap().to_string();
        assert_eq!(
            url(Enlace::Repositorio),
            "https://github.com/jparga/gitmereba"
        );
        assert_eq!(
            url(Enlace::Release),
            "https://github.com/jparga/gitmereba/releases/tag/v0.8.0"
        );
        assert_eq!(
            url(Enlace::Seguridad),
            "https://github.com/jparga/gitmereba/blob/main/SECURITY.md"
        );
        assert_eq!(
            url(Enlace::Marcas),
            "https://github.com/jparga/gitmereba/blob/main/TRADEMARKS.md"
        );
    }

    // --- comparar_version -----------------------------------------------------

    #[test]
    fn comparar_version_cubre_los_casos() {
        assert_eq!(comparar_version("1.27.3", "1.27.3"), EstadoVersion::AlDia);
        assert_eq!(
            comparar_version("1.26.1", "1.27.3"),
            EstadoVersion::Desfasada
        );
        assert_eq!(comparar_version("1.28.0", "1.27.3"), EstadoVersion::AlDia);
        assert_eq!(
            comparar_version("1.27.3+dev-5-gabc", "1.27.3"),
            EstadoVersion::AlDia
        );
        assert_eq!(comparar_version("1.27", "1.27.3"), EstadoVersion::Desfasada);
        assert_eq!(
            comparar_version("abc", "1.27.3"),
            EstadoVersion::Desconocida
        );
        assert_eq!(comparar_version("", "1.27.3"), EstadoVersion::Desconocida);
    }

    // --- version_gitea --------------------------------------------------------

    async fn servidor_version(respuesta: ResponseTemplate) -> MockServer {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/version"))
            .respond_with(respuesta)
            .mount(&servidor)
            .await;
        servidor
    }

    #[tokio::test]
    async fn version_gitea_compara_con_la_incluida() {
        let servidor = servidor_version(
            ResponseTemplate::new(200).set_body_json(json!({"version": VERSION_GITEA})),
        )
        .await;
        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        assert_eq!(
            version_gitea(&cliente, Duration::from_secs(3)).await,
            (Some(VERSION_GITEA.to_string()), EstadoVersion::AlDia)
        );
    }

    #[tokio::test]
    async fn version_gitea_con_error_no_esta_disponible() {
        let servidor = servidor_version(ResponseTemplate::new(500)).await;
        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        assert_eq!(
            version_gitea(&cliente, Duration::from_secs(3)).await,
            (None, EstadoVersion::NoDisponible)
        );
    }

    #[tokio::test]
    async fn version_gitea_no_espera_mas_de_la_cuenta() {
        let servidor = servidor_version(
            ResponseTemplate::new(200)
                .set_body_json(json!({"version": "1.27.3"}))
                .set_delay(Duration::from_millis(2000)),
        )
        .await;
        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let inicio = Instant::now();
        assert_eq!(
            version_gitea(&cliente, Duration::from_millis(100)).await,
            (None, EstadoVersion::NoDisponible)
        );
        assert!(inicio.elapsed() < Duration::from_millis(1000));
    }
}
