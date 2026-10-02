//! Alta de una cuenta (pasos 3-8 del alta). La previsualización (pasos 1-2) está en
//! [`super::solicitud`].

use crate::config::{self, EntradaCuenta, RutasCuenta, validar_alta};
use crate::gitea::{ApiGitea, ErrorGitea};
use crate::github::ApiGithub;
use crate::instancia::{ParametrosProvision, puerto_libre};
use crate::modelo::Cuenta;
use crate::secretos::{ClaveSecreto, Llavero, Secreto};
use crate::sync::OpcionesSync;

use super::aprovisionador::Aprovisionador;
use super::comun::{crear_directorio_0700, usuario_actual};
use super::contexto::Contexto;
use super::error::ErrorCuentas;
use super::lanzador::{Lanzador, ParametrosLanzamiento};
use super::progreso::{EstadoPaso, PasoAlta};
use super::sincronizar::{InformeSync, sincronizar_con};
use super::solicitud::SolicitudAlta;

/// Da de alta una cuenta: pasos 3-8 del alta, idempotentes.
///
/// `github` ya debe estar autenticado con `solicitud.token` (el mismo cliente, o un
/// doble equivalente, que se usó en [`super::previsualizar_alta`]: así no se vuelve a
/// pedir la identidad a GitHub). `fabrica_gitea` construye el cliente de Gitea de la
/// cuenta una vez que se conoce su token de administración (generado por
/// `aprovisionador.provisionar`, que no existe hasta ese momento): en producción es
/// `ClienteGitea::nuevo`; en las pruebas, una fábrica que siempre devuelve el mismo
/// doble.
///
/// Si un paso falla, no se borra nada de lo ya hecho: el error dice en qué paso falló
/// (a través de `progreso` y del propio `Err`) y repetir `alta` con los mismos datos lo
/// completa. La única limpieza automática: si el fallo ocurre antes de crear la
/// carpeta, se retira el token de GitHub del llavero.
#[allow(clippy::too_many_arguments)]
pub async fn alta<L, G, T, F, Ln, Ap>(
    contexto: &Contexto<'_, L>,
    solicitud: &SolicitudAlta,
    github: &G,
    fabrica_gitea: F,
    lanzador: &Ln,
    aprovisionador: &Ap,
    progreso: &mut dyn FnMut(PasoAlta, EstadoPaso),
) -> Result<InformeSync, ErrorCuentas>
where
    L: Llavero,
    G: ApiGithub,
    T: ApiGitea,
    F: Fn(&str, Secreto) -> Result<T, ErrorGitea>,
    Ln: Lanzador,
    Ap: Aprovisionador,
{
    progreso(PasoAlta::Validar, EstadoPaso::Iniciando);
    let puerto = validar_o_reanudar(contexto, solicitud)?;
    progreso(PasoAlta::Validar, EstadoPaso::Hecho);

    progreso(PasoAlta::GuardarToken, EstadoPaso::Iniciando);
    contexto.llavero.guardar(
        &solicitud.login,
        ClaveSecreto::TokenGithub,
        &solicitud.token,
    )?;
    progreso(PasoAlta::GuardarToken, EstadoPaso::Hecho);

    let resultado = continuar_alta(
        contexto,
        solicitud,
        puerto,
        github,
        fabrica_gitea,
        lanzador,
        aprovisionador,
        progreso,
    )
    .await;

    // Única limpieza automática permitida: si el fallo ocurre antes de crear
    // la carpeta, se retira el token que se acababa de guardar.
    if resultado.is_err() && !solicitud.carpeta.exists() {
        let _ = contexto
            .llavero
            .borrar(&solicitud.login, ClaveSecreto::TokenGithub);
    }

    resultado
}

