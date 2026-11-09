//! Perfil de AppArmor opcional para el binario de Gitea.
//!
//! La app trabaja sin root y cargar un perfil lo exige, así que aquí solo se GENERA el
//! texto: nunca se carga ni se escribe en `/etc`. La persona lo instala con `sudo`. En
//! Ubuntu, AppArmor anula en silencio `ProtectSystem`, `ProtectHome`, `ReadWritePaths` y
//! `PrivateTmp` en las unidades `systemd --user`; un perfil propio es la forma de confinar
//! el acceso de Gitea a ficheros.
//!
//! El perfil se carga como root, por lo que toda ruta interpolada se valida (ante la
//! duda, se rechaza) y se escribe siempre entre comillas dobles.

use std::path::{Component, Path, PathBuf};

use super::ErrorInstancia;

/// Qué se confina: el binario de Gitea y las carpetas donde puede escribir.
#[derive(Debug, Clone, Copy)]
pub struct ParametrosPerfil<'a> {
    /// Binario de Gitea al que se adjunta el perfil.
    pub binario_gitea: &'a Path,
    /// Directorio de datos de gitmereba. Solo se valida y se cita en la cabecera: Gitea no
    /// recibe ningún permiso sobre él (el binario se concede por separado).
    pub directorio_datos: &'a Path,
    /// Carpetas de las cuentas: lo único donde Gitea puede escribir.
    pub carpetas_cuenta: &'a [PathBuf],
}

/// Valida `ruta` y la devuelve normalizada (sin barra final) y lista para ir entre
/// comillas dobles.
fn ruta_segura(ruta: &Path) -> Result<String, ErrorInstancia> {
    let invalida =
        || ErrorInstancia::RutaPerfilApparmorInvalida(format!("{:?}", ruta.to_string_lossy()));
    let texto = ruta.to_str().ok_or_else(invalida)?;
    if !ruta.is_absolute() {
        return Err(invalida());
    }
    // Lista corta y cerrada: cualquier cosa fuera de ella con significado para AppArmor
    // (comodines, alternancias, clases, escapes, separadores) o de control se rechaza.
    let prohibido = |c: char| {
        c.is_control()
            || matches!(
                c,
                '"' | '{' | '}' | '[' | ']' | '*' | '?' | '\\' | ',' | '^'
            )
    };
    if texto.chars().any(prohibido) {
        return Err(invalida());
    }
    if ruta
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalida());
    }
    let normalizada = texto.trim_end_matches('/');
    // La raíz sola concedería escritura en todo el sistema.
    if normalizada.is_empty() {
        return Err(invalida());
    }
    Ok(normalizada.to_string())
}

