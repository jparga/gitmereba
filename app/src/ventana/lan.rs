//! Comandos del acceso desde la red local (`lan_estado`, `lan_activar`, `lan_desactivar`):
//! envoltorios de `core::cuentas::{estado_lan, exponer_lan, ocultar_lan}`, y de sus usuarios
//! (`lan_usuarios_listar`, `lan_usuario_crear`, `lan_usuario_eliminar`).

use gitmereba_core::cuentas::{self, ErrorCuentas, InformeLan, LanzadorSystemd};
use gitmereba_core::gitea::ClienteGitea;
use gitmereba_core::modelo::{Cuenta, Nombre, NombreHostInterno, host_lan_por_defecto};
use gitmereba_core::secretos::{ClaveSecreto, Llavero};
use serde::Serialize;
use tauri::State;

use super::error::ErrorUi;
use super::estado::{EstadoApp, Recursos, en_hilo};

/// Forma JSON de [`InformeLan`] con el acceso activo (contrato con la interfaz). Nada de esto
/// es secreto: el certificado es público y la clave privada nunca sale de `core`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InformeLanDto {
    pub url_publica: String,
    pub host: String,
    pub ip_lan: Option<String>,
    pub linea_hosts: String,
    pub huella_sha256: String,
    pub ruta_certificado: String,
    pub comando_git_cliente: String,
    pub regla_cortafuegos_sugerida: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EstadoLanDto {
    pub activo: bool,
    pub host_sugerido: String,
    pub informe: Option<InformeLanDto>,
}

/// `None` si el informe es de una cuenta sin acceso LAN. Sin IP detectada, las líneas que
/// la necesitan llevan un hueco visible para que el usuario lo sustituya.
fn informe_dto(informe: InformeLan) -> Option<InformeLanDto> {
    let host = informe.host?.to_string();
    Some(InformeLanDto {
        url_publica: informe.url_publica,
        linea_hosts: informe
            .linea_hosts
            .unwrap_or_else(|| format!("<IP de este equipo>  {host}")),
        host,
        ip_lan: informe.ip_lan.map(|ip| ip.to_string()),
        huella_sha256: informe.huella_sha256.unwrap_or_default(),
        ruta_certificado: informe
            .ruta_certificado
            .map(|ruta| ruta.display().to_string())
            .unwrap_or_default(),
        comando_git_cliente: informe.comando_git_cliente.unwrap_or_default(),
        regla_cortafuegos_sugerida: informe.regla_cortafuegos_sugerida.unwrap_or_default(),
    })
}

fn login_valido(login: &str) -> Result<Nombre, ErrorUi> {
    Nombre::nuevo(login).map_err(|_| ErrorUi::cuenta_no_encontrada(login))
}

#[tauri::command]
pub async fn lan_estado(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<EstadoLanDto, ErrorUi> {
    tracing::debug!(comando = "lan_estado");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _) = recursos.cuenta(&login)?;
        let informe = cuentas::estado_lan(&recursos.contexto(), &cuenta.login)
            .map_err(|error| ErrorUi::de(&error))?;
        let informe = informe_dto(informe);
        Ok(EstadoLanDto {
            activo: informe.is_some(),
            host_sugerido: host_lan_por_defecto(&cuenta.login).to_string(),
            informe,
        })
    })
    .await
}

#[tauri::command]
pub async fn lan_activar(
    login: String,
    host: Option<String>,
    estado: State<'_, EstadoApp>,
) -> Result<InformeLanDto, ErrorUi> {
    tracing::debug!(comando = "lan_activar");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let login = login_valido(&login)?;
        let host = host
            .map(|texto| NombreHostInterno::nuevo(texto.trim()))
            .transpose()
            .map_err(|error| ErrorUi::nuevo("host_invalido", error.to_string()))?;
        let recursos = Recursos::abrir(&rutas)?;
        recursos.cuenta(login.as_str())?;
        let informe = cuentas::exponer_lan(&recursos.contexto(), &LanzadorSystemd, &login, host)
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        informe_dto(informe).ok_or_else(|| ErrorUi::interno("el acceso LAN no quedó activo"))
    })
    .await
}

#[tauri::command]
pub async fn lan_desactivar(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<serde_json::Value, ErrorUi> {
    tracing::debug!(comando = "lan_desactivar");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let login = login_valido(&login)?;
        let recursos = Recursos::abrir(&rutas)?;
        recursos.cuenta(login.as_str())?;
        cuentas::ocultar_lan(&recursos.contexto(), &LanzadorSystemd, &login)
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(serde_json::json!({ "ok": true }))
    })
    .await
}

/// Un usuario de la LAN en la lista (contrato con la interfaz).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsuarioLanDto {
    pub nombre: String,
}

/// Respuesta de `lan_usuario_crear`. Es la única vez que la contraseña sale de `core`:
/// no se guarda ni se registra, y por eso este tipo no implementa `Debug`.
#[derive(Serialize)]
pub struct UsuarioLanCreadoDto {
    pub nombre: String,
    pub password: String,
}

