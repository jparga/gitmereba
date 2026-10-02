//! Comandos de solo lectura del contrato con la interfaz.

use std::collections::HashMap;

use gitmereba_core::almacen::{EntradaAuditoria, VerificacionAuditoria};
use gitmereba_core::contingencia;
use gitmereba_core::cuentas;
use gitmereba_core::gitea::ApiGitea;
use gitmereba_core::github::{ApiGithub, ClienteGithub};
use gitmereba_core::instancia;
use gitmereba_core::modelo::{Cuenta, IdRepo, Nombre, RepoLocal};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, Secreto};
use gitmereba_core::snapshots;
use gitmereba_core::verificacion;
use tauri::State;
use time::OffsetDateTime;

use super::dto::{
    AjustesCuentaDto, AjustesDto, AjustesGlobalDto, EntradaContingenciaDto, EstadoGithubDto,
    RepoListadoDto, ResumenCuentaDto, SincronizacionDto, VerificacionAuditoriaDto,
    convertir_commits_de_mas, convertir_estado_github, convertir_historial,
    convertir_repos_listado, convertir_snapshots, convertir_verificacion, marcar_contingencias,
    resumen_cuenta_dto,
};
use super::error::ErrorUi;
use super::estado::{EstadoApp, Recursos, en_hilo};

#[tauri::command]
pub async fn listar_cuentas(estado: State<'_, EstadoApp>) -> Result<Vec<Cuenta>, ErrorUi> {
    tracing::debug!(comando = "listar_cuentas");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        cuentas::listar(&recursos.contexto()).map_err(|error| ErrorUi::de(&error))
    })
    .await
}

#[tauri::command]
pub async fn resumen_cuenta(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<ResumenCuentaDto, ErrorUi> {
    tracing::debug!(comando = "resumen_cuenta");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, rutas_cuenta) = recursos.cuenta(&login)?;

        let espacio_bytes = verificacion::espacio_de(rutas_cuenta.carpeta())
            .map_err(|error| ErrorUi::de(&error))?;
        let estado_cuenta = cuentas::estado(&recursos.contexto(), &cuenta.login)
            .await
            .map_err(|error| ErrorUi::de(&error))?;

        Ok(resumen_cuenta_dto(estado_cuenta, espacio_bytes / 1024))
    })
    .await
}

#[tauri::command]
pub async fn listar_repos(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<Vec<RepoListadoDto>, ErrorUi> {
    tracing::debug!(comando = "listar_repos");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, _rutas_cuenta) = recursos.cuenta(&login)?;

        let activas = recursos
            .almacen
            .contingencias_de(&cuenta.login)
            .map_err(|error| ErrorUi::de(&error))?;
        let guardados = recursos
            .almacen
            .estados_de(&cuenta.login)
            .map_err(|error| ErrorUi::de(&error))?;
        let guardados = marcar_contingencias(guardados, &activas);

        let token_gitea = recursos
            .llavero
            .leer(&cuenta.login, ClaveSecreto::TokenGitea)
            .map_err(|error| ErrorUi::de(&error))?
            .unwrap_or_else(|| Secreto::nuevo(""));
        let gitea = cuentas::cliente_gitea_de_cuenta(&cuenta, token_gitea)
            .map_err(|error| ErrorUi::de(&error))?;

        let mut duenos: Vec<Nombre> = guardados.iter().map(|g| g.id.dueno.clone()).collect();
        duenos.sort();
        duenos.dedup();

        let mut locales: HashMap<IdRepo, RepoLocal> = HashMap::new();
        for dueno in duenos {
            // Si Gitea no responde o el dueño no existe todavía ahí, ese repo se lista
            // igualmente (con los metadatos locales a valores por defecto): no llamar a
            // GitHub es una restricción de este comando, pero un Gitea local parado no
            // debe tumbar todo el listado.
            if let Ok(repos) = gitea.repos_de(&dueno).await {
                for repo in repos {
                    locales.insert(repo.id.clone(), repo);
                }
            }
        }

        let origenes = recursos
            .almacen
            .origenes_de(&cuenta.login)
            .map_err(|error| ErrorUi::de(&error))?;

        Ok(convertir_repos_listado(
            &cuenta, guardados, &locales, &origenes,
        ))
    })
    .await
}

#[tauri::command]
pub async fn historial(
    login: Option<String>,
    limite: u32,
    estado: State<'_, EstadoApp>,
) -> Result<Vec<SincronizacionDto>, ErrorUi> {
    tracing::debug!(comando = "historial");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let nombre = login
            .map(Nombre::nuevo)
            .transpose()
            .map_err(|error| ErrorUi::de(&error))?;
        let listado = recursos
            .almacen
            .ultimas_sincronizaciones(nombre.as_ref(), limite)
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(convertir_historial(listado))
    })
    .await
}

