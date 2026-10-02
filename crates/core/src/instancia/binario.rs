//! Descarga verificada del binario de Gitea: SHA-256 fijado en el código y firma GPG
//! con la clave oficial embebida.

use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;
use url::{Host, Url};

use crate::config::Rutas;

use super::error::ErrorInstancia;
use super::fichero;
use super::proceso::mapear_error_spawn;

/// Versión de Gitea que la app descarga, provisiona y ejecuta. Fijarla aquí es lo
/// que hace reproducible y auditable el binario que acaba corriendo.
pub const VERSION_GITEA: &str = "1.27.3";

/// SHA-256 de `gitea-1.27.3-linux-amd64`, fijado en el código: la descarga se
/// verifica contra este valor, nunca contra el `.sha256` que sirve el propio
/// servidor de descargas.
///
/// Obtenido el 2026-09-19 de
/// `https://dl.gitea.com/gitea/1.27.3/gitea-1.27.3-linux-amd64.sha256` y contrastado
/// con el mismo fichero publicado como asset de la release de GitHub
/// (`https://github.com/go-gitea/gitea/releases/download/v1.27.3/gitea-1.27.3-linux-amd64.sha256`,
/// origen independiente de dl.gitea.com, gestionado por el bot de releases de
/// go-gitea): ambos coinciden byte a byte.
pub const SHA256_GITEA_1_27_3_LINUX_AMD64: &str =
    "4da93c2c10b6980c359bcb86d5573ebfd7770e2e151756534edee24c8c12d971";

/// Clave pública oficial de Gitea en formato binario (no armored), huella
/// `7C9E68152594688862D62AF62D9AE806EC1592E2` ("Teabot <teabot@gitea.io>"), obtenida
/// de keys.openpgp.org y verificada por huella completa antes de convertirla con
/// `gpg --dearmor`. Se usa para comprobar la firma `.asc` de cada descarga con
/// `gpgv`.
const CLAVE_PUBLICA_GITEA: &[u8] = include_bytes!("../../recursos/gitea-clave.gpg");

const URL_BASE_POR_DEFECTO: &str = "https://dl.gitea.com";
const HOST_PERMITIDO: &str = "dl.gitea.com";
/// Tope de tamaño del binario (300 MB).
const TAMANO_MAXIMO_BYTES: u64 = 300 * 1024 * 1024;
/// Tope de tamaño de la firma `.asc`: es un fichero de unos cientos de bytes; un
/// límite generoso basta para no descargar nada desproporcionado si el servidor
/// respondiera con algo inesperado.
const TAMANO_MAXIMO_FIRMA: u64 = 1024 * 1024;
const TIEMPO_LIMITE_DESCARGA: Duration = Duration::from_secs(600);
const TIEMPO_LIMITE_GPGV: Duration = Duration::from_secs(30);
const RUTA_GPGV: &str = "/usr/bin/gpgv";

/// Asegura que `gitea-<VERSION_GITEA>` existe en `rutas.directorio_bin()`,
/// descargándolo y verificándolo (SHA-256 fijado + firma GPG) si hace falta.
/// Devuelve la ruta al binario, ya ejecutable (0700).
pub async fn asegurar_binario(rutas: &Rutas) -> Result<PathBuf, ErrorInstancia> {
    asegurar_binario_interno(
        URL_BASE_POR_DEFECTO,
        VERSION_GITEA,
        SHA256_GITEA_1_27_3_LINUX_AMD64,
        TAMANO_MAXIMO_BYTES,
        &rutas.directorio_bin(),
    )
    .await
}

/// Igual que [`asegurar_binario`], pero con la URL base, el SHA-256 esperado y el
/// tope de tamaño inyectables: lo que permiten probar las pruebas de este módulo
/// contra un servidor `wiremock` sin descargar 300 MB reales. La API pública usa
/// siempre los valores fijados.
async fn asegurar_binario_interno(
    url_base: &str,
    version: &str,
    sha256_esperado: &str,
    tope_bytes: u64,
    directorio_bin: &Path,
) -> Result<PathBuf, ErrorInstancia> {
    fichero::crear_directorio_privado(directorio_bin)?;
    let destino = directorio_bin.join(format!("gitea-{version}"));

    if destino.exists() {
        if sha256_de_fichero(&destino).await? == sha256_esperado {
            return Ok(destino);
        }
        fs::remove_file(&destino)
            .await
            .map_err(|e| ErrorInstancia::Io(e.to_string()))?;
    }

    let url_base = Url::parse(url_base).map_err(|_| {
        ErrorInstancia::EntradaInvalida("URL base de descarga no válida".to_string())
    })?;
    validar_host_permitido(&url_base)?;

    let cliente = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(TIEMPO_LIMITE_DESCARGA)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|e| ErrorInstancia::Red(e.without_url().to_string()))?;

    let url_binario = url_descarga(&url_base, version, "")?;
    let url_firma = url_descarga(&url_base, version, ".asc")?;

    let temporal = directorio_bin.join(format!(".gitea-{version}.descarga.tmp"));
    descargar_binario_verificado(
        &cliente,
        &url_binario,
        &temporal,
        sha256_esperado,
        tope_bytes,
    )
    .await?;

    if let Err(error) = verificar_y_finalizar(&cliente, &url_firma, &temporal, &destino).await {
        let _ = fs::remove_file(&temporal).await;
        return Err(error);
    }

    Ok(destino)
}

