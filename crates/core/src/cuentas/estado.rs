//! Estado de una cuenta: para `gitmereba status` y para la tarjeta de resumen.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::almacen::{Almacen, Sincronizacion};
use crate::gitea::ApiGitea;
use crate::modelo::{Cuenta, EstadoRepo, Nombre};
use crate::secretos::{ClaveSecreto, Llavero, Secreto};
use crate::sync::InformeSync;

use super::comun::cargar_cuenta;
use super::contexto::Contexto;
use super::error::ErrorCuentas;

/// Cuántos repos guardados hay en cada [`EstadoRepo`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConteoRepos {
    pub ok: usize,
    pub fallo: usize,
    pub obsoleto: usize,
    pub huerfano: usize,
    pub contingencia: usize,
    pub excluido: usize,
}

impl ConteoRepos {
    pub fn total(&self) -> usize {
        self.ok + self.fallo + self.obsoleto + self.huerfano + self.contingencia + self.excluido
    }

    fn incrementar(&mut self, estado: EstadoRepo) {
        match estado {
            EstadoRepo::Ok => self.ok += 1,
            EstadoRepo::Fallo => self.fallo += 1,
            EstadoRepo::Obsoleto => self.obsoleto += 1,
            EstadoRepo::Huerfano => self.huerfano += 1,
            EstadoRepo::Contingencia => self.contingencia += 1,
            EstadoRepo::Excluido => self.excluido += 1,
        }
    }
}

/// Foto del estado de una cuenta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstadoCuenta {
    pub cuenta: Cuenta,
    pub gitea_responde: bool,
    pub version_gitea: Option<String>,
    pub repos: ConteoRepos,
    pub ultima_sincronizacion: Option<Sincronizacion>,
    pub ultima_correcta: Option<Sincronizacion>,
    /// Caducidad del token de GitHub conocida por la última sincronización.
    ///
    /// Decisión: no se vuelve a preguntar a GitHub (eso obligaría a `status` a hacer una
    /// petición de red y a tener el token a mano). En su lugar se recalcula leyendo
    /// `detalle_json` de la última sincronización registrada, donde
    /// [`super::sincronizar`] guarda el [`InformeSync`] completo (incluida
    /// `caduca_token`). Si nunca se ha sincronizado, es `None`.
    #[serde(with = "time::serde::rfc3339::option")]
    pub caduca_token: Option<OffsetDateTime>,
}

/// Estado de `login`: si Gitea responde, su versión, cuántos repos hay en cada estado
/// guardado y el histórico más reciente.
pub async fn estado<L: Llavero>(
    contexto: &Contexto<'_, L>,
    login: &Nombre,
) -> Result<EstadoCuenta, ErrorCuentas> {
    let cuenta = cargar_cuenta(contexto.rutas, login)?;
    let token_gitea = contexto
        .llavero
        .leer(login, ClaveSecreto::TokenGitea)?
        .unwrap_or_else(|| Secreto::nuevo(""));
    let gitea = super::comun::cliente_gitea_de_cuenta(&cuenta, token_gitea)?;

    calcular_estado(contexto.almacen, cuenta, &gitea).await
}

/// Núcleo genérico sobre [`ApiGitea`], para poder probarlo con
/// [`crate::cuentas::dobles::GiteaDoble`].
pub(super) async fn calcular_estado<T: ApiGitea>(
    almacen: &Almacen,
    cuenta: Cuenta,
    gitea: &T,
) -> Result<EstadoCuenta, ErrorCuentas> {
    let gitea_responde = gitea.salud().await.map_err(ErrorCuentas::from)?;
    let version_gitea = if gitea_responde {
        gitea.version().await.ok()
    } else {
        None
    };

    let mut repos = ConteoRepos::default();
    for estado_guardado in almacen.estados_de(&cuenta.login)? {
        repos.incrementar(estado_guardado.estado);
    }

    let ultima_sincronizacion = almacen
        .ultimas_sincronizaciones(Some(&cuenta.login), 1)?
        .into_iter()
        .next();
    let ultima_correcta = almacen.ultima_correcta(&cuenta.login)?;
    let caduca_token = ultima_sincronizacion
        .as_ref()
        .and_then(|s| s.detalle_json.as_deref())
        .and_then(|json| serde_json::from_str::<InformeSync>(json).ok())
        .and_then(|informe| informe.caduca_token);

    Ok(EstadoCuenta {
        cuenta,
        gitea_responde,
        version_gitea,
        repos,
        ultima_sincronizacion,
        ultima_correcta,
        caduca_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::cuentas::dobles::GiteaDoble;
    use crate::modelo::{Alcance, IdRepo};
    use std::path::PathBuf;

    fn cuenta_de_prueba() -> Cuenta {
        Cuenta {
            login: Nombre::nuevo("jparga").expect("nombre"),
            carpeta: PathBuf::from("/tmp/gitmereba-test-estado"),
            puerto: 33099,
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
    async fn gitea_parado_no_es_un_error_solo_gitea_responde_false() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let gitea = GiteaDoble::nueva();
        gitea.con_salud(false);

        let estado = calcular_estado(&almacen, cuenta_de_prueba(), &gitea)
            .await
            .expect("calcular_estado no falla");

        assert!(!estado.gitea_responde);
        assert!(estado.version_gitea.is_none());
    }

    #[tokio::test]
    async fn cuenta_los_repos_por_estado() {
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let cuenta = cuenta_de_prueba();
        almacen
            .guardar_estado_repo(
                &cuenta.login,
                &IdRepo {
                    dueno: cuenta.login.clone(),
                    nombre: Nombre::nuevo("uno").unwrap(),
                },
                EstadoRepo::Ok,
                None,
            )
            .expect("guardar");
        almacen
            .guardar_estado_repo(
                &cuenta.login,
                &IdRepo {
                    dueno: cuenta.login.clone(),
                    nombre: Nombre::nuevo("dos").unwrap(),
                },
                EstadoRepo::Huerfano,
                None,
            )
            .expect("guardar");
        let gitea = GiteaDoble::nueva();

        let estado = calcular_estado(&almacen, cuenta, &gitea)
            .await
            .expect("calcular_estado no falla");

        assert_eq!(estado.repos.ok, 1);
        assert_eq!(estado.repos.huerfano, 1);
        assert_eq!(estado.repos.total(), 2);
    }
}
