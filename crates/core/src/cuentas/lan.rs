//! Acceso a una cuenta desde la LAN por HTTPS con nombre `.internal`.
//!
//! Apagado por defecto y por cuenta. Al activarlo: se genera (si hace falta) un
//! certificado autofirmado, se regenera `app.ini` conservando los secretos ya
//! provisionados y se reinicia el servicio con el [`Lanzador`] de la cuenta. Al
//! desactivarlo, Gitea vuelve a escuchar solo en `127.0.0.1` por HTTP; el certificado
//! se conserva en disco (no invalida la confianza ya repartida si se reactiva).

use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};

use crate::config::{self, RutasCuenta};
use crate::instancia::{self, ParametrosCertificado, ParametrosLanAppIni};
use crate::modelo::{AccesoLan, Cuenta, Nombre, NombreHostInterno, host_lan_por_defecto};
use crate::secretos::Llavero;

use super::comun::{cargar_cuenta, ruta_binario_cacheado, usuario_actual};
use super::contexto::Contexto;
use super::error::ErrorCuentas;
use super::lanzador::{Lanzador, ParametrosLanzamiento};

/// Dirección de prueba (RFC 5737, TEST-NET-1): nunca se envía nada, solo sirve para que
/// el sistema operativo elija la ruta (y con ella la IP local) hacia la LAN.
const DESTINO_SONDA_IP_LAN: &str = "192.0.2.1:9";

/// Informe de una operación de acceso LAN: activar, desactivar o solo consultar el
/// estado. Los campos que no aplican (LAN inactiva, o IP de la LAN no detectable) son
/// `None` en vez de forzar un valor de relleno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InformeLan {
    /// URL que ven las personas: `cuenta.url_publica()`.
    pub url_publica: String,
    /// `Some` si el acceso LAN está activo tras esta operación.
    pub host: Option<NombreHostInterno>,
    /// IP del anfitrión en la LAN, detectada sin tocar la red (solo se elige la ruta).
    pub ip_lan: Option<IpAddr>,
    /// Línea para el `/etc/hosts` de OTRO equipo de la LAN: `<ip_lan>  <host>`.
    pub linea_hosts: Option<String>,
    pub huella_sha256: Option<String>,
    pub ruta_certificado: Option<PathBuf>,
    /// `git config --global http."https://<host>:<puerto>/".sslCAInfo <ruta>`.
    pub comando_git_cliente: Option<String>,
    pub regla_cortafuegos_sugerida: Option<String>,
}

/// Activa (o reconfigura, si ya estaba activo) el acceso LAN de `login`. `host` es
/// `None` para usar el valor por defecto (`<login>.gitmereba.internal`).
pub async fn exponer_lan<L: Llavero, Ln: Lanzador>(
    contexto: &Contexto<'_, L>,
    lanzador: &Ln,
    login: &Nombre,
    host: Option<NombreHostInterno>,
) -> Result<InformeLan, ErrorCuentas> {
    let mut cuenta = cargar_cuenta(contexto.rutas, login)?;
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let host = host.unwrap_or_else(|| host_lan_por_defecto(login));
    let binario = ruta_binario_cacheado(contexto.rutas);

    let resultado_cert = instancia::asegurar_certificado(&ParametrosCertificado {
        binario_gitea: &binario,
        rutas: &rutas_cuenta,
        host: &host,
    })
    .await?;

    let run_user = usuario_actual();
    instancia::regenerar_app_ini_conservando_secretos(
        &rutas_cuenta,
        cuenta.puerto,
        &run_user,
        Some(ParametrosLanAppIni {
            host: &host,
            ruta_certificado: &resultado_cert.ruta_certificado,
            ruta_clave: &resultado_cert.ruta_clave,
        }),
    )?;

    cuenta.lan = Some(AccesoLan { host: host.clone() });
    config::escribir_cuenta(&rutas_cuenta, &cuenta)?;

    let certificado_pem = std::fs::read(&resultado_cert.ruta_certificado)
        .map_err(|error| ErrorCuentas::Io(error.to_string()))?;
    reiniciar_servicio(
        contexto,
        lanzador,
        login,
        &cuenta,
        &rutas_cuenta,
        &binario,
        Some(certificado_pem),
    )
    .await?;

    contexto
        .almacen
        .auditar(Some(login), "lan.activar", &format!("host={host}"))?;

    let huella_sha256 = instancia::huella_sha256(&resultado_cert.ruta_certificado)?;
    let ip_lan = detectar_ip_lan();
    Ok(construir_informe_activo(
        &cuenta,
        &host,
        ip_lan,
        resultado_cert.ruta_certificado,
        huella_sha256,
    ))
}