/// Descarga la firma, verifica el binario ya descargado en `temporal` con `gpgv` y,
/// si es válida, lo marca ejecutable y lo mueve a `destino`.
async fn verificar_y_finalizar(
    cliente: &Client,
    url_firma: &Url,
    temporal: &Path,
    destino: &Path,
) -> Result<(), ErrorInstancia> {
    let firma = descargar_acotado(cliente, url_firma, TAMANO_MAXIMO_FIRMA).await?;
    verificar_firma(temporal, &firma).await?;
    marcar_ejecutable(temporal).await?;
    fs::rename(temporal, destino)
        .await
        .map_err(|e| ErrorInstancia::Io(e.to_string()))
}

/// Descarga `url` a `destino` (fichero nuevo 0600) comprobando el tamaño sobre la
/// marcha y, al terminar, el SHA-256. Si algo falla, no deja ningún fichero.
async fn descargar_binario_verificado(
    cliente: &Client,
    url: &Url,
    destino: &Path,
    sha256_esperado: &str,
    tope_bytes: u64,
) -> Result<(), ErrorInstancia> {
    let mut respuesta = cliente
        .get(url.clone())
        .send()
        .await
        .map_err(mapear_error_red)?;
    if !respuesta.status().is_success() {
        return Err(ErrorInstancia::Red(format!(
            "respuesta {} al descargar el binario",
            respuesta.status()
        )));
    }
    if let Some(longitud) = respuesta.content_length()
        && longitud > tope_bytes
    {
        return Err(ErrorInstancia::TamanoExcedido(tope_bytes / (1024 * 1024)));
    }

    let mut fichero = crear_fichero_privado(destino).await?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;

    loop {
        let trozo = match respuesta.chunk().await {
            Ok(trozo) => trozo,
            Err(error) => {
                let _ = fs::remove_file(destino).await;
                return Err(mapear_error_red(error));
            }
        };
        let Some(trozo) = trozo else { break };

        total += trozo.len() as u64;
        if total > tope_bytes {
            let _ = fs::remove_file(destino).await;
            return Err(ErrorInstancia::TamanoExcedido(tope_bytes / (1024 * 1024)));
        }
        hasher.update(&trozo);
        if let Err(error) = fichero.write_all(&trozo).await {
            let _ = fs::remove_file(destino).await;
            return Err(ErrorInstancia::Io(error.to_string()));
        }
    }
    if let Err(error) = fichero.flush().await {
        let _ = fs::remove_file(destino).await;
        return Err(ErrorInstancia::Io(error.to_string()));
    }
    drop(fichero);

    let calculado = formatear_hash(&hasher.finalize());
    if calculado != sha256_esperado {
        let _ = fs::remove_file(destino).await;
        return Err(ErrorInstancia::Sha256NoCoincide);
    }
    Ok(())
}

/// Descarga `url` entera en memoria, acotada a `tope` bytes: pensado para la firma
/// (unos cientos de bytes), nunca para el binario.
async fn descargar_acotado(
    cliente: &Client,
    url: &Url,
    tope: u64,
) -> Result<Vec<u8>, ErrorInstancia> {
    let mut respuesta = cliente
        .get(url.clone())
        .send()
        .await
        .map_err(mapear_error_red)?;
    if !respuesta.status().is_success() {
        return Err(ErrorInstancia::Red(format!(
            "respuesta {} al descargar la firma",
            respuesta.status()
        )));
    }
    if let Some(longitud) = respuesta.content_length()
        && longitud > tope
    {
        return Err(ErrorInstancia::TamanoExcedido(tope / (1024 * 1024)));
    }

    let mut datos = Vec::new();
    while let Some(trozo) = respuesta.chunk().await.map_err(mapear_error_red)? {
        datos.extend_from_slice(&trozo);
        if datos.len() as u64 > tope {
            return Err(ErrorInstancia::TamanoExcedido(tope / (1024 * 1024)));
        }
    }
    Ok(datos)
}

