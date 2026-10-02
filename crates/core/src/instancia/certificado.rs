//! Certificado autofirmado del acceso LAN: generado con `gitea cert` (sin
//! shell, argumentos como lista, `env_clear` como el resto de este módulo), ECDSA
//! P-256, válido 10 años, en `<carpeta>/gitea/tls/` (directorio 0700, clave 0600,
//! certificado 0644). La clave privada nunca se lee, nunca se imprime y nunca se
//! registra desde este módulo: solo se comprueba su existencia y sus permisos.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::config::RutasCuenta;
use crate::modelo::NombreHostInterno;

use super::error::ErrorInstancia;
use super::fichero;
use super::proceso::ejecutar_gitea;

const TIEMPO_LIMITE_CERT: Duration = Duration::from_secs(30);
/// 10 años, en horas: 10 * 365 * 24.
const DURACION_CERTIFICADO: &str = "87600h";
/// Marca en texto plano (no es un secreto) con el host para el que se generó el
/// certificado vigente: permite decidir si `asegurar_certificado` puede ser idempotente
/// sin tener que analizar el X.509 (si ya existe y cubre ese host, no se
/// regenera).
const FICHERO_MARCA_HOST: &str = "generado-para";

/// Parámetros de [`asegurar_certificado`].
pub struct ParametrosCertificado<'a> {
    pub binario_gitea: &'a Path,
    pub rutas: &'a RutasCuenta,
    pub host: &'a NombreHostInterno,
}

/// Resultado de [`asegurar_certificado`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoCertificado {
    /// `false` si ya existía un certificado válido para ese host y no se ha tocado.
    pub generado: bool,
    pub ruta_certificado: PathBuf,
    pub ruta_clave: PathBuf,
}

/// Genera (si hace falta) el certificado autofirmado de `parametros.host`, válido
/// también para `127.0.0.1` y `localhost` (para que la propia app pueda hablarle por
/// loopback). Idempotente: si ya hay un certificado y una clave generados para ese
/// mismo host, no se vuelve a invocar `gitea cert`.
pub async fn asegurar_certificado(
    parametros: &ParametrosCertificado<'_>,
) -> Result<ResultadoCertificado, ErrorInstancia> {
    let rutas = parametros.rutas;
    let directorio_tls = rutas.gitea_tls();
    fichero::crear_directorio_privado(&directorio_tls)?;

    let ruta_certificado = rutas.gitea_tls_cert();
    let ruta_clave = rutas.gitea_tls_key();
    let ruta_marca = directorio_tls.join(FICHERO_MARCA_HOST);

    if cubre_el_host(&ruta_certificado, &ruta_clave, &ruta_marca, parametros.host) {
        return Ok(ResultadoCertificado {
            generado: false,
            ruta_certificado,
            ruta_clave,
        });
    }

    let host_arg = format!("{},127.0.0.1,localhost", parametros.host.as_str());
    let ruta_certificado_str = ruta_como_str(&ruta_certificado)?;
    let ruta_clave_str = ruta_como_str(&ruta_clave)?;

    ejecutar_gitea(
        parametros.binario_gitea,
        &[
            "cert",
            "--host",
            &host_arg,
            "--ecdsa-curve",
            "P256",
            "--duration",
            DURACION_CERTIFICADO,
            "--out",
            ruta_certificado_str,
            "--keyout",
            ruta_clave_str,
        ],
        &directorio_tls,
        TIEMPO_LIMITE_CERT,
    )
    .await?;

    fijar_permisos(&ruta_certificado, 0o644)?;
    fijar_permisos(&ruta_clave, 0o600)?;
    std::fs::write(&ruta_marca, parametros.host.as_str())
        .map_err(|error| ErrorInstancia::Io(error.to_string()))?;

    Ok(ResultadoCertificado {
        generado: true,
        ruta_certificado,
        ruta_clave,
    })
}

fn cubre_el_host(
    ruta_certificado: &Path,
    ruta_clave: &Path,
    ruta_marca: &Path,
    host: &NombreHostInterno,
) -> bool {
    ruta_certificado.exists()
        && ruta_clave.exists()
        && std::fs::read_to_string(ruta_marca)
            .map(|marca| marca == host.as_str())
            .unwrap_or(false)
}

fn ruta_como_str(ruta: &Path) -> Result<&str, ErrorInstancia> {
    ruta.to_str()
        .ok_or_else(|| ErrorInstancia::EntradaInvalida("la ruta no es UTF-8".to_string()))
}

fn fijar_permisos(ruta: &Path, modo: u32) -> Result<(), ErrorInstancia> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(modo))
        .map_err(|error| ErrorInstancia::Io(error.to_string()))
}

