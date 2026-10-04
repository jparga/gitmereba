//! Comandos de la pantalla «Acerca de» y de la versión del pie: datos de la versión y del
//! entorno, versión de Gitea de cada cuenta y apertura de los enlaces del proyecto.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gitmereba_core::config::{self, Rutas};
use gitmereba_core::cuentas;
use gitmereba_core::entorno::{self, Enlace, EstadoVersion};
use gitmereba_core::idioma::Idioma;
use gitmereba_core::instancia::VERSION_GITEA;
use gitmereba_core::secretos::{ClaveSecreto, Llavero, Secreto};
use serde::Serialize;
use tauri::State;

use super::acciones::abrir_en_navegador;
use super::dto_acciones::RespuestaOk;
use super::error::{ErrorUi, texto};
use super::estado::{EstadoApp, Recursos, en_hilo};

const LICENCIA: &str = "GPL-3.0-or-later";

/// Cuánto se espera a que la instancia de una cuenta diga su versión.
const ESPERA_VERSION_GITEA: Duration = Duration::from_secs(3);

/// Fichero del que sale el nombre del sistema operativo.
const OS_RELEASE: &str = "/etc/os-release";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RutasDto {
    pub datos: String,
    pub config: String,
    pub systemd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CuentaAcercaDto {
    pub login: String,
    pub carpeta: String,
}

/// Respuesta de `acerca_de`. Las rutas bajo el directorio personal llegan con `~`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcercaDeDto {
    pub version: String,
    pub licencia: String,
    pub version_gitea_incluida: String,
    pub rutas: RutasDto,
    pub ejecutable: Option<String>,
    pub idioma: String,
    pub sistema: Option<String>,
    pub cuentas: Vec<CuentaAcercaDto>,
}

/// Respuesta de `version_gitea_cuenta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionGiteaDto {
    pub version: Option<String>,
    pub estado: String,
}

/// Compone la respuesta de `acerca_de`. Pura sobre sus entradas para probarla sin ventana.
fn construir_acerca_de(
    rutas: &Rutas,
    idioma: Idioma,
    sistema: Option<String>,
    ejecutable: Option<PathBuf>,
    mut cuentas: Vec<(String, PathBuf)>,
) -> AcercaDeDto {
    let home = rutas.directorio_home();
    let corta = |ruta: &Path| entorno::acortar_home(ruta, home);
    cuentas.sort();
    AcercaDeDto {
        version: env!("CARGO_PKG_VERSION").to_string(),
        licencia: LICENCIA.to_string(),
        version_gitea_incluida: VERSION_GITEA.to_string(),
        rutas: RutasDto {
            datos: corta(rutas.directorio_datos()),
            config: corta(rutas.directorio_config()),
            systemd: corta(rutas.directorio_systemd_usuario()),
        },
        ejecutable: ejecutable.as_deref().map(corta),
        idioma: idioma.codigo().to_string(),
        sistema,
        cuentas: cuentas
            .into_iter()
            .map(|(login, carpeta)| CuentaAcercaDto {
                carpeta: corta(&carpeta),
                login,
            })
            .collect(),
    }
}

/// Destino de `abrir_enlace`. Solo se admite el conjunto cerrado de [`Enlace`].
fn destino_de(destino: &str, idioma: Idioma) -> Result<Enlace, ErrorUi> {
    match destino {
        "repositorio" => Ok(Enlace::Repositorio),
        "release" => Ok(Enlace::Release),
        "seguridad" => Ok(Enlace::Seguridad),
        "marcas" => Ok(Enlace::Marcas),
        _ => Err(ErrorUi::nuevo(
            "datos_invalidos",
            texto(idioma, "enlace desconocido", "unknown link"),
        )),
    }
}

#[tauri::command]
pub async fn acerca_de(estado: State<'_, EstadoApp>) -> Result<AcercaDeDto, ErrorUi> {
    tracing::debug!(comando = "acerca_de");
    let rutas = estado.rutas.clone();
    let idioma = estado.idioma();
    en_hilo(idioma, move || async move {
        let indice =
            config::leer_indice_cuentas(&rutas).map_err(|error| ErrorUi::de(&error, idioma))?;
        let cuentas = indice
            .cuentas
            .into_iter()
            .map(|(login, entrada)| (login, entrada.carpeta))
            .collect();
        Ok(construir_acerca_de(
            &rutas,
            idioma,
            entorno::nombre_sistema(Path::new(OS_RELEASE)),
            std::env::current_exe().ok(),
            cuentas,
        ))
    })
    .await
}

