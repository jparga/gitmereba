//! Usuarios de Gitea para la LAN: personas de la LAN con LECTURA sobre los
//! mirrors y ESCRITURA sobre las organizaciones de contingencia (`contingencia-*`),
//! mediante dos equipos de Gitea (`lan-lectura`, `lan-escritura`) que se reconcilian
//! automáticamente tras cada sincronización periódica y cada activación de contingencia.
//!
//! El usuario de Gitea lo crea/borra la CLI (`crate::instancia::usuarios`); la
//! pertenencia a los equipos se gestiona por la API (`crate::gitea::ApiGitea`). La
//! contraseña generada al crear un usuario se devuelve una única vez: no se guarda en el
//! llavero ni en ningún fichero.

use std::collections::BTreeSet;

use crate::config::RutasCuenta;
use crate::contingencia::es_org_contingencia;
use crate::gitea::{ApiGitea, DefinicionEquipo, PermisoEquipo};
use crate::instancia::{self, NOMBRE_ADMIN_GITEA};
use crate::modelo::Nombre;
use crate::secretos::{Llavero, Secreto};

use super::comun::{cargar_cuenta, ruta_binario_cacheado};
use super::contexto::Contexto;
use super::error::ErrorCuentas;

/// Equipo con LECTURA sobre las organizaciones normales (los mirrors).
pub const EQUIPO_LECTURA: &str = "lan-lectura";
/// Equipo con ESCRITURA sobre las organizaciones de contingencia (`contingencia-*`).
pub const EQUIPO_ESCRITURA: &str = "lan-escritura";

/// Correo con el que se crean los usuarios de la LAN: nunca reciben avisos, así que un
/// dominio `.invalid` (RFC 2606) basta.
const DOMINIO_CORREO_LAN: &str = "lan.gitmereba.invalid";

/// Un usuario de la LAN recién creado. La contraseña se devuelve una única vez: repetir
/// la operación (o consultar `listar_usuarios_lan`) nunca vuelve a mostrarla.
#[derive(Debug)]
pub struct UsuarioLanCreado {
    pub nombre: Nombre,
    pub password: Secreto,
}

/// Un usuario de la LAN (sin privilegios de administración) tal como lo ve
/// [`listar_usuarios_lan`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsuarioLan {
    pub nombre: Nombre,
}

/// Equipo que le corresponde a `org`: escritura si es una organización de contingencia,
/// lectura si no (los mirrors).
fn equipo_de(org: &Nombre) -> &'static str {
    if es_org_contingencia(org) {
        EQUIPO_ESCRITURA
    } else {
        EQUIPO_LECTURA
    }
}

fn definicion_de(org: &Nombre) -> DefinicionEquipo {
    if es_org_contingencia(org) {
        DefinicionEquipo {
            nombre: EQUIPO_ESCRITURA,
            permiso: PermisoEquipo::Escritura,
        }
    } else {
        DefinicionEquipo {
            nombre: EQUIPO_LECTURA,
            permiso: PermisoEquipo::Lectura,
        }
    }
}

/// Reconcilia los equipos LAN de todas las organizaciones de esta instancia: cada
/// organización de contingencia recibe (si hace falta) su equipo de escritura, cada
/// organización normal su equipo de lectura, y ambos se completan con todos los usuarios
/// LAN conocidos.
///
/// Los usuarios LAN son la UNIÓN de los miembros ya presentes en los equipos LAN de
/// cualquier organización, más `extra` (típicamente el usuario recién creado, en
/// [`crear_usuario_lan`]). La búsqueda de equipos existentes ([`ApiGitea::buscar_equipo`])
/// nunca crea nada: si no hay ningún usuario LAN (ni ya conocido, ni en `extra`), esta
/// función no crea ningún equipo y devuelve `0` sin escribir nada en Gitea.
///
/// Devuelve cuántas altas de miembro ha hecho (para poder informar o depurar).
pub async fn reconciliar_permisos_lan<T: ApiGitea>(
    gitea: &T,
    extra: &[Nombre],
) -> Result<usize, ErrorCuentas> {
    let organizaciones = gitea.organizaciones().await?;

    let mut usuarios_lan: BTreeSet<Nombre> = extra.iter().cloned().collect();
    for org in &organizaciones {
        if let Some(id) = gitea.buscar_equipo(org, equipo_de(org)).await? {
            usuarios_lan.extend(gitea.miembros_equipo(id).await?);
        }
    }

    if usuarios_lan.is_empty() {
        return Ok(0);
    }

    let mut altas = 0usize;
    for org in &organizaciones {
        let id = gitea.asegurar_equipo(org, &definicion_de(org)).await?;
        let miembros_actuales: BTreeSet<Nombre> =
            gitea.miembros_equipo(id).await?.into_iter().collect();
        for usuario in &usuarios_lan {
            if !miembros_actuales.contains(usuario) {
                gitea.anadir_miembro_equipo(id, usuario).await?;
                altas += 1;
            }
        }
    }

    Ok(altas)
}

