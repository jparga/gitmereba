//! Datos del entorno para la pantalla «Acerca de»: nombre del sistema, rutas acortadas,
//! enlaces del proyecto y versión de Gitea de una instancia frente a la incluida.

use std::io::Read;
use std::path::Path;
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
