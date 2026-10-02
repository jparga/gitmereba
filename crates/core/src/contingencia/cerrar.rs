//! `cerrar`: reanuda el mirror original tras una reconciliación completa. Y
//! `borrar_contingencia`: borra el repo hermano, aparte, con confirmación explícita.

use crate::config::RutasCuenta;
use crate::gitea::ApiGitea;
use crate::modelo::IdRepo;

use super::error::ErrorContingencia;
use super::estado::estado;
use super::modelo::{PuntoDePartida, ResultadoCierre};
use super::nombres::org_contingencia;
use super::reconciliar::InformeReconciliacion;

/// Vuelve a activar el mirror original de `id_original`: solo si `informe_reconciliacion`
/// (si se da) llegó `completa`, o si [`super::estado`] dice que no queda ningún commit de
/// más.
///
/// El repo de contingencia no se toca ni se borra (la API de Gitea actual no permite
/// archivarlo): queda `pendiente_de_borrar`. Bórralo aparte con [`borrar_contingencia`].
pub async fn cerrar<T: ApiGitea>(
    gitea: &T,
    rutas: &RutasCuenta,
    id_original: &IdRepo,
    intervalo_minutos: u32,
    punto_de_partida: &PuntoDePartida,
    informe_reconciliacion: Option<&InformeReconciliacion>,
) -> Result<ResultadoCierre, ErrorContingencia> {
    let permitido = match informe_reconciliacion {
        Some(informe) => informe.completa,
        None => !estado(rutas, id_original, punto_de_partida)
            .await?
            .hay_commits_de_mas(),
    };
    if !permitido {
        return Err(ErrorContingencia::ReconciliacionIncompleta(
            id_original.clone(),
        ));
    }

    let intervalo = format!("{intervalo_minutos}m");
    gitea.fijar_intervalo(id_original, &intervalo).await?;

    let dueno_contingencia = org_contingencia(&id_original.dueno)?;
    let id_contingencia = IdRepo {
        dueno: dueno_contingencia,
        nombre: id_original.nombre.clone(),
    };
    Ok(ResultadoCierre {
        original: id_original.clone(),
        contingencia: id_contingencia,
        pendiente_de_borrar: true,
    })
}