/// `true` si `nombre` no puede ser un usuario de la LAN: el administrador de Gitea, o un
/// nombre con la forma de una organización de contingencia.
fn nombre_no_valido_para_lan(nombre: &Nombre) -> bool {
    nombre.as_str() == NOMBRE_ADMIN_GITEA || es_org_contingencia(nombre)
}

/// Da de alta a `nombre` como usuario de la LAN de la cuenta `login`: crea el usuario
/// restringido en Gitea (correo `<nombre>@lan.gitmereba.invalid`) y reconcilia sus
/// permisos ([`reconciliar_permisos_lan`]) para que quede en todos los equipos LAN.
///
/// Rechaza el nombre del administrador y cualquier nombre con forma de organización de
/// contingencia ([`ErrorCuentas::UsuarioLanNoValido`]). Si la reconciliación falla
/// después de crear el usuario, se borra el usuario recién creado (mejor esfuerzo: un
/// fallo al borrarlo solo se registra con `tracing::warn!`) para que reintentar la
/// operación funcione limpio, y se devuelve el error original.
pub async fn crear_usuario_lan<L: Llavero, T: ApiGitea>(
    contexto: &Contexto<'_, L>,
    gitea: &T,
    login: &Nombre,
    nombre: &Nombre,
) -> Result<UsuarioLanCreado, ErrorCuentas> {
    if nombre_no_valido_para_lan(nombre) {
        return Err(ErrorCuentas::UsuarioLanNoValido(nombre.clone()));
    }

    let cuenta = cargar_cuenta(contexto.rutas, login)?;
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let binario = ruta_binario_cacheado(contexto.rutas);
    let ini = mostrar(&rutas_cuenta.gitea_app_ini());
    let directorio_trabajo = rutas_cuenta.gitea();
    let correo = format!("{}@{DOMINIO_CORREO_LAN}", nombre.as_str());

    let password = instancia::crear_usuario(&binario, &ini, &directorio_trabajo, nombre, &correo)
        .await
        .map_err(|error| mapear_error_creacion(nombre, error))?;

    if let Err(error) = reconciliar_permisos_lan(gitea, std::slice::from_ref(nombre)).await {
        if let Err(error_borrado) =
            instancia::borrar_usuario(&binario, &ini, &directorio_trabajo, nombre).await
        {
            tracing::warn!(
                usuario = %nombre,
                error = %error_borrado,
                "no se pudo deshacer la creación de un usuario LAN tras fallar su reconciliación"
            );
        }
        return Err(error);
    }

    contexto.almacen.auditar(
        Some(login),
        "lan.usuario.crear",
        &format!("usuario={nombre}"),
    )?;

    Ok(UsuarioLanCreado {
        nombre: nombre.clone(),
        password,
    })
}

fn mapear_error_creacion(nombre: &Nombre, error: crate::instancia::ErrorInstancia) -> ErrorCuentas {
    match error {
        crate::instancia::ErrorInstancia::UsuarioGiteaYaExiste(_) => {
            ErrorCuentas::UsuarioLanYaExiste(nombre.clone())
        }
        otro => ErrorCuentas::from(otro),
    }
}