/// Huella SHA-256 del certificado (sobre su DER, no sobre el PEM), en hexadecimal en
/// minúsculas con `:` entre cada byte, para que la app la muestre por un canal aparte
/// de confianza.
pub fn huella_sha256(ruta_certificado: &Path) -> Result<String, ErrorInstancia> {
    let pem = std::fs::read_to_string(ruta_certificado)
        .map_err(|error| ErrorInstancia::Io(error.to_string()))?;
    let der = pem_a_der(&pem, "CERTIFICATE")?;
    let hash = Sha256::digest(&der);
    Ok(hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":"))
}

/// Decodifica el cuerpo de un bloque PEM `-----BEGIN <etiqueta>----- ... -----END
/// <etiqueta>-----` a DER. Implementación propia del Base64 estándar (RFC 4648 §4, con
/// relleno): evita una dependencia solo para esto, igual que `git::proceso` ya hace con
/// su propio codificador.
fn pem_a_der(pem: &str, etiqueta: &str) -> Result<Vec<u8>, ErrorInstancia> {
    let inicio = format!("-----BEGIN {etiqueta}-----");
    let fin = format!("-----END {etiqueta}-----");
    let cuerpo = pem
        .split(&inicio)
        .nth(1)
        .and_then(|resto| resto.split(&fin).next())
        .ok_or_else(|| {
            ErrorInstancia::EntradaInvalida(format!("el PEM no contiene «{etiqueta}»"))
        })?;
    let compacto: String = cuerpo.chars().filter(|c| !c.is_whitespace()).collect();
    decodificar_base64(&compacto)
}

/// Decodifica Base64 estándar (con `+`/`/` y relleno `=`). Rechaza cualquier carácter
/// fuera del alfabeto y cualquier longitud que no sea múltiplo de 4.
fn decodificar_base64(entrada: &str) -> Result<Vec<u8>, ErrorInstancia> {
    const TABLA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn valor(caracter: u8) -> Result<u8, ErrorInstancia> {
        TABLA
            .iter()
            .position(|&c| c == caracter)
            .map(|posicion| posicion as u8)
            .ok_or_else(|| ErrorInstancia::EntradaInvalida("base64 no válido".to_string()))
    }

    if entrada.is_empty() || !entrada.len().is_multiple_of(4) || !entrada.is_ascii() {
        return Err(ErrorInstancia::EntradaInvalida(
            "base64 con longitud inválida".to_string(),
        ));
    }

    let bytes = entrada.as_bytes();
    let mut salida = Vec::with_capacity(entrada.len() / 4 * 3);
    for bloque in bytes.chunks(4) {
        let v0 = valor(bloque[0])?;
        let v1 = valor(bloque[1])?;
        salida.push((v0 << 2) | (v1 >> 4));

        if bloque[2] == b'=' {
            break;
        }
        let v2 = valor(bloque[2])?;
        salida.push((v1 << 4) | (v2 >> 2));

        if bloque[3] == b'=' {
            break;
        }
        let v3 = valor(bloque[3])?;
        salida.push((v2 << 6) | v3);
    }
    Ok(salida)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(v: &str) -> NombreHostInterno {
        NombreHostInterno::nuevo(v).expect("host de prueba válido")
    }

    // --- decodificar_base64 / pem_a_der (sin red ni gitea) --------------------

    #[test]
    fn decodifica_los_vectores_de_la_rfc_4648() {
        let vectores: &[(&str, &[u8])] = &[
            ("Zg==", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("Zm9vYg==", b"foob"),
            ("Zm9vYmE=", b"fooba"),
            ("Zm9vYmFy", b"foobar"),
        ];
        for (codificado, esperado) in vectores {
            assert_eq!(
                decodificar_base64(codificado).expect("decodifica"),
                *esperado,
                "{codificado}"
            );
        }
    }

    #[test]
    fn rechaza_base64_con_longitud_no_multiplo_de_cuatro() {
        assert!(decodificar_base64("abc").is_err());
        assert!(decodificar_base64("").is_err());
    }

    #[test]
    fn rechaza_caracteres_fuera_del_alfabeto() {
        assert!(decodificar_base64("ab$=").is_err());
    }

    #[test]
    fn pem_a_der_extrae_y_decodifica_el_cuerpo() {
        // «hola» en base64 es «aG9sYQ==»; el PEM lo envuelve con cabeceras y con
        // espacios/saltos de línea que deben ignorarse.
        let pem = "-----BEGIN CERTIFICATE-----\naG9s\nYQ==\n-----END CERTIFICATE-----\n";
        let der = pem_a_der(pem, "CERTIFICATE").expect("decodifica el PEM");
        assert_eq!(der, b"hola");
    }

    #[test]
    fn pem_a_der_falla_si_falta_la_etiqueta() {
        let pem = "no es un PEM";
        assert!(pem_a_der(pem, "CERTIFICATE").is_err());
    }

    #[test]
    fn huella_sha256_no_falla_con_un_certificado_de_prueba() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("cert.pem");
        // No hace falta que sea un X.509 válido: huella_sha256 solo decodifica el
        // Base64 y calcula el SHA-256, sin analizar el certificado.
        std::fs::write(
            &ruta,
            "-----BEGIN CERTIFICATE-----\naG9sYQ==\n-----END CERTIFICATE-----\n",
        )
        .expect("escribir el pem de prueba");

        let huella = huella_sha256(&ruta).expect("calcula la huella");
        assert_eq!(huella.split(':').count(), 32);
        assert!(huella.chars().all(|c| c.is_ascii_hexdigit() || c == ':'));
    }

    // --- asegurar_certificado (con un `gitea` falso: sin red) -----------------

    /// Un script que imita `gitea cert`: escribe algo en `--out`/`--keyout`, sin ser un
    /// certificado real (no hace falta para probar la idempotencia ni los permisos).
    fn crear_gitea_falso(directorio: &Path) -> PathBuf {
        let script = directorio.join("gitea-cert-falso.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -e
salida=""
clave=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --out) salida="$2"; shift 2 ;;
    --keyout) clave="$2"; shift 2 ;;
    *) shift ;;
  esac
