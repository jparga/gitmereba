//! Resumen de una reconciliación, en español.

use crate::contingencia::InformeReconciliacion;

pub(crate) fn resumen(informe: &InformeReconciliacion) -> String {
    let c = informe.cifras();
    let mut partes = vec![format!("{} rama(s) enviada(s)", c.enviadas)];
    if !c.divergentes.is_empty() {
        partes.push(format!(
            "{} rama(s) divergente(s) ({})",
            c.divergentes.len(),
            c.divergentes.join(", ")
        ));
    }
    if !c.fallidas.is_empty() {
        partes.push(format!(
            "{} rama(s) con fallo ({})",
            c.fallidas.len(),
            c.fallidas.join(", ")
        ));
    }
    if !c.borradas.is_empty() {
        partes.push(format!(
            "{} rama(s) borrada(s) en local, conservada(s) en el remoto ({})",
            c.borradas.len(),
            c.borradas.join(", ")
        ));
    }
    partes.push(format!("{} tag(s) enviado(s)", c.tags_enviados));
    if !c.tags_conflicto.is_empty() {
        partes.push(format!(
            "{} tag(s) en conflicto, no tocado(s) ({})",
            c.tags_conflicto.len(),
            c.tags_conflicto.join(", ")
        ));
    }
    let estado = if c.completa {
        "reconciliación completa"
    } else {
        "reconciliación incompleta"
    };
    format!("{estado}: {}", partes.join("; "))
}