/// Los errores propios de los usuarios de la LAN llevan el código del contrato; el resto
/// sigue la regla general de [`ErrorUi::de`].
fn error_de_usuarios(error: &ErrorCuentas) -> ErrorUi {
    match error {
        ErrorCuentas::UsuarioLanYaExiste(_) => {
            ErrorUi::nuevo("usuario_ya_existe", error.to_string())
        }
        ErrorCuentas::UsuarioLanNoValido(_) => {
            ErrorUi::nuevo("usuario_no_valido", error.to_string())
        }
        ErrorCuentas::UsuarioLanNoExiste(_) => {
            ErrorUi::nuevo("usuario_no_existe", error.to_string())
        }
        otro => ErrorUi::de(otro),
    }
}

fn usuario_valido(nombre: &str) -> Result<Nombre, ErrorUi> {
    Nombre::nuevo(nombre.trim())
        .map_err(|error| ErrorUi::nuevo("usuario_no_valido", error.to_string()))
}

fn cliente_gitea(recursos: &Recursos, cuenta: &Cuenta) -> Result<ClienteGitea, ErrorUi> {
    let token = recursos
        .llavero
        .leer(&cuenta.login, ClaveSecreto::TokenGitea)
        .map_err(|error| ErrorUi::de(&error))?
        .ok_or_else(|| {
            ErrorUi::interno("no hay token de administración de Gitea para esta cuenta")
        })?;
    cuentas::cliente_gitea_de_cuenta(cuenta, token).map_err(|error| ErrorUi::de(&error))
}

#[tauri::command]
pub async fn lan_usuarios_listar(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<Vec<UsuarioLanDto>, ErrorUi> {
    tracing::debug!(comando = "lan_usuarios_listar");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _) = recursos.cuenta(&login)?;
        let usuarios = cuentas::listar_usuarios_lan(&recursos.contexto(), &cuenta.login)
            .await
            .map_err(|error| error_de_usuarios(&error))?;
        Ok(usuarios
            .into_iter()
            .map(|usuario| UsuarioLanDto {
                nombre: usuario.nombre.to_string(),
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn lan_usuario_crear(
    login: String,
    nombre: String,
    estado: State<'_, EstadoApp>,
) -> Result<UsuarioLanCreadoDto, ErrorUi> {
    tracing::debug!(comando = "lan_usuario_crear");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let nombre = usuario_valido(&nombre)?;
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _) = recursos.cuenta(&login)?;
        let gitea = cliente_gitea(&recursos, &cuenta)?;
        let creado =
            cuentas::crear_usuario_lan(&recursos.contexto(), &gitea, &cuenta.login, &nombre)
                .await
                .map_err(|error| error_de_usuarios(&error))?;
        Ok(UsuarioLanCreadoDto {
            nombre: creado.nombre.to_string(),
            password: creado.password.exponer().to_string(),
        })
    })
    .await
}

#[tauri::command]
pub async fn lan_usuario_eliminar(
    login: String,
    nombre: String,
    estado: State<'_, EstadoApp>,
) -> Result<serde_json::Value, ErrorUi> {
    tracing::debug!(comando = "lan_usuario_eliminar");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let nombre = usuario_valido(&nombre)?;
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _) = recursos.cuenta(&login)?;
        let gitea = cliente_gitea(&recursos, &cuenta)?;
        cuentas::eliminar_usuario_lan(&recursos.contexto(), &gitea, &cuenta.login, &nombre)
            .await
            .map_err(|error| error_de_usuarios(&error))?;
        Ok(serde_json::Value::Null)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn informe(host: Option<&str>) -> InformeLan {
        InformeLan {
            url_publica: "https://jparga.gitmereba.internal:3900".to_string(),
            host: host.map(|h| NombreHostInterno::nuevo(h).expect("host válido")),
            ip_lan: None,
            linea_hosts: None,
            huella_sha256: Some("AA:BB".to_string()),
            ruta_certificado: None,
            comando_git_cliente: None,
            regla_cortafuegos_sugerida: None,
        }
    }

    #[test]
    fn los_errores_de_usuarios_llevan_el_codigo_del_contrato() {
        let ana = Nombre::nuevo("ana").expect("nombre válido");
        let ya = error_de_usuarios(&ErrorCuentas::UsuarioLanYaExiste(ana.clone()));
        assert_eq!(ya.codigo, "usuario_ya_existe");
        let no = error_de_usuarios(&ErrorCuentas::UsuarioLanNoValido(ana));
        assert_eq!(no.codigo, "usuario_no_valido");
        assert_eq!(
            usuario_valido("con espacio").expect_err("inválido").codigo,
            "usuario_no_valido"
        );
    }

    #[test]
    fn sin_host_no_hay_informe() {
        assert_eq!(informe_dto(informe(None)), None);
    }

    #[test]
    fn sin_ip_la_linea_de_hosts_lleva_un_hueco_visible() {
        let dto = informe_dto(informe(Some("jparga.gitmereba.internal"))).expect("activo");
        assert_eq!(dto.ip_lan, None);
        assert_eq!(
            dto.linea_hosts,
            "<IP de este equipo>  jparga.gitmereba.internal"
        );
        let json = serde_json::to_value(&dto).expect("serializa");
        assert!(json["ip_lan"].is_null());
        assert_eq!(json["host"], "jparga.gitmereba.internal");
    }
}
