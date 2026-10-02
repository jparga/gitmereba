//! Usuarios de Gitea para la LAN: alta, listado y baja con la CLI de `gitea`,
//! igual que `provision::crear_admin`/`admin_existe` pero para usuarios sin privilegios
//! de administración. La reconciliación de sus permisos (equipos de lectura/escritura
//! por la API) vive en `crate::cuentas::usuarios_lan`; este módulo solo habla con la CLI.

use std::path::Path;
use std::time::Duration;

use crate::modelo::Nombre;
use crate::secretos::Secreto;

use super::error::ErrorInstancia;
use super::proceso::ejecutar_gitea;
use super::provision::{LONGITUD_PASSWORD, extraer_password_generada};

const TIEMPO_LIMITE: Duration = Duration::from_secs(30);

/// Fragmento de `stderr` con el que Gitea anuncia que el usuario ya existía.
const YA_EXISTE: &str = "user already exists";
/// Fragmento de `stderr` con el que Gitea anuncia que el usuario no existía al borrarlo.
const NO_EXISTE: &str = "user does not exist";

/// Un usuario de Gitea, tal como lo devuelve [`listar_usuarios`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsuarioGitea {
    pub nombre: Nombre,
    pub correo: String,
    pub admin: bool,
}

/// Crea un usuario restringido (sin privilegios de administración) con `gitea admin user
/// create --restricted`. La contraseña la genera Gitea y se lee de su salida estándar:
/// nunca se pasa por `--password` (los argumentos de un proceso son legibles por
/// cualquier usuario del sistema en `/proc/<pid>/cmdline`).
///
/// Si el usuario ya existe, `Err(ErrorInstancia::UsuarioGiteaYaExiste)`: Gitea imprime
/// igualmente una contraseña generada en su salida estándar, pero este error nunca la
/// incluye (se detecta por `stderr`, y `ejecutar_gitea` nunca mete el `stdout` en sus
/// errores).
pub async fn crear_usuario(
    binario: &Path,
    ini: &str,
    directorio_trabajo: &Path,
    nombre: &Nombre,
    correo: &str,
) -> Result<Secreto, ErrorInstancia> {
    let longitud = LONGITUD_PASSWORD.to_string();
    let resultado = ejecutar_gitea(
        binario,
        &[
            "admin",
            "user",
            "create",
            "--config",
            ini,
            "--username",
            nombre.as_str(),
            "--email",
            correo,
            "--random-password",
            "--random-password-length",
            &longitud,
            "--restricted",
            "--must-change-password=false",
        ],
        directorio_trabajo,
        TIEMPO_LIMITE,
    )
    .await;

    match resultado {
        Ok(salida) => extraer_password_generada(&salida.stdout),
        Err(ErrorInstancia::GiteaFallo { stderr, .. }) if stderr.contains(YA_EXISTE) => {
            Err(ErrorInstancia::UsuarioGiteaYaExiste(nombre.clone()))
        }
        Err(otro) => Err(otro),
    }
}

/// `gitea admin user list` y lo interpreta como la tabla `ID Username Email IsActive
/// IsAdmin 2FA` (mismo formato que usa `provision::admin_existe`).
pub async fn listar_usuarios(
    binario: &Path,
    ini: &str,
    directorio_trabajo: &Path,
) -> Result<Vec<UsuarioGitea>, ErrorInstancia> {
    let salida = ejecutar_gitea(
        binario,
        &["admin", "user", "list", "--config", ini],
        directorio_trabajo,
        TIEMPO_LIMITE,
    )
    .await?;
    parsear_lista_usuarios(&salida.stdout)
}

/// `ID Username Email IsActive IsAdmin 2FA`: solo se usan las columnas 1 (usuario), 2
/// (correo) y 4 (administrador). Una línea sin suficientes columnas se ignora (más
/// tolerante que fallar ante un formato de tabla ligeramente distinto).
fn parsear_lista_usuarios(stdout: &str) -> Result<Vec<UsuarioGitea>, ErrorInstancia> {
    let mut resultado = Vec::new();
    for linea in stdout.lines().skip(1) {
        let columnas: Vec<&str> = linea.split_whitespace().collect();
        if columnas.len() < 5 {
            continue;
        }
        let nombre = Nombre::nuevo(columnas[1]).map_err(|error| {
            ErrorInstancia::EntradaInvalida(format!(
                "nombre de usuario inválido en la salida de «gitea admin user list»: {error}"
            ))
        })?;
        resultado.push(UsuarioGitea {
            nombre,
            correo: columnas[2].to_string(),
            admin: columnas[4].eq_ignore_ascii_case("true"),
        });
    }
    Ok(resultado)
}