/// Usuarios de la LAN de `login`: todos los usuarios de Gitea que no son administradores,
/// ordenados por nombre.
pub async fn listar_usuarios_lan<L: Llavero>(
    contexto: &Contexto<'_, L>,
    login: &Nombre,
) -> Result<Vec<UsuarioLan>, ErrorCuentas> {
    let cuenta = cargar_cuenta(contexto.rutas, login)?;
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let binario = ruta_binario_cacheado(contexto.rutas);
    let ini = mostrar(&rutas_cuenta.gitea_app_ini());
    let directorio_trabajo = rutas_cuenta.gitea();

    let usuarios = instancia::listar_usuarios(&binario, &ini, &directorio_trabajo).await?;
    let mut lan: Vec<UsuarioLan> = usuarios
        .into_iter()
        .filter(|usuario| !usuario.admin)
        .map(|usuario| UsuarioLan {
            nombre: usuario.nombre,
        })
        .collect();
    lan.sort();
    Ok(lan)
}

/// Da de baja a `nombre` de la LAN de `login`: lo quita de todos los equipos LAN de todas
/// las organizaciones (Gitea rechaza el borrado de un usuario que siga en algún equipo) y
/// borra el usuario con la CLI. Rechaza al administrador
/// ([`ErrorCuentas::UsuarioLanNoValido`]) y a quien no es un usuario de la LAN
/// ([`ErrorCuentas::UsuarioLanNoExiste`]).
pub async fn eliminar_usuario_lan<L: Llavero, T: ApiGitea>(
    contexto: &Contexto<'_, L>,
    gitea: &T,
    login: &Nombre,
    nombre: &Nombre,
) -> Result<(), ErrorCuentas> {
    if nombre.as_str() == NOMBRE_ADMIN_GITEA {
        return Err(ErrorCuentas::UsuarioLanNoValido(nombre.clone()));
    }

    let cuenta = cargar_cuenta(contexto.rutas, login)?;
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let binario = ruta_binario_cacheado(contexto.rutas);
    let ini = mostrar(&rutas_cuenta.gitea_app_ini());
    let directorio_trabajo = rutas_cuenta.gitea();

    // Sin esta comprobación, un nombre mal escrito diría «eliminado» y quedaría auditado.
    let existe = instancia::listar_usuarios(&binario, &ini, &directorio_trabajo)
        .await?
        .iter()
        .any(|usuario| !usuario.admin && &usuario.nombre == nombre);
    if !existe {
        return Err(ErrorCuentas::UsuarioLanNoExiste(nombre.clone()));
    }

    for org in gitea.organizaciones().await? {
        if let Some(id) = gitea.buscar_equipo(&org, equipo_de(&org)).await? {
            gitea.quitar_miembro_equipo(id, nombre).await?;
        }
    }

    instancia::borrar_usuario(&binario, &ini, &directorio_trabajo, nombre).await?;

    contexto.almacen.auditar(
        Some(login),
        "lan.usuario.eliminar",
        &format!("usuario={nombre}"),
    )?;
    Ok(())
}

