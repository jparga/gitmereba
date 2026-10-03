//! El informe que devuelve [`super::ejecutar`].

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::idioma::{Idioma, Localizable};
use crate::modelo::{Cuenta, IdRepo, Nombre, RepoOrigen};

use super::plan::{Accion, AlertaPlan, Plan};

/// Qué tipo de acción se aplicó sobre un repo, para el informe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TipoAccion {
    CrearMirror,
    MarcarHuerfano,
    Reanudar,
    AjustarIntervalo,
    /// Sincronización forzada tras crear, reanudar o ajustar, cuando `opciones.forzar_sync`.
    ForzarSincronizacion,
}

/// El resultado de aplicar una acción a un repo concreto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultadoAccion {
    Ok,
    /// Mensaje de error saneado: nunca contiene el token.
    Error(String),
}

/// Resultado de aplicar una acción del plan a un repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultadoRepo {
    pub id: IdRepo,
    pub accion: TipoAccion,
    pub resultado: ResultadoAccion,
}

/// Fallo al listar los repos de un dueño (el login de la cuenta o una de sus
/// organizaciones en alcance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorListado {
    pub dueno: Nombre,
    /// Mensaje saneado: nunca contiene el token.
    pub mensaje: String,
}

/// Resultado completo de una pasada de sincronización.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformeSync {
    /// Sin secretos: login, carpeta, puerto, intervalo y alcance.
    pub cuenta: Cuenta,
    #[serde(with = "time::serde::rfc3339")]
    pub inicio: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub fin: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub caduca_token: Option<OffsetDateTime>,
    pub plan: Plan,
    pub resultados: Vec<ResultadoRepo>,
    pub errores_de_listado: Vec<ErrorListado>,
    pub alerta: Option<AlertaPlan>,
    /// Listado de GitHub de esta pasada, para que la verificación no vuelva a pedirlo.
    /// No se serializa: no debe engordar el histórico.
    #[serde(skip)]
    pub origen: Vec<RepoOrigen>,
}

impl InformeSync {
    /// Si hay algo que requiera atención: fallos al aplicar una acción, errores al
    /// listar un dueño, o una alerta del plan.
    pub fn hay_fallos(&self) -> bool {
        self.alerta.is_some()
            || !self.errores_de_listado.is_empty()
            || self
                .resultados
                .iter()
                .any(|r| matches!(r.resultado, ResultadoAccion::Error(_)))
    }

    /// Una línea en español con el resumen de la pasada. Es el texto que se guarda en el
    /// histórico y la auditoría; para mostrarlo al usuario, [`Localizable::localizar`].
    pub fn resumen(&self) -> String {
        self.localizar(Idioma::Es)
    }

    /// Las cifras de la pasada, que los catálogos de idioma convierten en texto.
    pub(crate) fn cifras(&self) -> CifrasResumen<'_> {
        CifrasResumen {
            login: self.cuenta.login.as_str(),
            altas: contar(&self.plan.acciones, |a| matches!(a, Accion::CrearMirror(_))),
            pausados: contar(&self.plan.acciones, |a| {
                matches!(a, Accion::MarcarHuerfano(_))
            }),
            reanudados: contar(&self.plan.acciones, |a| matches!(a, Accion::Reanudar(_))),
            ajustados: contar(&self.plan.acciones, |a| {
                matches!(a, Accion::AjustarIntervalo { .. })
            }),
            omitidos: self.plan.omitidos.len(),
            fallos: self
                .resultados
                .iter()
                .filter(|r| matches!(r.resultado, ResultadoAccion::Error(_)))
                .count(),
            listados_fallidos: self.errores_de_listado.len(),
            alerta: self.alerta.is_some(),
        }
    }
}

/// Cifras de un [`InformeSync`] para redactar su resumen.
pub(crate) struct CifrasResumen<'a> {
    pub login: &'a str,
    pub altas: usize,
    pub pausados: usize,
    pub reanudados: usize,
    pub ajustados: usize,
    pub omitidos: usize,
    pub fallos: usize,
    pub listados_fallidos: usize,
    pub alerta: bool,
}