/// Borra el repo de contingencia de `id_original`.
///
/// Exige `confirmacion == "<dueño de contingencia>/<nombre>"` (exactamente el `IdRepo`
/// del repo que se va a borrar) y que no quede ningún commit sin reconciliar: en ese caso
/// devuelve [`ErrorContingencia::HayTrabajoSinReconciliar`], sin ninguna opción de forzar.
pub async fn borrar_contingencia<T: ApiGitea>(
    gitea: &T,
    rutas: &RutasCuenta,
    id_original: &IdRepo,
    punto_de_partida: &PuntoDePartida,
    confirmacion: &str,
) -> Result<(), ErrorContingencia> {
    let dueno_contingencia = org_contingencia(&id_original.dueno)?;
    let id_contingencia = IdRepo {
        dueno: dueno_contingencia,
        nombre: id_original.nombre.clone(),
    };
    let esperado = id_contingencia.to_string();
    if confirmacion != esperado {
        return Err(ErrorContingencia::ConfirmacionIncorrecta { esperado });
    }

    if estado(rutas, id_original, punto_de_partida)
        .await?
        .hay_commits_de_mas()
    {
        return Err(ErrorContingencia::HayTrabajoSinReconciliar(
            id_original.clone(),
        ));
    }

    gitea.borrar_repo(&id_contingencia).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::git::{self, Opciones};
    use crate::modelo::Nombre;

    use super::super::dobles::GiteaDoble;
    use super::*;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(repo),
        }
    }

    async fn git_de_prueba(directorio: &std::path::Path, args: &[&str]) -> String {
        let mut completos = vec!["-c", "user.name=Test", "-c", "user.email=test@test.invalid"];
        completos.extend_from_slice(args);
        let opciones = Opciones {
            directorio: Some(directorio.to_path_buf()),
            ..Opciones::default()
        };
        git::ejecutar(&completos, &opciones)
            .await
            .unwrap_or_else(|e| panic!("«git {args:?}» falló: {e}"))
            .stdout
    }

    /// Bare de contingencia de `jparga/repo1` con un commit, y el `PuntoDePartida`
    /// correspondiente (coincide con el estado actual: sin commits de más).
    async fn preparar_al_dia() -> (tempfile::TempDir, RutasCuenta, PuntoDePartida) {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let repositorios = rutas.gitea_repositorios();

        let trabajo = tempfile::tempdir().expect("directorio de trabajo temporal");
        git_de_prueba(
            trabajo.path(),
            &["-c", "init.defaultBranch=main", "init", "-q"],
        )
        .await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "inicial"],
        )
        .await;
        let sha = git_de_prueba(trabajo.path(), &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_string();

        let bare = repositorios.join("contingencia-jparga").join("repo1.git");
        std::fs::create_dir_all(bare.parent().expect("padre")).expect("crear carpeta del dueño");
        git::ejecutar(
            &[
                "clone",
                "--bare",
                "-q",
                trabajo.path().to_str().expect("utf8"),
                bare.to_str().expect("utf8"),
            ],
            &Opciones::default(),
        )
        .await
        .expect("clonar en bare");

        let punto = PuntoDePartida {
            refs: [("refs/heads/main".to_string(), sha)].into(),
        };
        (raiz, rutas, punto)
    }

    #[tokio::test]
    async fn borrar_contingencia_rechaza_una_confirmacion_incorrecta() {
        let (_raiz, rutas, punto) = preparar_al_dia().await;
        let gitea = GiteaDoble::nueva();

        let resultado = borrar_contingencia(
            &gitea,
            &rutas,
            &id("jparga", "repo1"),
            &punto,
            "algo-mal-escrito",
        )
        .await;
        assert!(matches!(
            resultado,
            Err(ErrorContingencia::ConfirmacionIncorrecta { .. })
        ));
        assert!(gitea.borrados().is_empty());
    }

    #[tokio::test]
    async fn borrar_contingencia_rechaza_si_hay_trabajo_sin_reconciliar() {
        let (_raiz, rutas, punto) = preparar_al_dia().await;
        let bare = rutas
            .gitea_repositorios()
            .join("contingencia-jparga")
            .join("repo1.git");

        // Un commit más, sin reconciliar todavía.
        let trabajo = tempfile::tempdir().expect("clon de trabajo");
        git_de_prueba(
            trabajo.path(),
            &["clone", "-q", bare.to_str().expect("utf8"), "."],
        )
        .await;
        git_de_prueba(
            trabajo.path(),
            &["commit", "--allow-empty", "-q", "-m", "pendiente"],
        )
        .await;
        git_de_prueba(trabajo.path(), &["push", "-q", "origin", "main"]).await;

        let gitea = GiteaDoble::nueva();
        let resultado = borrar_contingencia(
            &gitea,
            &rutas,
            &id("jparga", "repo1"),
            &punto,
            "contingencia-jparga/repo1",
        )
        .await;
        assert!(matches!(
            resultado,
            Err(ErrorContingencia::HayTrabajoSinReconciliar(_))
        ));
        assert!(gitea.borrados().is_empty());
    }

    #[tokio::test]
    async fn borrar_contingencia_borra_si_esta_al_dia_y_la_confirmacion_es_correcta() {
        let (_raiz, rutas, punto) = preparar_al_dia().await;
        let gitea = GiteaDoble::nueva();

        borrar_contingencia(
            &gitea,
            &rutas,
            &id("jparga", "repo1"),
            &punto,
            "contingencia-jparga/repo1",
        )
        .await
        .expect("debe poder borrarse");
        assert_eq!(gitea.borrados(), vec![id("contingencia-jparga", "repo1")]);
    }

    #[tokio::test]
    async fn cerrar_rechaza_un_informe_incompleto() {
        let (_raiz, rutas, punto) = preparar_al_dia().await;
        let gitea = GiteaDoble::nueva();
        let informe = InformeReconciliacion {
            ramas: vec![],
            tags: vec![],
            completa: false,
        };

        let resultado = cerrar(
            &gitea,
            &rutas,
            &id("jparga", "repo1"),
            30,
            &punto,
            Some(&informe),
        )
        .await;
        assert!(matches!(
            resultado,
            Err(ErrorContingencia::ReconciliacionIncompleta(_))
        ));
        assert!(gitea.intervalos_fijados().is_empty());
    }

    #[tokio::test]
    async fn cerrar_reanuda_el_mirror_si_el_informe_esta_completo() {
        let (_raiz, rutas, punto) = preparar_al_dia().await;
        let gitea = GiteaDoble::nueva();
        let informe = InformeReconciliacion {
            ramas: vec![],
            tags: vec![],
            completa: true,
        };

        let resultado = cerrar(
            &gitea,
            &rutas,
            &id("jparga", "repo1"),
            30,
            &punto,
            Some(&informe),
        )
        .await
        .expect("debe poder cerrarse");
        assert!(resultado.pendiente_de_borrar);
        assert_eq!(
            gitea.intervalos_fijados(),
            vec![(id("jparga", "repo1"), "30m".to_string())]
        );
    }

    #[tokio::test]
    async fn cerrar_sin_informe_usa_estado_si_no_hay_commits_de_mas() {
        let (_raiz, rutas, punto) = preparar_al_dia().await;
        let gitea = GiteaDoble::nueva();

        cerrar(&gitea, &rutas, &id("jparga", "repo1"), 15, &punto, None)
            .await
            .expect("sin commits de más, debe poder cerrarse");
        assert_eq!(
            gitea.intervalos_fijados(),
            vec![(id("jparga", "repo1"), "15m".to_string())]
        );
    }
}