/// Decide si esta llamada reanuda un alta a medias para el mismo login+carpeta (en cuyo
/// caso se reutiliza el puerto ya elegido y no se repite `validar_alta`, que rechazaría
/// un login que ya está en el índice) o si es un alta nueva (se elige un puerto libre y
/// se valida normalmente).
fn validar_o_reanudar<L: Llavero>(
    contexto: &Contexto<'_, L>,
    solicitud: &SolicitudAlta,
) -> Result<u16, ErrorCuentas> {
    let indice = config::leer_indice_cuentas(contexto.rutas)?;

    if let Some(entrada) = indice.cuentas.get(solicitud.login.as_str())
        && entrada.carpeta == solicitud.carpeta
    {
        return Ok(entrada.puerto);
    }

    let excluidos: Vec<u16> = indice.cuentas.values().map(|e| e.puerto).collect();
    let puerto = puerto_libre(&excluidos)?;
    validar_alta(
        contexto.rutas,
        &indice,
        solicitud.login.as_str(),
        &solicitud.carpeta,
        puerto,
        solicitud.intervalo_minutos,
    )?;
    Ok(puerto)
}

#[allow(clippy::too_many_arguments)]
async fn continuar_alta<L, G, T, F, Ln, Ap>(
    contexto: &Contexto<'_, L>,
    solicitud: &SolicitudAlta,
    puerto: u16,
    github: &G,
    fabrica_gitea: F,
    lanzador: &Ln,
    aprovisionador: &Ap,
    progreso: &mut dyn FnMut(PasoAlta, EstadoPaso),
) -> Result<InformeSync, ErrorCuentas>
where
    L: Llavero,
    G: ApiGithub,
    T: ApiGitea,
    F: Fn(&str, Secreto) -> Result<T, ErrorGitea>,
    Ln: Lanzador,
    Ap: Aprovisionador,
{
    progreso(PasoAlta::CrearCarpeta, EstadoPaso::Iniciando);
    crear_directorio_0700(&solicitud.carpeta)?;
    progreso(PasoAlta::CrearCarpeta, EstadoPaso::Hecho);

    progreso(PasoAlta::EscribirConfiguracion, EstadoPaso::Iniciando);
    let cuenta = Cuenta {
        login: solicitud.login.clone(),
        carpeta: solicitud.carpeta.clone(),
        puerto,
        intervalo_minutos: solicitud.intervalo_minutos,
        alcance: solicitud.alcance.clone(),
        // Una cuenta nueva siempre nace sin acceso LAN: se activa aparte con
        // `cuentas::exponer_lan`.
        lan: None,
    };
    let rutas_cuenta = RutasCuenta::nueva(&solicitud.carpeta);
    config::escribir_cuenta(&rutas_cuenta, &cuenta)?;

    let mut indice = config::leer_indice_cuentas(contexto.rutas)?;
    indice.cuentas.insert(
        solicitud.login.as_str().to_string(),
        EntradaCuenta {
            carpeta: solicitud.carpeta.clone(),
            puerto,
        },
    );
    config::escribir_indice_cuentas(contexto.rutas, &indice)?;
    progreso(PasoAlta::EscribirConfiguracion, EstadoPaso::Hecho);

    progreso(PasoAlta::AsegurarBinario, EstadoPaso::Iniciando);
    let binario = aprovisionador.asegurar_binario(contexto.rutas).await?;
    progreso(PasoAlta::AsegurarBinario, EstadoPaso::Hecho);

    progreso(PasoAlta::Provisionar, EstadoPaso::Iniciando);
    let run_user = usuario_actual();
    let parametros_provision = ParametrosProvision {
        binario_gitea: &binario,
        rutas_cuenta: &rutas_cuenta,
        puerto,
        run_user: &run_user,
        login: &solicitud.login,
        secretos: contexto.llavero,
        modo_pruebas: solicitud.modo_pruebas,
    };
    aprovisionador.provisionar(&parametros_provision).await?;
    progreso(PasoAlta::Provisionar, EstadoPaso::Hecho);

    progreso(PasoAlta::ArrancarGitea, EstadoPaso::Iniciando);
    let parametros_lanzamiento = ParametrosLanzamiento {
        login: &solicitud.login,
        binario_gitea: &binario,
        app_ini: &rutas_cuenta.gitea_app_ini(),
        directorio_trabajo: &rutas_cuenta.gitea(),
        carpeta_cuenta: &solicitud.carpeta,
        puerto,
        // Una cuenta nueva siempre nace sin acceso LAN (ver más arriba): la sonda de
        // arranque siempre es HTTP simple.
        url_local: cuenta.url_gitea(),
        certificado_pem: None,
    };
    lanzador
        .arrancar(contexto.rutas, &parametros_lanzamiento)
        .await?;
    progreso(PasoAlta::ArrancarGitea, EstadoPaso::Hecho);

    progreso(PasoAlta::PrimeraSincronizacion, EstadoPaso::Iniciando);
    let token_gitea = contexto
        .llavero
        .leer(&solicitud.login, ClaveSecreto::TokenGitea)?
        .ok_or_else(|| ErrorCuentas::TokenGiteaNoGenerado(solicitud.login.clone()))?;
    let gitea = fabrica_gitea(&cuenta.url_gitea(), token_gitea).map_err(ErrorCuentas::from)?;
    let informe = sincronizar_con(
        contexto.almacen,
        &cuenta,
        github,
        &gitea,
        &solicitud.token,
        &OpcionesSync::default(),
    )
    .await?;
    progreso(PasoAlta::PrimeraSincronizacion, EstadoPaso::Hecho);

    progreso(PasoAlta::RegistrarEnAlmacen, EstadoPaso::Iniciando);
    contexto.almacen.auditar(
        Some(&solicitud.login),
        "cuenta.alta",
        &format!("carpeta={}", solicitud.carpeta.display()),
    )?;
    progreso(PasoAlta::RegistrarEnAlmacen, EstadoPaso::Hecho);

    Ok(informe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::config::Rutas;
    use crate::cuentas::dobles::{AprovisionadorDoble, GiteaDoble, GithubDoble, LanzadorDoble};
    use crate::modelo::Alcance;
    use crate::secretos::LlaveroEnMemoria;

    fn nombre(v: &str) -> crate::modelo::Nombre {
        crate::modelo::Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn solicitud(temporal: &std::path::Path, login: &str) -> SolicitudAlta {
        SolicitudAlta {
            login: nombre(login),
            token: Secreto::nuevo("ghp_de_prueba"),
            carpeta: temporal.join(format!("cuenta-{login}")),
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            intervalo_minutos: 30,
            modo_pruebas: false,
        }
    }

    fn sin_progreso(_paso: PasoAlta, _estado: EstadoPaso) {}

    #[tokio::test]
    async fn alta_feliz_registra_auditoria_y_sincronizacion() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        let github = GithubDoble::con_identidad("jparga", None);
        github.con_repos_usuario(vec![GithubDoble::repo("jparga", "repo1", false, false)]);
        let gitea = GiteaDoble::nueva();
        let lanzador = LanzadorDoble::default();
        let aprovisionador = AprovisionadorDoble::default();
        let solicitud = solicitud(temporal.path(), "jparga");

        let gitea_para_fabrica = gitea.clone();
        let informe = alta(
            &contexto,
            &solicitud,
            &github,
            move |_url, _token| Ok(gitea_para_fabrica.clone()),
            &lanzador,
            &aprovisionador,
            &mut sin_progreso,
        )
        .await
        .expect("alta no falla");

        assert!(!informe.hay_fallos());
        assert_eq!(gitea.llamadas_crear_mirror().len(), 1);
        assert_eq!(lanzador.arrancadas.lock().unwrap().len(), 1);

        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "cuenta.alta"));

        assert_eq!(
            almacen
                .ultimas_sincronizaciones(Some(&nombre("jparga")), 10)
                .expect("leer histórico")
                .len(),
            1
        );

        let indice = config::leer_indice_cuentas(&rutas).expect("leer índice");
        assert!(indice.cuentas.contains_key("jparga"));
    }

    #[tokio::test]
    async fn alta_que_falla_al_validar_no_deja_token_en_el_llavero() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        let mut solicitud = solicitud(temporal.path(), "jparga");
        solicitud.intervalo_minutos = 1; // por debajo del mínimo: `validar_alta` falla.

        let github = GithubDoble::con_identidad("jparga", None);
        let gitea = GiteaDoble::nueva();
        let lanzador = LanzadorDoble::default();
        let aprovisionador = AprovisionadorDoble::default();

        let resultado = alta(
            &contexto,
            &solicitud,
            &github,
            move |_url, _token| Ok(gitea.clone()),
            &lanzador,
            &aprovisionador,
            &mut sin_progreso,
        )
        .await;

        assert!(resultado.is_err());
        assert!(
            llavero
                .leer(&nombre("jparga"), ClaveSecreto::TokenGithub)
                .expect("leer no falla")
                .is_none()
        );
    }

    #[tokio::test]
    async fn alta_que_falla_al_provisionar_no_borra_el_token_porque_ya_hay_carpeta() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        let solicitud = solicitud(temporal.path(), "jparga");
        let github = GithubDoble::con_identidad("jparga", None);
        let gitea = GiteaDoble::nueva();
        let lanzador = LanzadorDoble::default();
        let aprovisionador = AprovisionadorDoble::default();
        aprovisionador.con_fallo();

        let resultado = alta(
            &contexto,
            &solicitud,
            &github,
            move |_url, _token| Ok(gitea.clone()),
            &lanzador,
            &aprovisionador,
            &mut sin_progreso,
        )
        .await;

        assert!(resultado.is_err());
        assert!(solicitud.carpeta.exists());
        assert!(
            llavero
                .leer(&nombre("jparga"), ClaveSecreto::TokenGithub)
                .expect("leer no falla")
                .is_some(),
            "la carpeta ya existía: el token no debe retirarse"
        );
    }

    #[tokio::test]
    async fn alta_repetida_tras_fallo_a_medias_se_completa_sin_duplicar_el_indice() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        let solicitud = solicitud(temporal.path(), "jparga");
        let github = GithubDoble::con_identidad("jparga", None);

        // Primer intento: falla al arrancar Gitea (después de escribir la
        // configuración y el índice).
        {
            let gitea = GiteaDoble::nueva();
            let lanzador = LanzadorDoble::default();
            lanzador.con_fallo_al_arrancar();
            let aprovisionador = AprovisionadorDoble::default();

            let resultado = alta(
                &contexto,
                &solicitud,
                &github,
                move |_url, _token| Ok(gitea.clone()),
                &lanzador,
                &aprovisionador,
                &mut sin_progreso,
            )
            .await;
            assert!(resultado.is_err());
        }

        let indice_tras_fallo = config::leer_indice_cuentas(&rutas).expect("leer índice");
        assert_eq!(indice_tras_fallo.cuentas.len(), 1);

        // Segundo intento, mismos datos: debe completarse sin duplicar la entrada del
        // índice ni cambiar de puerto.
        let gitea = GiteaDoble::nueva();
        let lanzador = LanzadorDoble::default();
        let aprovisionador = AprovisionadorDoble::default();

        let informe = alta(
            &contexto,
            &solicitud,
            &github,
            move |_url, _token| Ok(gitea.clone()),
            &lanzador,
            &aprovisionador,
            &mut sin_progreso,
        )
        .await
        .expect("la segunda alta se completa");

        assert!(!informe.hay_fallos());
        let indice_final = config::leer_indice_cuentas(&rutas).expect("leer índice");
        assert_eq!(indice_final.cuentas.len(), 1);
        assert_eq!(
            indice_final.cuentas["jparga"].puerto,
            indice_tras_fallo.cuentas["jparga"].puerto
        );
    }
}