/// Borra un usuario con `gitea admin user delete`. Idempotente: si no existía, `Ok(())`.
///
/// El llamador debe haber sacado antes al usuario de cualquier equipo (por la API): Gitea
/// rechaza el borrado con «user still has membership of organizations» si sigue en
/// alguno (ver `crate::cuentas::usuarios_lan::eliminar_usuario_lan`).
pub async fn borrar_usuario(
    binario: &Path,
    ini: &str,
    directorio_trabajo: &Path,
    nombre: &Nombre,
) -> Result<(), ErrorInstancia> {
    let resultado = ejecutar_gitea(
        binario,
        &[
            "admin",
            "user",
            "delete",
            "--config",
            ini,
            "--username",
            nombre.as_str(),
        ],
        directorio_trabajo,
        TIEMPO_LIMITE,
    )
    .await;

    match resultado {
        Ok(_) => Ok(()),
        Err(ErrorInstancia::GiteaFallo { stderr, .. }) if stderr.contains(NO_EXISTE) => Ok(()),
        Err(otro) => Err(otro),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    /// Un `gitea` falso que entiende `admin user create/list/delete`, sin red ni Gitea
    /// real: mismo estilo que `provision::tests::crear_gitea_falso`.
    fn crear_gitea_falso(directorio: &Path) -> std::path::PathBuf {
        let script = directorio.join("gitea-falso-usuarios.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -e
argumento_tras() {
  buscado="$1"
  shift
  prev=""
  for a in "$@"; do
    if [ "$prev" = "$buscado" ]; then
      echo "$a"
      return 0
    fi
    prev="$a"
  done
}

case "$1" in
  admin)
    case "$3" in
      create)
        for a in "$@"; do [ "$a" = "--password" ] && exit 9; done
        usuario=$(argumento_tras --username "$@")
        if [ "$usuario" = "ya-existe" ]; then
          echo "generated random password is 'zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz'"
          echo "Command error: CreateUser: user already exists [name: ya-existe]" >&2
          exit 1
        fi
        echo "generated random password is 'abcdefghijklmnopqrstuvwxyzABCDEF'"
        echo "New user '$usuario' has been successfully created!"
        ;;
      list)
        echo "ID   Username        Email                           IsActive IsAdmin 2FA"
        echo "1    gitmereba-admin gitmereba-admin@localhost.local true     true    false"
        echo "2    ana             ana@lan.gitmereba.invalid       true     false   false"
        ;;
      delete)
        usuario=$(argumento_tras --username "$@")
        if [ "$usuario" = "no-existe" ]; then
          echo "Command error: user does not exist" >&2
          exit 1
        fi
        ;;
    esac
    ;;
esac
"#,
        )
        .expect("escribir el script de gitea falso");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el script");
        script
    }

    // --- parsear_lista_usuarios ------------------------------------------------

    #[test]
    fn parsea_la_tabla_de_gitea_admin_user_list() {
        let tabla = "ID   Username   Email   IsActive IsAdmin 2FA\n\
             2    ana   ana@x.invalid true  false  false\n";
        let usuarios = parsear_lista_usuarios(tabla).expect("parsea sin fallar");
        assert_eq!(usuarios.len(), 1);
        assert_eq!(usuarios[0].nombre, nombre("ana"));
        assert_eq!(usuarios[0].correo, "ana@x.invalid");
        assert!(!usuarios[0].admin);
    }

    #[test]
    fn una_linea_sin_columnas_suficientes_se_ignora() {
        let tabla = "ID   Username   Email   IsActive IsAdmin 2FA\ncorta\n";
        let usuarios = parsear_lista_usuarios(tabla).expect("parsea sin fallar");
        assert!(usuarios.is_empty());
    }

    // --- crear_usuario ----------------------------------------------------------

    #[tokio::test]
    async fn crea_un_usuario_y_extrae_la_password_sin_pasarla_por_password() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());

        let password = crear_usuario(
            &binario,
            "app.ini",
            directorio.path(),
            &nombre("ana"),
            "ana@lan.gitmereba.invalid",
        )
        .await
        .expect("crear_usuario no falla");

        assert_eq!(password.exponer(), "abcdefghijklmnopqrstuvwxyzABCDEF");
    }

    #[tokio::test]
    async fn si_el_usuario_ya_existe_el_error_no_incluye_la_password_de_stdout() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());

        let resultado = crear_usuario(
            &binario,
            "app.ini",
            directorio.path(),
            &nombre("ya-existe"),
            "ya-existe@lan.gitmereba.invalid",
        )
        .await;

        match &resultado {
            Err(ErrorInstancia::UsuarioGiteaYaExiste(nombre_error)) => {
                assert_eq!(*nombre_error, nombre("ya-existe"));
            }
            otro => panic!("se esperaba UsuarioGiteaYaExiste, se obtuvo {otro:?}"),
        }
        let mensaje = resultado.unwrap_err().to_string();
        assert!(!mensaje.contains("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    // --- listar_usuarios ----------------------------------------------------------

    #[tokio::test]
    async fn listar_usuarios_distingue_al_administrador() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());

        let usuarios = listar_usuarios(&binario, "app.ini", directorio.path())
            .await
            .expect("listar_usuarios no falla");

        assert_eq!(usuarios.len(), 2);
        let admin = usuarios
            .iter()
            .find(|u| u.nombre == nombre("gitmereba-admin"))
            .expect("el administrador aparece en la lista");
        assert!(admin.admin);
        let lan = usuarios
            .iter()
            .find(|u| u.nombre == nombre("ana"))
            .expect("ana aparece en la lista");
        assert!(!lan.admin);
    }

    // --- borrar_usuario ----------------------------------------------------------

    #[tokio::test]
    async fn borrar_usuario_es_ok_si_no_existia() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());

        borrar_usuario(&binario, "app.ini", directorio.path(), &nombre("no-existe"))
            .await
            .expect("borrar un usuario inexistente no debe fallar");
    }

    #[tokio::test]
    async fn borrar_usuario_existente_no_falla() {
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let binario = crear_gitea_falso(directorio.path());

        borrar_usuario(&binario, "app.ini", directorio.path(), &nombre("ana"))
            .await
            .expect("borrar un usuario existente no debe fallar");
    }
}