fn contar<F: Fn(&Accion) -> bool>(acciones: &[Accion], f: F) -> usize {
    acciones.iter().filter(|a| f(a)).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::dobles::pruebas::{cuenta, id, repo_origen};

    fn informe_base() -> InformeSync {
        InformeSync {
            cuenta: cuenta("jparga", false, &[], &[], 30),
            inicio: OffsetDateTime::UNIX_EPOCH,
            fin: OffsetDateTime::UNIX_EPOCH,
            caduca_token: None,
            plan: Plan::default(),
            resultados: Vec::new(),
            errores_de_listado: Vec::new(),
            alerta: None,
            origen: Vec::new(),
        }
    }

    #[test]
    fn sin_fallos_ni_alerta_no_hay_fallos() {
        assert!(!informe_base().hay_fallos());
    }

    #[test]
    fn un_resultado_con_error_cuenta_como_fallo() {
        let mut informe = informe_base();
        informe.resultados.push(ResultadoRepo {
            id: id("jparga", "repo1"),
            accion: TipoAccion::CrearMirror,
            resultado: ResultadoAccion::Error("boom".to_string()),
        });
        assert!(informe.hay_fallos());
    }

    #[test]
    fn un_error_de_listado_cuenta_como_fallo() {
        let mut informe = informe_base();
        informe.errores_de_listado.push(ErrorListado {
            dueno: id("jparga", "x").dueno,
            mensaje: "fallo".to_string(),
        });
        assert!(informe.hay_fallos());
    }

    #[test]
    fn una_alerta_cuenta_como_fallo() {
        let mut informe = informe_base();
        informe.alerta = Some(AlertaPlan::DemasiadosHuerfanos {
            candidatos: 5,
            total_mirrors: 6,
            listado_github_vacio: false,
        });
        assert!(informe.hay_fallos());
    }

    #[test]
    fn resumen_es_una_linea_en_espanol_con_las_cifras() {
        let mut informe = informe_base();
        informe.plan.acciones.push(Accion::CrearMirror(repo_origen(
            "jparga", "nuevo", false, false,
        )));
        informe
            .plan
            .acciones
            .push(Accion::MarcarHuerfano(id("jparga", "viejo")));
        informe.resultados.push(ResultadoRepo {
            id: id("jparga", "nuevo"),
            accion: TipoAccion::CrearMirror,
            resultado: ResultadoAccion::Ok,
        });
        let resumen = informe.resumen();
        assert!(resumen.contains("jparga"));
        assert!(resumen.contains("1 alta"));
        assert!(resumen.contains("1 pausado"));
        assert!(resumen.ends_with('.'));
    }

    #[test]
    fn el_resumen_sale_en_el_idioma_pedido() {
        use crate::idioma::{Idioma, Localizable};
        let mut informe = informe_base();
        informe.plan.acciones.push(Accion::CrearMirror(repo_origen(
            "jparga", "nuevo", false, false,
        )));
        informe.resultados.push(ResultadoRepo {
            id: id("jparga", "nuevo"),
            accion: TipoAccion::CrearMirror,
            resultado: ResultadoAccion::Error("boom".to_string()),
        });
        informe.alerta = Some(AlertaPlan::DemasiadosHuerfanos {
            candidatos: 5,
            total_mirrors: 6,
            listado_github_vacio: false,
        });
        informe.errores_de_listado.push(ErrorListado {
            dueno: id("jparga", "x").dueno,
            mensaje: "fallo".to_string(),
        });
        assert_eq!(
            informe.localizar(Idioma::Es),
            "Sincronización de «jparga»: 1 alta(s), 0 pausado(s), 0 reanudado(s), \
             0 ajuste(s) de intervalo, 0 omitido(s), 1 fallo(s), 1 listado(s) fallido(s), \
             alerta: demasiados huérfanos, no se ha marcado ninguno."
        );
        assert_eq!(informe.localizar(Idioma::Es), informe.resumen());
        assert_eq!(
            informe.localizar(Idioma::En),
            "Sync of \"jparga\": 1 creation(s), 0 paused, 0 resumed, 0 interval change(s), \
             0 skipped, 1 failure(s), 1 failed listing(s), \
             alert: too many orphans, none have been marked."
        );
    }
}
