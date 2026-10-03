//! Resumen de una reconciliación, en inglés.

use crate::contingencia::InformeReconciliacion;

pub(crate) fn resumen(informe: &InformeReconciliacion) -> String {
    let c = informe.cifras();
    let mut partes = vec![format!("{} branch(es) sent", c.enviadas)];
    if !c.divergentes.is_empty() {
        partes.push(format!(
            "{} diverged branch(es) ({})",
            c.divergentes.len(),
            c.divergentes.join(", ")
        ));
    }
    if !c.fallidas.is_empty() {
        partes.push(format!(
            "{} failed branch(es) ({})",
            c.fallidas.len(),
            c.fallidas.join(", ")
        ));
    }
    if !c.borradas.is_empty() {
        partes.push(format!(
            "{} branch(es) deleted locally, kept on the remote ({})",
            c.borradas.len(),
            c.borradas.join(", ")
        ));
    }
    partes.push(format!("{} tag(s) sent", c.tags_enviados));
    if !c.tags_conflicto.is_empty() {
        partes.push(format!(
            "{} conflicting tag(s), left untouched ({})",
            c.tags_conflicto.len(),
            c.tags_conflicto.join(", ")
        ));
    }
    let estado = if c.completa {
        "reconciliation complete"
    } else {
        "incomplete reconciliation"
    };
    format!("{estado}: {}", partes.join("; "))
}