/// Desactiva el acceso LAN de `login`: Gitea vuelve a `127.0.0.1` por HTTP. El
/// certificado no se borra (para no invalidar la confianza ya repartida si
/// se reactiva), así que el informe lo sigue mostrando si sigue en disco.
pub async fn ocultar_lan<L: Llavero, Ln: Lanzador>(
    contexto: &Contexto<'_, L>,
    lanzador: &Ln,
    login: &Nombre,
) -> Result<InformeLan, ErrorCuentas> {
    let mut cuenta = cargar_cuenta(contexto.rutas, login)?;
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let binario = ruta_binario_cacheado(contexto.rutas);

    let run_user = usuario_actual();
    instancia::regenerar_app_ini_conservando_secretos(
        &rutas_cuenta,
        cuenta.puerto,
        &run_user,
        None,
    )?;

    cuenta.lan = None;
    config::escribir_cuenta(&rutas_cuenta, &cuenta)?;

    reiniciar_servicio(
        contexto,
        lanzador,
        login,
        &cuenta,
        &rutas_cuenta,
        &binario,
        None,
    )
    .await?;

    contexto
        .almacen
        .auditar(Some(login), "lan.desactivar", "")?;

    let (huella_sha256, ruta_certificado) = certificado_si_existe(&rutas_cuenta);
    Ok(InformeLan {
        url_publica: cuenta.url_publica(),
        host: None,
        ip_lan: None,
        linea_hosts: None,
        huella_sha256,
        ruta_certificado,
        comando_git_cliente: None,
        regla_cortafuegos_sugerida: None,
    })
}