done
echo "-----BEGIN CERTIFICATE-----" > "$salida"
echo "aG9sYQ==" >> "$salida"
echo "-----END CERTIFICATE-----" >> "$salida"
echo "clave-de-prueba-nunca-real" > "$clave"
"#,
        )
        .expect("escribir el script de gitea falso");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el script");
        script
    }

    #[tokio::test]
    async fn genera_el_certificado_con_los_permisos_esperados() {
        use std::os::unix::fs::PermissionsExt;

        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());
        let rutas = RutasCuenta::nueva(directorio.path().join("cuenta"));
        let host = host("jparga.gitmereba.internal");

        let resultado = asegurar_certificado(&ParametrosCertificado {
            binario_gitea: &binario,
            rutas: &rutas,
            host: &host,
        })
        .await
        .expect("genera el certificado");

        assert!(resultado.generado);
        assert!(resultado.ruta_certificado.exists());
        assert!(resultado.ruta_clave.exists());

        let permisos_dir = std::fs::metadata(rutas.gitea_tls())
            .expect("metadata del directorio tls")
            .permissions();
        assert_eq!(permisos_dir.mode() & 0o777, 0o700);
        let permisos_cert = std::fs::metadata(&resultado.ruta_certificado)
            .expect("metadata del certificado")
            .permissions();
        assert_eq!(permisos_cert.mode() & 0o777, 0o644);
        let permisos_clave = std::fs::metadata(&resultado.ruta_clave)
            .expect("metadata de la clave")
            .permissions();
        assert_eq!(permisos_clave.mode() & 0o777, 0o600);
    }

    #[tokio::test]
    async fn es_idempotente_si_ya_cubre_el_mismo_host() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());
        let rutas = RutasCuenta::nueva(directorio.path().join("cuenta"));
        let host = host("jparga.gitmereba.internal");
        let parametros = ParametrosCertificado {
            binario_gitea: &binario,
            rutas: &rutas,
            host: &host,
        };

        let primero = asegurar_certificado(&parametros)
            .await
            .expect("primera generación");
        assert!(primero.generado);
        let contenido_primero =
            std::fs::read_to_string(&primero.ruta_certificado).expect("leer el certificado");

        let segundo = asegurar_certificado(&parametros)
            .await
            .expect("segunda llamada no falla");
        assert!(!segundo.generado);
        let contenido_segundo =
            std::fs::read_to_string(&segundo.ruta_certificado).expect("leer el certificado");
        assert_eq!(contenido_primero, contenido_segundo);
    }

    #[tokio::test]
    async fn regenera_si_el_host_cambia() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());
        let rutas = RutasCuenta::nueva(directorio.path().join("cuenta"));

        let host1 = host("jparga.gitmereba.internal");
        asegurar_certificado(&ParametrosCertificado {
            binario_gitea: &binario,
            rutas: &rutas,
            host: &host1,
        })
        .await
        .expect("primera generación");

        let host2 = host("otro.gitmereba.internal");
        let resultado = asegurar_certificado(&ParametrosCertificado {
            binario_gitea: &binario,
            rutas: &rutas,
            host: &host2,
        })
        .await
        .expect("segunda generación");
        assert!(resultado.generado, "un host distinto debe regenerar");
    }
}