/// Verifica con `gpgv` que `firma` (contenido de un `.asc`) es una firma válida de
/// `binario` hecha con la clave embebida. La clave y la firma se escriben en
/// ficheros temporales 0600 que se borran al terminar, se llame o no a `gpgv` sin
/// error.
async fn verificar_firma(binario: &Path, firma: &[u8]) -> Result<(), ErrorInstancia> {
    if !Path::new(RUTA_GPGV).exists() {
        return Err(ErrorInstancia::GpgvNoDisponible);
    }
    let directorio = binario
        .parent()
        .ok_or_else(|| ErrorInstancia::Io("ruta sin directorio padre".to_string()))?;
    let sufijo = std::process::id();
    let clave_tmp = directorio.join(format!(".gitea-clave-{sufijo}.tmp"));
    let firma_tmp = directorio.join(format!(".gitea-firma-{sufijo}.tmp"));

    let resultado = (async {
        escribir_temporal_privado(&clave_tmp, CLAVE_PUBLICA_GITEA)?;
        escribir_temporal_privado(&firma_tmp, firma)?;
        ejecutar_gpgv(&clave_tmp, &firma_tmp, binario).await
    })
    .await;

    let _ = std::fs::remove_file(&clave_tmp);
    let _ = std::fs::remove_file(&firma_tmp);
    resultado
}

async fn ejecutar_gpgv(clave: &Path, firma: &Path, binario: &Path) -> Result<(), ErrorInstancia> {
    let mut comando = Command::new(RUTA_GPGV);
    comando.env_clear();
    comando.arg("--keyring").arg(clave);
    comando.arg(firma);
    comando.arg(binario);
    comando.stdin(Stdio::null());
    comando.stdout(Stdio::piped());
    comando.stderr(Stdio::piped());
    comando.kill_on_drop(true);

    let salida = timeout(TIEMPO_LIMITE_GPGV, comando.output())
        .await
        .map_err(|_| ErrorInstancia::Timeout)?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ErrorInstancia::GpgvNoDisponible
            } else {
                mapear_error_spawn(error)
            }
        })?;

    if salida.status.success() {
        Ok(())
    } else {
        Err(ErrorInstancia::FirmaInvalida)
    }
}

/// SHA-256 de un fichero ya en disco, leído por trozos para no cargarlo entero en
/// memoria (el binario ocupa unos 126 MB).
async fn sha256_de_fichero(ruta: &Path) -> Result<String, ErrorInstancia> {
    let mut fichero = fs::File::open(ruta)
        .await
        .map_err(|e| ErrorInstancia::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let leidos = fichero
            .read(&mut buffer)
            .await
            .map_err(|e| ErrorInstancia::Io(e.to_string()))?;
        if leidos == 0 {
            break;
        }
        hasher.update(&buffer[..leidos]);
    }
    Ok(formatear_hash(&hasher.finalize()))
}

fn formatear_hash(hash: &[u8]) -> String {
    use std::fmt::Write;
    let mut salida = String::with_capacity(hash.len() * 2);
    for byte in hash {
        let _ = write!(salida, "{byte:02x}");
    }
    salida
}

/// Solo se descarga por HTTPS de `dl.gitea.com`, o por HTTP en loopback (para que
/// las pruebas puedan usar un servidor `wiremock` local): nunca de un host arbitrario.
fn validar_host_permitido(url: &Url) -> Result<(), ErrorInstancia> {
    let host_texto = url
        .host_str()
        .ok_or_else(|| ErrorInstancia::HostNoPermitido("(sin host)".to_string()))?
        .to_string();
    let es_loopback = matches!(url.host(), Some(Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(), Some(Host::Ipv6(ip)) if ip.is_loopback())
        || host_texto.eq_ignore_ascii_case("localhost");

    let permitido = match url.scheme() {
        "https" => host_texto.eq_ignore_ascii_case(HOST_PERMITIDO),
        "http" => es_loopback,
        _ => false,
    };
    if permitido {
        Ok(())
    } else {
        Err(ErrorInstancia::HostNoPermitido(host_texto))
    }
}

/// Construye `<base>/gitea/<version>/gitea-<version>-linux-amd64<sufijo>` con el
/// crate `url` (segmentos codificados, nunca por concatenación de cadenas).
fn url_descarga(base: &Url, version: &str, sufijo: &str) -> Result<Url, ErrorInstancia> {
    let mut url = base.clone();
    {
        let mut segmentos = url.path_segments_mut().map_err(|()| {
            ErrorInstancia::EntradaInvalida("la URL base no admite rutas".to_string())
        })?;
        segmentos.clear();
        segmentos.push("gitea");
        segmentos.push(version);
        segmentos.push(&format!("gitea-{version}-linux-amd64{sufijo}"));
    }
    Ok(url)
}

fn mapear_error_red(error: reqwest::Error) -> ErrorInstancia {
    ErrorInstancia::Red(error.without_url().to_string())
}

async fn crear_fichero_privado(ruta: &Path) -> Result<fs::File, ErrorInstancia> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(ruta)
        .await
        .map_err(|e| ErrorInstancia::Io(e.to_string()))
}

