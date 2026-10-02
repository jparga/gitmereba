//! El informe que devuelve [`super::verificar_cuenta`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::modelo::{Cuenta, EstadoRepo};

use super::aviso::Aviso;
use super::evaluar::Diagnostico;

/// Resultado completo de una pasada de verificación de una cuenta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformeVerificacion {
    /// Sin secretos: login, carpeta, puerto, intervalo y alcance.
    pub cuenta: Cuenta,
    #[serde(with = "time::serde::rfc3339")]
    pub momento: OffsetDateTime,
    pub diagnosticos: Vec<Diagnostico>,
    /// Cuántos SHA se han consultado a GitHub en esta pasada.
    pub consultas_sha: usize,
    /// Cuántos `git fsck` se han ejecutado en esta pasada.
    pub fsck_hechos: usize,
    /// Si se ha agotado el límite de peticiones a GitHub durante esta pasada.
    pub limite_api_alcanzado: bool,
    pub avisos: Vec<Aviso>,
}

impl InformeVerificacion {
    /// El estado más urgente entre todos los diagnósticos, si hay alguno.
    pub fn peor_estado(&self) -> Option<EstadoRepo> {
        self.diagnosticos.iter().map(|d| d.estado).min()
    }

    /// Cuántos repos hay en cada estado.
    pub fn conteo_por_estado(&self) -> BTreeMap<EstadoRepo, usize> {
        let mut conteo = BTreeMap::new();
        for diagnostico in &self.diagnosticos {
            *conteo.entry(diagnostico.estado).or_insert(0) += 1;
        }
        conteo
    }

    /// Una línea en español con el resumen de la pasada.
    pub fn resumen(&self) -> String {
        let conteo = self.conteo_por_estado();
        let total = self.diagnosticos.len();
        let fallos = conteo.get(&EstadoRepo::Fallo).copied().unwrap_or(0);
        let obsoletos = conteo.get(&EstadoRepo::Obsoleto).copied().unwrap_or(0);
        let huerfanos = conteo.get(&EstadoRepo::Huerfano).copied().unwrap_or(0);

        let mut resumen = format!(
            "Verificación de «{}»: {total} repo(s), {fallos} fallo(s), {obsoletos} obsoleto(s), \
             {huerfanos} huérfano(s)",
            self.cuenta.login
        );
        if self.limite_api_alcanzado {
            resumen.push_str(", límite de peticiones a GitHub alcanzado");
        }
        if !self.avisos.is_empty() {
            resumen.push_str(&format!(", {} aviso(s)", self.avisos.len()));
        }
        resumen.push('.');
        resumen
    }
}

#[cfg(test)]
mod tests {
    use crate::modelo::IdRepo;
    use crate::verificacion::dobles::pruebas::cuenta;

    use super::*;

    fn diagnostico(nombre: &str, estado: EstadoRepo) -> Diagnostico {
        Diagnostico {
            id: IdRepo {
                dueno: crate::modelo::Nombre::nuevo("jparga").unwrap(),
                nombre: crate::modelo::Nombre::nuevo(nombre).unwrap(),
            },
            estado,
            motivos: Vec::new(),
            desfase: None,
        }
    }

    fn informe_base() -> InformeVerificacion {
        InformeVerificacion {
            cuenta: cuenta("jparga", 30, "/tmp/gitmereba-test"),
            momento: OffsetDateTime::UNIX_EPOCH,
            diagnosticos: Vec::new(),
            consultas_sha: 0,
            fsck_hechos: 0,
            limite_api_alcanzado: false,
            avisos: Vec::new(),
        }
    }

    #[test]
    fn peor_estado_es_none_sin_diagnosticos() {
        assert_eq!(informe_base().peor_estado(), None);
    }

    #[test]
    fn peor_estado_es_el_mas_urgente() {
        let mut informe = informe_base();
        informe
            .diagnosticos
            .push(diagnostico("uno", EstadoRepo::Ok));
        informe
            .diagnosticos
            .push(diagnostico("dos", EstadoRepo::Obsoleto));
        informe
            .diagnosticos
            .push(diagnostico("tres", EstadoRepo::Fallo));
        assert_eq!(informe.peor_estado(), Some(EstadoRepo::Fallo));
    }

    #[test]
    fn conteo_por_estado_agrupa_correctamente() {
        let mut informe = informe_base();
        informe
            .diagnosticos
            .push(diagnostico("uno", EstadoRepo::Ok));
        informe
            .diagnosticos
            .push(diagnostico("dos", EstadoRepo::Ok));
        informe
            .diagnosticos
            .push(diagnostico("tres", EstadoRepo::Fallo));
        let conteo = informe.conteo_por_estado();
        assert_eq!(conteo.get(&EstadoRepo::Ok), Some(&2));
        assert_eq!(conteo.get(&EstadoRepo::Fallo), Some(&1));
        assert_eq!(conteo.get(&EstadoRepo::Obsoleto), None);
    }

    #[test]
    fn resumen_es_una_linea_en_espanol_con_las_cifras() {
        let mut informe = informe_base();
        informe
            .diagnosticos
            .push(diagnostico("uno", EstadoRepo::Fallo));
        informe
            .diagnosticos
            .push(diagnostico("dos", EstadoRepo::Obsoleto));
        informe
            .diagnosticos
            .push(diagnostico("tres", EstadoRepo::Huerfano));
        let resumen = informe.resumen();
        assert!(resumen.contains("jparga"));
        assert!(resumen.contains("3 repo"));
        assert!(resumen.contains("1 fallo"));
        assert!(resumen.contains("1 obsoleto"));
        assert!(resumen.contains("1 huérfano"));
        assert!(resumen.ends_with('.'));
    }

    #[test]
    fn resumen_menciona_el_limite_de_api_y_los_avisos() {
        let mut informe = informe_base();
        informe.limite_api_alcanzado = true;
        informe.avisos.push(Aviso::TokenCaducado);
        let resumen = informe.resumen();
        assert!(resumen.contains("límite de peticiones"));
        assert!(resumen.contains("1 aviso"));
    }
}