/// Consulta el estado del acceso LAN de `login` sin cambiar nada.
pub fn estado_lan<L: Llavero>(
    contexto: &Contexto<'_, L>,
    login: &Nombre,
) -> Result<InformeLan, ErrorCuentas> {
    let cuenta = cargar_cuenta(contexto.rutas, login)?;
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let (huella_sha256, ruta_certificado) = certificado_si_existe(&rutas_cuenta);

    match &cuenta.lan {
        Some(acceso) => {
            let ip_lan = detectar_ip_lan();
            let ruta = ruta_certificado.unwrap_or_else(|| rutas_cuenta.gitea_tls_cert());
            Ok(construir_informe_activo(
                &cuenta,
                &acceso.host,
                ip_lan,
                ruta,
                huella_sha256.unwrap_or_default(),
            ))
        }
        None => Ok(InformeLan {
            url_publica: cuenta.url_publica(),
            host: None,
            ip_lan: None,
            linea_hosts: None,
            huella_sha256,
            ruta_certificado,
            comando_git_cliente: None,
            regla_cortafuegos_sugerida: None,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
async fn reiniciar_servicio<L: Llavero, Ln: Lanzador>(
    contexto: &Contexto<'_, L>,
    lanzador: &Ln,
    login: &Nombre,
    cuenta: &Cuenta,
    rutas_cuenta: &RutasCuenta,
    binario: &Path,
    certificado_pem: Option<Vec<u8>>,
) -> Result<(), ErrorCuentas> {
    lanzador.parar(contexto.rutas, login).await?;
    let parametros_lanzamiento = ParametrosLanzamiento {
        login,
        binario_gitea: binario,
        app_ini: &rutas_cuenta.gitea_app_ini(),
        directorio_trabajo: &rutas_cuenta.gitea(),
        carpeta_cuenta: &cuenta.carpeta,
        puerto: cuenta.puerto,
        url_local: cuenta.url_gitea(),
        certificado_pem,
    };
    lanzador
        .arrancar(contexto.rutas, &parametros_lanzamiento)
        .await
}

fn certificado_si_existe(rutas_cuenta: &RutasCuenta) -> (Option<String>, Option<PathBuf>) {
    let ruta = rutas_cuenta.gitea_tls_cert();
    if ruta.exists() {
        (instancia::huella_sha256(&ruta).ok(), Some(ruta))
    } else {
        (None, None)
    }
}

fn construir_informe_activo(
    cuenta: &Cuenta,
    host: &NombreHostInterno,
    ip_lan: Option<IpAddr>,
    ruta_certificado: PathBuf,
    huella_sha256: String,
) -> InformeLan {
    let linea_hosts = ip_lan.map(|ip| linea_hosts_lan(ip, host));
    let red = ip_lan.and_then(red_sugerida);
    InformeLan {
        url_publica: cuenta.url_publica(),
        host: Some(host.clone()),
        ip_lan,
        linea_hosts,
        huella_sha256: Some(huella_sha256),
        comando_git_cliente: Some(construir_comando_git_cliente(
            host,
            cuenta.puerto,
            &ruta_certificado,
        )),
        regla_cortafuegos_sugerida: Some(sugerir_regla_cortafuegos(red.as_deref(), cuenta.puerto)),
        ruta_certificado: Some(ruta_certificado),
    }
}

/// Línea de `/etc/hosts` para que OTRO equipo de la LAN resuelva `host` al anfitrión.
/// Función pura, sin E/S.
fn linea_hosts_lan(ip: IpAddr, host: &NombreHostInterno) -> String {
    format!("{ip}  {host}")
}

/// Red `/24` sugerida a partir de la IP del anfitrión en la LAN (solo IPv4: en IPv6 no
/// hay un prefijo «tradicional» equivalente y se deja sin sugerir). Función pura.
fn red_sugerida(ip: IpAddr) -> Option<String> {
    match ip {
        IpAddr::V4(v4) => {
            let octetos = v4.octets();
            Some(format!("{}.{}.{}.0/24", octetos[0], octetos[1], octetos[2]))
        }
        IpAddr::V6(_) => None,
    }
}

/// Regla `ufw` sugerida. Función pura: si no se conoce la red, deja
/// un marcador de posición explícito en vez de inventar una IP.
fn sugerir_regla_cortafuegos(red: Option<&str>, puerto: u16) -> String {
    let red = red.unwrap_or("<red-de-tu-lan>/<prefijo>");
    format!("sudo ufw allow from {red} to any port {puerto} proto tcp")
}

/// Comando para que un cliente `git` confíe en el certificado de esta cuenta, acotado a
/// su origen. Función pura.
fn construir_comando_git_cliente(
    host: &NombreHostInterno,
    puerto: u16,
    ruta_certificado: &Path,
) -> String {
    format!(
        "git config --global http.\"https://{host}:{puerto}/\".sslCAInfo {}",
        ruta_certificado.display()
    )
}

/// Detecta la IP del anfitrión en la LAN sin enviar ningún paquete: conecta un socket
/// UDP a una dirección de prueba (RFC 5737, nunca se envía nada con UDP hasta que se
/// escribe) y lee qué IP local eligió el sistema operativo para esa ruta. `None` si no
/// hay ninguna interfaz de red disponible.
fn detectar_ip_lan() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(DESTINO_SONDA_IP_LAN).ok()?;
    socket.local_addr().ok().map(|direccion| direccion.ip())
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn host(v: &str) -> NombreHostInterno {
        NombreHostInterno::nuevo(v).expect("host de prueba válido")
    }

    // --- partes puras ----------------------------------------------------

    #[test]
    fn linea_hosts_lan_junta_ip_y_host_con_dos_espacios() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
        assert_eq!(
            linea_hosts_lan(ip, &host("jparga.gitmereba.internal")),
            "192.168.1.42  jparga.gitmereba.internal"
        );
    }

    #[test]
    fn red_sugerida_calcula_el_24_a_partir_de_una_ipv4() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
        assert_eq!(red_sugerida(ip).as_deref(), Some("192.168.1.0/24"));
    }

    #[test]
    fn red_sugerida_no_inventa_nada_para_ipv6() {
        let ip: IpAddr = "::1".parse().expect("ipv6 válida");
        assert_eq!(red_sugerida(ip), None);
    }

    #[test]
    fn regla_de_cortafuegos_usa_la_red_conocida() {
        assert_eq!(
            sugerir_regla_cortafuegos(Some("192.168.1.0/24"), 3999),
            "sudo ufw allow from 192.168.1.0/24 to any port 3999 proto tcp"
        );
    }

    #[test]
    fn regla_de_cortafuegos_deja_un_marcador_sin_red_conocida() {
        let regla = sugerir_regla_cortafuegos(None, 3999);
        assert!(regla.contains("to any port 3999 proto tcp"));
        assert!(!regla.contains("192.168"));
    }

    #[test]
    fn comando_git_cliente_acota_la_confianza_al_origen_exacto() {
        let comando = construir_comando_git_cliente(
            &host("jparga.gitmereba.internal"),
            3999,
            &PathBuf::from("/cuentas/jparga/gitea/tls/cert.pem"),
        );
        assert_eq!(
            comando,
            "git config --global http.\"https://jparga.gitmereba.internal:3999/\".sslCAInfo /cuentas/jparga/gitea/tls/cert.pem"
        );
        assert!(!comando.contains("sslVerify"));
    }

    #[test]
    fn detectar_ip_lan_no_entra_en_panico() {
        // No se puede afirmar nada sobre el valor (depende de la máquina que ejecute el
        // test), solo que no falla ni se cuelga.
        let _ = detectar_ip_lan();
    }

    // --- exponer_lan / ocultar_lan / estado_lan (con dobles y un `gitea` falso) ----

    use crate::almacen::Almacen;
    use crate::config::Rutas;
    use crate::cuentas::dobles::LanzadorDoble;
    use crate::instancia::{ParametrosAppIni, escribir_app_ini, generar_app_ini};
    use crate::modelo::Alcance;
    use crate::secretos::{LlaveroEnMemoria, Secreto};

    /// Un script que imita lo mínimo de `gitea` que usa este módulo: `cert --out
    /// --keyout` (ver `instancia::certificado`, que tiene su propio doble equivalente).
    fn crear_gitea_falso(directorio: &Path) -> PathBuf {
        let script = directorio.join("gitea-falso.sh");
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
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el script");
        script
    }

    /// Prepara una cuenta completa (índice, `gitmereba.toml`, `app.ini` ya
    /// provisionado y el binario de `gitea` cacheado en la ruta que espera este
    /// módulo) para poder probar `exponer_lan`/`ocultar_lan` sin un Gitea real.
    fn preparar_cuenta(raiz: &Path, login: &str, puerto: u16) -> (Rutas, RutasCuenta, Cuenta) {
        let rutas = Rutas::con_raiz(raiz);
        let carpeta = raiz.join("cuenta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);

        let cuenta = Cuenta {
            login: nombre(login),
            carpeta: carpeta.clone(),
            puerto,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        };
        config::escribir_cuenta(&rutas_cuenta, &cuenta).expect("escribir gitmereba.toml");

        let mut indice = config::leer_indice_cuentas(&rutas).expect("leer índice");
        indice.cuentas.insert(
            login.to_string(),
            config::EntradaCuenta {
                carpeta: carpeta.clone(),
                puerto,
            },
        );
        config::escribir_indice_cuentas(&rutas, &indice).expect("escribir índice");

        // «Provisiona» un app.ini mínimo, como si `instancia::provisionar` ya se
        // hubiera ejecutado: es lo que `regenerar_app_ini_conservando_secretos` espera
        // encontrar para leer los cuatro secretos.
        let secretos = [
            Secreto::nuevo("secreto-clave-de-prueba"),
            Secreto::nuevo("secreto-interno-de-prueba"),
            Secreto::nuevo("secreto-jwt-de-prueba"),
            Secreto::nuevo("secreto-lfs-de-prueba"),
        ];
        let parametros = ParametrosAppIni {
            rutas: &rutas_cuenta,
            puerto,
            run_user: "pruebas",
            secret_key: &secretos[0],
            internal_token: &secretos[1],
            jwt_secret: &secretos[2],
            lfs_jwt_secret: &secretos[3],
            modo_pruebas: false,
            acceso_lan: None,
        };
        let ini = generar_app_ini(&parametros).expect("genera el ini inicial");
        escribir_app_ini(&rutas_cuenta, &ini).expect("escribe el ini inicial");

        // El binario «cacheado» que `ruta_binario_cacheado` espera encontrar.
        let directorio_bin = rutas.directorio_bin();
        std::fs::create_dir_all(&directorio_bin).expect("crear el directorio de binarios");
        let destino = directorio_bin.join(format!("gitea-{}", instancia::VERSION_GITEA));
        let origen = crear_gitea_falso(raiz);
        std::fs::copy(&origen, &destino).expect("copiar el binario falso al caché");
        std::fs::set_permissions(&destino, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el binario cacheado");

        (rutas, rutas_cuenta, cuenta)
    }

    #[tokio::test]
    async fn exponer_lan_activa_con_el_host_por_defecto_y_reinicia_el_servicio() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, rutas_cuenta, _cuenta) = preparar_cuenta(temporal.path(), "jparga", 33099);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();
        let login = nombre("jparga");

        let informe = exponer_lan(&contexto, &lanzador, &login, None)
            .await
            .expect("exponer_lan no falla");

        assert_eq!(
            informe.host.as_ref().map(|h| h.as_str()),
            Some("jparga.gitmereba.internal")
        );
        assert_eq!(
            informe.url_publica,
            "https://jparga.gitmereba.internal:33099"
        );
        assert!(informe.huella_sha256.is_some());
        assert!(rutas_cuenta.gitea_tls_cert().exists());
        assert_eq!(
            std::fs::metadata(rutas_cuenta.gitea_tls_key())
                .expect("metadata de la clave")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let cuenta_guardada = config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
        assert!(cuenta_guardada.lan.is_some());

        // El servicio se ha parado y vuelto a arrancar (idempotente en cuanto a
        // secuencia: para, luego arranca).
        assert_eq!(lanzador.paradas.lock().unwrap().len(), 1);
        assert_eq!(lanzador.arrancadas.lock().unwrap().len(), 1);

        let ini = std::fs::read_to_string(rutas_cuenta.gitea_app_ini()).expect("leer app.ini");
        assert!(ini.contains("PROTOCOL = https"));
        assert!(ini.contains("SECRET_KEY = secreto-clave-de-prueba"));

        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "lan.activar"));
    }

    #[tokio::test]
    async fn exponer_lan_admite_un_host_explicito() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta, _cuenta) = preparar_cuenta(temporal.path(), "jparga", 33100);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();
        let login = nombre("jparga");

        let informe = exponer_lan(&contexto, &lanzador, &login, Some(host("otro.internal")))
            .await
            .expect("exponer_lan no falla");

        assert_eq!(
            informe.host.map(|h| h.to_string()),
            Some("otro.internal".to_string())
        );
    }

    #[tokio::test]
    async fn ocultar_lan_vuelve_a_http_local_y_conserva_el_certificado() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, rutas_cuenta, _cuenta) = preparar_cuenta(temporal.path(), "jparga", 33101);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();
        let login = nombre("jparga");

        exponer_lan(&contexto, &lanzador, &login, None)
            .await
            .expect("exponer_lan no falla");
        let informe = ocultar_lan(&contexto, &lanzador, &login)
            .await
            .expect("ocultar_lan no falla");

        assert_eq!(informe.host, None);
        assert_eq!(informe.url_publica, "http://127.0.0.1:33101");
        // El certificado se conserva: sigue existiendo y se sigue informando.
        assert!(rutas_cuenta.gitea_tls_cert().exists());
        assert!(informe.huella_sha256.is_some());

        let cuenta_guardada = config::leer_cuenta(&rutas_cuenta).expect("leer gitmereba.toml");
        assert!(cuenta_guardada.lan.is_none());

        let ini = std::fs::read_to_string(rutas_cuenta.gitea_app_ini()).expect("leer app.ini");
        assert!(ini.contains("PROTOCOL = http\n"));
        assert!(ini.contains("SECRET_KEY = secreto-clave-de-prueba"));

        assert_eq!(lanzador.paradas.lock().unwrap().len(), 2);
        assert_eq!(lanzador.arrancadas.lock().unwrap().len(), 2);

        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "lan.desactivar"));
    }

    #[tokio::test]
    async fn estado_lan_sin_activar_no_toca_nada() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta, _cuenta) = preparar_cuenta(temporal.path(), "jparga", 33102);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let login = nombre("jparga");

        let informe = estado_lan(&contexto, &login).expect("estado_lan no falla");

        assert_eq!(informe.host, None);
        assert_eq!(informe.url_publica, "http://127.0.0.1:33102");
    }

    #[tokio::test]
    async fn estado_lan_activo_refleja_el_host_configurado() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta, _cuenta) = preparar_cuenta(temporal.path(), "jparga", 33103);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();
        let login = nombre("jparga");

        exponer_lan(&contexto, &lanzador, &login, None)
            .await
            .expect("exponer_lan no falla");

        let informe = estado_lan(&contexto, &login).expect("estado_lan no falla");
        assert_eq!(
            informe.host.map(|h| h.to_string()),
            Some("jparga.gitmereba.internal".to_string())
        );
    }
}