#[tauri::command]
pub async fn auditoria(
    limite: u32,
    desde_id: Option<i64>,
    estado: State<'_, EstadoApp>,
) -> Result<Vec<EntradaAuditoria>, ErrorUi> {
    tracing::debug!(comando = "auditoria");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        recursos
            .almacen
            .auditoria(limite, desde_id)
            .map_err(|error| ErrorUi::de(&error))
    })
    .await
}

#[tauri::command]
pub async fn verificar_auditoria(
    estado: State<'_, EstadoApp>,
) -> Result<VerificacionAuditoriaDto, ErrorUi> {
    tracing::debug!(comando = "verificar_auditoria");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let verificacion: VerificacionAuditoria = recursos
            .almacen
            .verificar_auditoria()
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(convertir_verificacion(verificacion))
    })
    .await
}

#[tauri::command]
pub async fn estado_github() -> Result<EstadoGithubDto, ErrorUi> {
    tracing::debug!(comando = "estado_github");
    en_hilo(move || async move {
        // No necesita token: `estado_servicio` consulta githubstatus.com sin
        // autenticar (ver `github::cliente::ClienteGithub`).
        let cliente =
            ClienteGithub::nuevo(Secreto::nuevo("")).map_err(|error| ErrorUi::de(&error))?;
        let estado_servicio = cliente
            .estado_servicio()
            .await
            .map_err(|error| ErrorUi::de(&error))?;
        Ok(convertir_estado_github(
            estado_servicio,
            OffsetDateTime::now_utc(),
        ))
    })
    .await
}

#[tauri::command]
pub async fn estado_contingencia(
    login: String,
    estado: State<'_, EstadoApp>,
) -> Result<Vec<EntradaContingenciaDto>, ErrorUi> {
    tracing::debug!(comando = "estado_contingencia");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        let (cuenta, rutas_cuenta) = recursos.cuenta(&login)?;

        // La fuente de verdad de qué está en contingencia son los puntos de partida
        // anotados al activar (los repos originales, no sus copias hermanas).
        let activas = recursos
            .almacen
            .contingencias_de(&cuenta.login)
            .map_err(|error| ErrorUi::de(&error))?;

        let mut resultado = Vec::new();
        for id_original in activas {
            let dueno_contingencia = contingencia::org_contingencia(&id_original.dueno)
                .map_err(|error| ErrorUi::de(&error))?;
            let comando_git = format!(
                "git remote add mereba {}/{}/{}.git",
                cuenta.url_publica(),
                dueno_contingencia.as_str(),
                id_original.nombre.as_str()
            );
            // El punto de partida anotado al activar manda; el mirror congelado solo es
            // el respaldo para contingencias anteriores a que se guardara.
            let anotado = recursos
                .almacen
                .punto_de_partida(&cuenta.login, &id_original)
                .map_err(|error| ErrorUi::de(&error))?;
            let estado_contingencia = match anotado {
                Some(punto) => contingencia::estado(&rutas_cuenta, &id_original, &punto).await,
                None => contingencia::estado_de_mirror(&rutas_cuenta, &id_original).await,
            }
            .map_err(|error| ErrorUi::de(&error))?;

            resultado.push(EntradaContingenciaDto {
                id: id_original,
                comando: comando_git,
                commits_de_mas: convertir_commits_de_mas(estado_contingencia.ramas),
            });
        }
        Ok(resultado)
    })
    .await
}

#[tauri::command]
pub async fn ajustes_leer(
    login: Option<String>,
    estado: State<'_, EstadoApp>,
) -> Result<AjustesDto, ErrorUi> {
    tracing::debug!(comando = "ajustes_leer");
    let rutas = estado.rutas.clone();
    en_hilo(move || async move {
        let recursos = Recursos::abrir(&rutas)?;
        match login {
            Some(login) => {
                let (cuenta, rutas_cuenta) = recursos.cuenta(&login)?;
                let capturas =
                    snapshots::listar_cuenta(&rutas_cuenta).map_err(|error| ErrorUi::de(&error))?;
                Ok(AjustesDto::Cuenta(AjustesCuentaDto {
                    cuenta,
                    version_gitea: instancia::VERSION_GITEA.to_string(),
                    snapshots: convertir_snapshots(capturas),
                }))
            }
            None => Ok(AjustesDto::Global(AjustesGlobalDto {
                version_gitea: instancia::VERSION_GITEA.to_string(),
                snapshots: Vec::new(),
            })),
        }
    })
    .await
}