/// Escribe (síncrono, para ficheros de un tamaño trivial) un fichero 0600 nuevo. Se
/// usa solo para la clave y la firma temporales de `verificar_firma`.
fn escribir_temporal_privado(ruta: &Path, contenido: &[u8]) -> Result<(), ErrorInstancia> {
    use std::io::Write;
    let mut fichero = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(ruta)
        .map_err(|e| ErrorInstancia::Io(e.to_string()))?;
    fichero
        .write_all(contenido)
        .map_err(|e| ErrorInstancia::Io(e.to_string()))
}

async fn marcar_ejecutable(ruta: &Path) -> Result<(), ErrorInstancia> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(ruta, std::fs::Permissions::from_mode(0o700))
        .await
        .map_err(|e| ErrorInstancia::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn sha256_hex(datos: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(datos);
        formatear_hash(&hasher.finalize())
    }

    #[tokio::test]
    async fn hash_incorrecto_borra_el_fichero_y_no_llega_a_pedir_la_firma() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gitea/9.9.9/gitea-9.9.9-linux-amd64"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(b"contenido-de-prueba".to_vec()),
            )
            .expect(1)
            .mount(&servidor)
            .await;
        // Sin mock para «.asc»: si se llegara a pedir, wiremock haría fallar el test.

        let directorio = tempfile::tempdir().expect("directorio temporal");
        let resultado = asegurar_binario_interno(
            &servidor.uri(),
            "9.9.9",
            "0".repeat(64).as_str(),
            1024,
            directorio.path(),
        )
        .await;

        assert!(matches!(resultado, Err(ErrorInstancia::Sha256NoCoincide)));
        let ficheros: Vec<_> = std::fs::read_dir(directorio.path())
            .expect("leer directorio")
            .filter_map(Result::ok)
            .collect();
        assert!(
            ficheros.is_empty(),
            "no debería quedar ningún fichero: {ficheros:?}"
        );
    }

    #[tokio::test]
    async fn tamano_excedido_no_descarga_y_no_deja_fichero() {
        let servidor = MockServer::start().await;
        let cuerpo = vec![b'x'; 200];
        Mock::given(method("GET"))
            .and(path("/gitea/9.9.9/gitea-9.9.9-linux-amd64"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(cuerpo))
            .mount(&servidor)
            .await;

        let directorio = tempfile::tempdir().expect("directorio temporal");
        let resultado = asegurar_binario_interno(
            &servidor.uri(),
            "9.9.9",
            &sha256_hex(&[b'x'; 200]),
            100, // tope muy por debajo de los 200 bytes servidos
            directorio.path(),
        )
        .await;

        assert!(matches!(resultado, Err(ErrorInstancia::TamanoExcedido(_))));
        let ficheros: Vec<_> = std::fs::read_dir(directorio.path())
            .expect("leer directorio")
            .filter_map(Result::ok)
            .collect();
        assert!(ficheros.is_empty(), "{ficheros:?}");
    }

    #[tokio::test]
    async fn host_no_permitido_no_llega_a_hacer_ninguna_peticion() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let resultado = asegurar_binario_interno(
            "https://un-host-cualquiera.ejemplo.com",
            "9.9.9",
            "0".repeat(64).as_str(),
            1024,
            directorio.path(),
        )
        .await;
        assert!(matches!(resultado, Err(ErrorInstancia::HostNoPermitido(_))));
    }

    #[tokio::test]
    async fn http_solo_se_permite_en_loopback() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let resultado = asegurar_binario_interno(
            "http://ejemplo.com",
            "9.9.9",
            "0".repeat(64).as_str(),
            1024,
            directorio.path(),
        )
        .await;
        assert!(matches!(resultado, Err(ErrorInstancia::HostNoPermitido(_))));
    }

    #[tokio::test]
    async fn binario_existente_con_hash_correcto_no_se_vuelve_a_descargar() {
        let servidor = MockServer::start().await;
        // Sin ningún mock montado: cualquier petición haría fallar el test.

        let directorio = tempfile::tempdir().expect("directorio temporal");
        let contenido = b"ya-estoy-aqui".to_vec();
        let destino = directorio.path().join("gitea-9.9.9");
        std::fs::write(&destino, &contenido).expect("escribir binario de prueba");

        let ruta = asegurar_binario_interno(
            &servidor.uri(),
            "9.9.9",
            &sha256_hex(&contenido),
            1024,
            directorio.path(),
        )
        .await
        .expect("no hace falta descargar nada");

        assert_eq!(ruta, destino);
        assert_eq!(std::fs::read(&destino).expect("leer"), contenido);
    }

    #[test]
    fn formatear_hash_coincide_con_un_vector_conocido() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
