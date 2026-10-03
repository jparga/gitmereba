//! Resumen de una pasada de sincronización, en inglés.

use crate::sync::InformeSync;

pub(crate) fn resumen(informe: &InformeSync) -> String {
    let c = informe.cifras();
    let mut resumen = format!(
        "Sync of “{}”: {} creation(s), {} paused, {} resumed, \
         {} interval change(s), {} skipped",
        c.login, c.altas, c.pausados, c.reanudados, c.ajustados, c.omitidos
    );
    if c.fallos > 0 {
        resumen.push_str(&format!(", {} failure(s)", c.fallos));
    }
    if c.listados_fallidos > 0 {
        resumen.push_str(&format!(", {} failed listing(s)", c.listados_fallidos));
    }
    if c.alerta {
        resumen.push_str(", alert: too many orphans, none have been marked");
    }
    resumen.push('.');
    resumen
}