/// Genera el perfil (en modo `complain`: solo registra) para instalarlo a mano.
///
/// # Errors
///
/// `RutaPerfilApparmorInvalida` si alguna ruta no es absoluta o lleva caracteres con
/// significado para AppArmor.
pub fn generar_perfil(parametros: &ParametrosPerfil<'_>) -> Result<String, ErrorInstancia> {
    let binario = ruta_segura(parametros.binario_gitea)?;
    let datos = ruta_segura(parametros.directorio_datos)?;
    let carpetas = parametros
        .carpetas_cuenta
        .iter()
        .map(|carpeta| ruta_segura(carpeta))
        .collect::<Result<Vec<_>, _>>()?;

    let mut perfil = String::new();
    perfil.push_str(&format!(
        "\
# Perfil de AppArmor OPCIONAL para el Gitea de gitmereba. gitmereba no lo carga.
# Datos de gitmereba: {datos}
#
# Instalación (como administrador):
#   1. Guarda esta salida en un fichero, p. ej. gitmereba-gitea
#   2. sudo cp gitmereba-gitea /etc/apparmor.d/gitmereba-gitea
#   3. sudo apparmor_parser -r /etc/apparmor.d/gitmereba-gitea
#   4. Reinicia el Gitea de cada cuenta para que el perfil se aplique.
#
# Nace en modo complain: solo registra lo que denegaría, no bloquea nada. Tras usar la
# app un tiempo, revisa los avisos con:
#   journalctl -k | grep apparmor
# y, si no hay accesos legítimos registrados, pásalo a enforce:
#   sudo aa-enforce gitmereba-gitea
# Las carpetas de cuenta se fijan al generar el perfil: si das de alta o mueves una
# cuenta, vuelve a generarlo e instalarlo.

abi <abi/4.0>,

include <tunables/global>

profile gitmereba-gitea \"{binario}\" flags=(complain) {{
  # Base de AppArmor, resolución de nombres y certificados raíz (HTTPS saliente).
  include <abstractions/base>
  include <abstractions/nameservice>
  include <abstractions/ssl_certs>

  # Red: HTTP local y saliente (IPv4/IPv6) y sockets de Unix (p. ej. los de git).
  network inet stream,
  network inet6 stream,
  network unix stream,

  # El propio binario de Gitea: lectura y mapeo en memoria.
  \"{binario}\" mr,

  # Gitea invoca git para los repositorios; sus ayudantes viven en git-core.
  /usr/bin/git ix,
  /usr/lib/git-core/** ix,
  /etc/gitconfig r,
  /usr/share/git-core/** r,

  # Resolución de usuarios y nombres de equipo.
  /etc/hosts r,
  /etc/passwd r,

  # Estado mínimo del propio proceso, que Gitea consulta al arrancar y al supervisarse.
  owner @{{PROC}}/@{{pid}}/stat r,
  owner @{{PROC}}/@{{pid}}/status r,
  owner @{{PROC}}/@{{pid}}/cmdline r,

  # Sin capacidades de root (no se concede ninguna) y sin depurar ni montar nada.
  deny ptrace,
  deny mount,
"
    ));

    if carpetas.is_empty() {
        perfil.push_str(
            "\n  # No hay cuentas dadas de alta: no se concede escritura en ninguna carpeta.\n",
        );
    }
    for carpeta in &carpetas {
        perfil.push_str(&format!(
            "\n  # Carpeta de una cuenta: lo único donde Gitea puede leer y escribir.\n  \"{carpeta}/\" r,\n  \"{carpeta}/**\" rwk,\n"
        ));
    }
    perfil.push_str("}\n");
    Ok(perfil)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const APPARMOR_PARSER: &str = "/usr/sbin/apparmor_parser";

    fn perfil(carpetas: &[&str]) -> String {
        let carpetas: Vec<PathBuf> = carpetas.iter().map(PathBuf::from).collect();
        generar_perfil(&ParametrosPerfil {
            binario_gitea: Path::new("/home/ana/.local/share/gitmereba/bin/gitea-1.27.3"),
            directorio_datos: Path::new("/home/ana/.local/share/gitmereba"),
            carpetas_cuenta: &carpetas,
        })
        .expect("perfil")
    }

    fn reglas(perfil: &str) -> Vec<&str> {
        perfil
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    }

    #[test]
    fn el_perfil_va_en_complain_con_binario_y_carpetas() {
        let p = perfil(&["/srv/ana", "/srv/luis"]);
        assert!(p.contains("flags=(complain)"));
        assert!(!p.contains("enforce)"));
        assert!(p.contains("\"/home/ana/.local/share/gitmereba/bin/gitea-1.27.3\""));
        assert!(p.contains("\"/srv/ana/**\" rwk,"));
        assert!(p.contains("\"/srv/luis/**\" rwk,"));
        assert!(p.contains("abi <abi/4.0>,"));
        assert!(p.contains("deny ptrace,"));
        assert!(p.contains("deny mount,"));
    }

    #[test]
    fn la_cabecera_explica_la_instalacion() {
        let p = perfil(&["/srv/ana"]);
        assert!(p.starts_with('#'));
        assert!(p.contains("sudo cp"));
        assert!(p.contains("/etc/apparmor.d/gitmereba-gitea"));
        assert!(p.contains("sudo apparmor_parser -r /etc/apparmor.d/gitmereba-gitea"));
        assert!(p.contains("journalctl -k | grep apparmor"));
        assert!(p.contains("sudo aa-enforce gitmereba-gitea"));
    }

    #[test]
    fn no_hay_escritura_fuera_de_las_carpetas_ni_capacidades() {
        let p = perfil(&["/srv/ana"]);
        assert!(!p.contains("capability"));
        for regla in reglas(&p) {
            let permisos = regla.trim_end_matches(',').rsplit(' ').next().unwrap_or("");
            let escribe = !regla.starts_with("deny")
                && !regla.starts_with("include")
                && regla.ends_with(',')
                && regla.starts_with('"')
                && permisos.chars().any(|c| c == 'w' || c == 'a');
            if escribe {
                assert!(
                    regla.starts_with("\"/srv/ana/**\""),
                    "escritura fuera: {regla}"
                );
            }
        }
    }

    #[test]
    fn sin_cuentas_no_hay_reglas_de_carpeta() {
        let p = perfil(&[]);
        assert!(!p.contains("rwk"));
    }

    #[test]
    fn rutas_peligrosas_se_rechazan() {
        let casos = [
            "/srv/a\"b",
            "/srv/a\nb",
            "/srv/a\rb",
            "/srv/a{b",
            "/srv/a}b",
            "/srv/a[b",
            "/srv/a]b",
            "/srv/a*b",
            "/srv/a?b",
            "/srv/a\\b",
            "/srv/a,b",
            "/srv/a^b",
            "/srv/a\0b",
            "/srv/../etc",
            "relativa/ruta",
            "",
            "/",
            "///",
        ];
        for caso in casos {
            let r = generar_perfil(&ParametrosPerfil {
                binario_gitea: Path::new("/bin/gitea"),
                directorio_datos: Path::new("/datos"),
                carpetas_cuenta: &[PathBuf::from(caso)],
            });
            assert!(
                matches!(r, Err(ErrorInstancia::RutaPerfilApparmorInvalida(_))),
                "{caso:?} debía rechazarse"
            );
        }
    }

    #[test]
    fn binario_y_datos_tambien_se_validan() {
        let ok = [PathBuf::from("/srv/ana")];
        let a = generar_perfil(&ParametrosPerfil {
            binario_gitea: Path::new("/bin/gi*tea"),
            directorio_datos: Path::new("/datos"),
            carpetas_cuenta: &ok,
        });
        let b = generar_perfil(&ParametrosPerfil {
            binario_gitea: Path::new("/bin/gitea"),
            directorio_datos: Path::new("datos"),
            carpetas_cuenta: &ok,
        });
        assert!(a.is_err());
        assert!(b.is_err());
    }

    #[test]
    fn el_error_no_vuelca_saltos_de_linea() {
        let e = generar_perfil(&ParametrosPerfil {
            binario_gitea: Path::new("/bin/gitea"),
            directorio_datos: Path::new("/datos"),
            carpetas_cuenta: &[PathBuf::from("/a\nb")],
        })
        .unwrap_err();
        assert!(!e.to_string().contains('\n'));
    }

    #[test]
    fn rutas_con_espacios_van_entre_comillas() {
        let p = perfil(&["/srv/mi carpeta/ana/"]);
        assert!(p.contains("\"/srv/mi carpeta/ana/**\" rwk,"));
        assert!(p.contains("\"/srv/mi carpeta/ana/\" r,"));
    }

    #[test]
    fn apparmor_parser_acepta_el_perfil() {
        if !Path::new(APPARMOR_PARSER).exists() {
            eprintln!("se salta: no existe {APPARMOR_PARSER}");
            return;
        }
        for carpetas in [&["/srv/ana", "/srv/mi carpeta/luis"][..], &[][..]] {
            let temporal = tempfile::tempdir().expect("temporal");
            let fichero = temporal.path().join("gitmereba-gitea");
            std::fs::write(&fichero, perfil(carpetas)).expect("escribir");
            let salida = Command::new(APPARMOR_PARSER)
                .arg("-QK")
                .arg(&fichero)
                .output()
                .expect("ejecutar apparmor_parser");
            assert!(
                salida.status.success(),
                "apparmor_parser rechazó el perfil: {}",
                String::from_utf8_lossy(&salida.stderr)
            );
        }
    }
}
