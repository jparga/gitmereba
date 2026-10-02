//! Rotar el token de lectura de GitHub de una cuenta ya dada de alta (pantalla
//! Ajustes).

use crate::almacen::Almacen;
use crate::github::ApiGithub;
use crate::modelo::Cuenta;
use crate::secretos::{ClaveSecreto, Llavero, Secreto};

use super::error::ErrorCuentas;

/// Valida `token` contra GitHub (debe ser el mismo login que `cuenta`, insensible a
/// mayúsculas) y, solo si es válido, lo guarda en el llavero en sustitución del
/// anterior. El token viejo sigue en el llavero hasta este `guardar` (que lo
/// sobrescribe): si la validación falla, no se toca nada.
pub async fn rotar_token<L: Llavero, G: ApiGithub>(
    llavero: &L,
    almacen: &Almacen,
    cuenta: &Cuenta,
    github: &G,
    token: Secreto,
) -> Result<(), ErrorCuentas> {
    let identidad = github.identidad().await?;
    if identidad.login.as_str().to_lowercase() != cuenta.login.as_str().to_lowercase() {
        return Err(ErrorCuentas::TokenNoCoincideConLogin {
            esperado: cuenta.login.clone(),
            obtenido: identidad.login,
        });
    }

    llavero.guardar(&cuenta.login, ClaveSecreto::TokenGithub, &token)?;
    almacen.auditar(
        Some(&cuenta.login),
        "cuenta.rotar-token",
        "token de GitHub rotado",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuentas::dobles::GithubDoble;
    use crate::modelo::{Alcance, Nombre};
    use crate::secretos::LlaveroEnMemoria;
    use std::path::PathBuf;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn cuenta(login: &str) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: PathBuf::from("/tmp/gitmereba-test-rotar-token"),
            puerto: 3900,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    #[tokio::test]
    async fn rota_el_token_cuando_el_login_coincide() {
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = cuenta("jparga");
        llavero
            .guardar(
                &cuenta.login,
                ClaveSecreto::TokenGithub,
                &Secreto::nuevo("viejo"),
            )
            .expect("guardar token viejo");
        let github = GithubDoble::con_identidad("jparga", None);

        rotar_token(
            &llavero,
            &almacen,
            &cuenta,
            &github,
            Secreto::nuevo("nuevo"),
        )
        .await
        .expect("rotar no falla");

        let guardado = llavero
            .leer(&cuenta.login, ClaveSecreto::TokenGithub)
            .expect("leer")
            .expect("hay token guardado");
        assert_eq!(guardado.exponer(), "nuevo");
        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "cuenta.rotar-token"));
    }

    #[tokio::test]
    async fn no_toca_el_llavero_si_el_login_del_token_no_coincide() {
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = cuenta("jparga");
        llavero
            .guardar(
                &cuenta.login,
                ClaveSecreto::TokenGithub,
                &Secreto::nuevo("viejo"),
            )
            .expect("guardar token viejo");
        let github = GithubDoble::con_identidad("otra-persona", None);

        let resultado = rotar_token(
            &llavero,
            &almacen,
            &cuenta,
            &github,
            Secreto::nuevo("nuevo"),
        )
        .await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::TokenNoCoincideConLogin { .. })
        ));
        let guardado = llavero
            .leer(&cuenta.login, ClaveSecreto::TokenGithub)
            .expect("leer")
            .expect("el token viejo sigue ahí");
        assert_eq!(guardado.exponer(), "viejo");
    }
}