fn mostrar(ruta: &std::path::Path) -> String {
    ruta.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::config::{self, Rutas};
    use crate::cuentas::dobles::GiteaDoble;
    use crate::modelo::{Alcance, Cuenta};
    use crate::secretos::LlaveroEnMemoria;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    // --- reconciliar_permisos_lan (con el doble en memoria) --------------------

    #[tokio::test]
    async fn sin_usuarios_lan_no_crea_ningun_equipo() {
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["acme", "contingencia-acme"]);

        let altas = reconciliar_permisos_lan(&gitea, &[])
            .await
            .expect("no falla");

        assert_eq!(altas, 0);
        assert_eq!(gitea.miembros_de("acme", EQUIPO_LECTURA), None);
        assert_eq!(
            gitea.miembros_de("contingencia-acme", EQUIPO_ESCRITURA),
            None
        );
    }

    #[tokio::test]
    async fn con_extra_crea_los_equipos_y_anade_al_usuario() {
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["acme", "contingencia-acme"]);

        let altas = reconciliar_permisos_lan(&gitea, &[nombre("ana")])
            .await
            .expect("no falla");

        assert_eq!(altas, 2); // una alta por equipo (lectura y escritura).
        assert_eq!(
            gitea.miembros_de("acme", EQUIPO_LECTURA),
            Some(vec![nombre("ana")])
        );
        assert_eq!(
            gitea.miembros_de("contingencia-acme", EQUIPO_ESCRITURA),
            Some(vec![nombre("ana")])
        );
    }

    #[tokio::test]
    async fn una_organizacion_nueva_recibe_su_equipo_y_los_usuarios_lan_ya_conocidos() {
        let gitea = GiteaDoble::nueva();
        // «ana» ya es miembro del equipo de lectura de una organización existente.
        gitea.con_organizaciones(&["acme"]);
        reconciliar_permisos_lan(&gitea, &[nombre("ana")])
            .await
            .expect("primera reconciliación");

        // Aparece una organización nueva (p. ej. tras un alta o una nueva contingencia).
        gitea.con_organizaciones(&["acme", "contingencia-nueva"]);
        let altas = reconciliar_permisos_lan(&gitea, &[])
            .await
            .expect("no falla");

        assert_eq!(altas, 1, "solo la organización nueva necesita el alta");
        assert_eq!(
            gitea.miembros_de("contingencia-nueva", EQUIPO_ESCRITURA),
            Some(vec![nombre("ana")])
        );
    }

    #[tokio::test]
    async fn no_repite_altas_ya_hechas() {
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["acme"]);
        reconciliar_permisos_lan(&gitea, &[nombre("ana")])
            .await
            .expect("primera reconciliación");

        let altas = reconciliar_permisos_lan(&gitea, &[])
            .await
            .expect("no falla");
        assert_eq!(altas, 0);
    }

    // --- crear_usuario_lan / listar_usuarios_lan / eliminar_usuario_lan --------

    /// Un `gitea` falso que entiende lo mínimo de `admin user create/list/delete` para
    /// poder probar sin un Gitea real (mismo estilo que `cuentas::lan::tests`).
    fn crear_gitea_falso_cli(directorio: &Path) -> PathBuf {
        let script = directorio.join("gitea-falso-usuarios-lan.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -e
usuarios_creados="$GITEA_WORK_DIR/.usuarios-creados"
touch "$usuarios_creados"
argumento_tras() {
  buscado="$1"
  shift
  prev=""
  for a in "$@"; do
    if [ "$prev" = "$buscado" ]; then echo "$a"; return 0; fi
    prev="$a"
  done
}
case "$1" in
  admin)
    case "$3" in
      create)
        usuario=$(argumento_tras --username "$@")
        if grep -qx "$usuario" "$usuarios_creados" 2>/dev/null; then
          echo "generated random password is 'zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz'"
          echo "Command error: CreateUser: user already exists [name: $usuario]" >&2
          exit 1
        fi
        echo "$usuario" >> "$usuarios_creados"
        echo "generated random password is 'abcdefghijklmnopqrstuvwxyzABCDEF'"
        ;;
      list)
        echo "ID   Username        Email                           IsActive IsAdmin 2FA"
        echo "1    gitmereba-admin gitmereba-admin@localhost.local true     true    false"
        while read -r usuario; do
          [ -z "$usuario" ] && continue
          echo "9    $usuario        $usuario@lan.gitmereba.invalid  true     false   false"
        done < "$usuarios_creados"
        ;;
      delete)
        usuario=$(argumento_tras --username "$@")
        if ! grep -qx "$usuario" "$usuarios_creados" 2>/dev/null; then
          echo "Command error: user does not exist" >&2
          exit 1
        fi
        grep -vx "$usuario" "$usuarios_creados" > "$usuarios_creados.tmp" || true
        mv "$usuarios_creados.tmp" "$usuarios_creados"
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

    /// Prepara una cuenta (índice, `gitmereba.toml`, `app.ini` y el binario de `gitea`
    /// cacheado en la ruta que espera `ruta_binario_cacheado`), como en `cuentas::lan`.
    fn preparar_cuenta(raiz: &Path, login: &str, puerto: u16) -> (Rutas, RutasCuenta) {
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

        std::fs::create_dir_all(rutas_cuenta.gitea()).expect("crear el directorio de gitea");

        let directorio_bin = rutas.directorio_bin();
        std::fs::create_dir_all(&directorio_bin).expect("crear el directorio de binarios");
        let destino = directorio_bin.join(format!("gitea-{}", instancia::VERSION_GITEA));
        let origen = crear_gitea_falso_cli(raiz);
        std::fs::copy(&origen, &destino).expect("copiar el binario falso al caché");
        std::fs::set_permissions(&destino, std::fs::Permissions::from_mode(0o700))
            .expect("marcar ejecutable el binario cacheado");

        (rutas, rutas_cuenta)
    }

    #[tokio::test]
    async fn crear_usuario_lan_rechaza_al_administrador_y_a_contingencia() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33200);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        let login = nombre("jparga");

        let resultado_admin =
            crear_usuario_lan(&contexto, &gitea, &login, &nombre(NOMBRE_ADMIN_GITEA)).await;
        assert!(matches!(
            resultado_admin,
            Err(ErrorCuentas::UsuarioLanNoValido(_))
        ));

        let resultado_contingencia =
            crear_usuario_lan(&contexto, &gitea, &login, &nombre("contingencia-x")).await;
        assert!(matches!(
            resultado_contingencia,
            Err(ErrorCuentas::UsuarioLanNoValido(_))
        ));
    }

    #[tokio::test]
    async fn crear_usuario_lan_lo_crea_y_lo_reconcilia() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33201);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["jparga"]);
        let login = nombre("jparga");

        let creado = crear_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
            .await
            .expect("crear_usuario_lan no falla");

        assert_eq!(creado.nombre, nombre("ana"));
        assert_eq!(creado.password.exponer().len(), 32);
        assert_eq!(
            gitea.miembros_de("jparga", EQUIPO_LECTURA),
            Some(vec![nombre("ana")])
        );

        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "lan.usuario.crear"));
        assert!(
            auditoria
                .iter()
                .all(|e| !e.detalle.contains(creado.password.exponer()))
        );
    }

    #[tokio::test]
    async fn crear_usuario_lan_ya_existente_da_el_error_especifico() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33202);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        let login = nombre("jparga");

        crear_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
            .await
            .expect("primera creación no falla");
        let resultado = crear_usuario_lan(&contexto, &gitea, &login, &nombre("ana")).await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::UsuarioLanYaExiste(_))
        ));
    }

    #[tokio::test]
    async fn listar_usuarios_lan_excluye_al_administrador() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33203);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        let login = nombre("jparga");
        crear_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
            .await
            .expect("crear_usuario_lan no falla");
        crear_usuario_lan(&contexto, &gitea, &login, &nombre("bea"))
            .await
            .expect("crear_usuario_lan no falla");

        let lan = listar_usuarios_lan(&contexto, &login)
            .await
            .expect("listar_usuarios_lan no falla");

        assert_eq!(
            lan,
            vec![
                UsuarioLan {
                    nombre: nombre("ana")
                },
                UsuarioLan {
                    nombre: nombre("bea")
                },
            ]
        );
    }

    #[tokio::test]
    async fn eliminar_usuario_lan_lo_quita_de_los_equipos_y_lo_borra() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33204);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        gitea.con_organizaciones(&["jparga"]);
        let login = nombre("jparga");
        crear_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
            .await
            .expect("crear_usuario_lan no falla");

        eliminar_usuario_lan(&contexto, &gitea, &login, &nombre("ana"))
            .await
            .expect("eliminar_usuario_lan no falla");

        assert_eq!(gitea.miembros_de("jparga", EQUIPO_LECTURA), Some(vec![]));
        let lan = listar_usuarios_lan(&contexto, &login)
            .await
            .expect("listar_usuarios_lan no falla");
        assert!(lan.is_empty());

        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "lan.usuario.eliminar"));
    }

    #[tokio::test]
    async fn eliminar_un_usuario_que_no_existe_falla_y_no_se_audita() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33206);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        let login = nombre("jparga");

        let resultado = eliminar_usuario_lan(&contexto, &gitea, &login, &nombre("nadie")).await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::UsuarioLanNoExiste(_))
        ));
        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().all(|e| e.accion != "lan.usuario.eliminar"));
    }

    #[tokio::test]
    async fn eliminar_usuario_lan_rechaza_al_administrador() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, _rutas_cuenta) = preparar_cuenta(temporal.path(), "jparga", 33205);
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let gitea = GiteaDoble::nueva();
        let login = nombre("jparga");

        let resultado =
            eliminar_usuario_lan(&contexto, &gitea, &login, &nombre(NOMBRE_ADMIN_GITEA)).await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::UsuarioLanNoValido(_))
        ));
    }
}