#[tauri::command]
pub async fn version_gitea_cuenta(
    estado: State<'_, EstadoApp>,
    login: String,
) -> Result<VersionGiteaDto, ErrorUi> {
    tracing::debug!(comando = "version_gitea_cuenta");
    let rutas = estado.rutas.clone();
    let idioma = estado.idioma();
    en_hilo(idioma, move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _rutas_cuenta) = recursos.cuenta(&login)?;
        let no_disponible = VersionGiteaDto {
            version: None,
            estado: EstadoVersion::NoDisponible.codigo().to_string(),
        };
        // Un llavero o un cliente que fallan no son un error de la pantalla: la versión
        // de esa cuenta, sencillamente, no está disponible.
        let Ok(token) = recursos
            .llavero
            .leer(&cuenta.login, ClaveSecreto::TokenGitea)
            .map(|token| token.unwrap_or_else(|| Secreto::nuevo("")))
        else {
            return Ok(no_disponible);
        };
        let Ok(gitea) = cuentas::cliente_gitea_de_cuenta(&cuenta, token) else {
            return Ok(no_disponible);
        };
        let (version, estado) = entorno::version_gitea(&gitea, ESPERA_VERSION_GITEA).await;
        Ok(VersionGiteaDto {
            version,
            estado: estado.codigo().to_string(),
        })
    })
    .await
}

#[tauri::command]
pub async fn abrir_enlace(
    estado: State<'_, EstadoApp>,
    destino: String,
) -> Result<RespuestaOk, ErrorUi> {
    tracing::debug!(comando = "abrir_enlace");
    let idioma = estado.idioma();
    let enlace = destino_de(&destino, idioma)?;
    let url = entorno::url_enlace(
        enlace,
        env!("CARGO_PKG_REPOSITORY"),
        env!("CARGO_PKG_VERSION"),
    )
    .map_err(|error| ErrorUi::externo("interno", None, &error))?;
    abrir_en_navegador(url.as_str(), idioma)?;
    Ok(RespuestaOk::si())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acerca_de_lleva_version_licencia_y_rutas_acortadas() {
        let dir = tempfile::tempdir().unwrap();
        let rutas = Rutas::con_raiz(dir.path());
        let home = rutas.directorio_home().to_path_buf();
        let dto = construir_acerca_de(
            &rutas,
            Idioma::Es,
            Some("Ubuntu 26.04 LTS".to_string()),
            Some(PathBuf::from("/usr/bin/gitmereba")),
            vec![
                ("zeta".to_string(), home.join("gitmereba-zeta")),
                ("ana".to_string(), PathBuf::from("/srv/ana")),
            ],
        );
        assert_eq!(dto.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(dto.licencia, "GPL-3.0-or-later");
        assert_eq!(dto.version_gitea_incluida, VERSION_GITEA);
        assert_eq!(dto.idioma, "es");
        assert_eq!(dto.ejecutable.as_deref(), Some("/usr/bin/gitmereba"));
        assert!(dto.rutas.datos.starts_with('~'), "{}", dto.rutas.datos);
        assert_eq!(
            dto.cuentas,
            vec![
                CuentaAcercaDto {
                    login: "ana".to_string(),
                    carpeta: "/srv/ana".to_string()
                },
                CuentaAcercaDto {
                    login: "zeta".to_string(),
                    carpeta: "~/gitmereba-zeta".to_string()
                },
            ]
        );
    }

    #[test]
    fn abrir_enlace_solo_admite_destinos_conocidos() {
        assert_eq!(
            destino_de("release", Idioma::Es).ok(),
            Some(Enlace::Release)
        );
        assert_eq!(
            destino_de("seguridad", Idioma::En).ok(),
            Some(Enlace::Seguridad)
        );
        assert!(destino_de("https://ejemplo.invalid", Idioma::Es).is_err());
        assert!(destino_de("", Idioma::Es).is_err());
    }
}
