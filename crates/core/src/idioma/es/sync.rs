//! Resumen de una pasada de sincronización, en español.

use crate::sync::InformeSync;

pub(crate) fn resumen(informe: &InformeSync) -> String {
    let c = informe.cifras();
    let mut resumen = format!(
        "Sincronización de «{}»: {} alta(s), {} pausado(s), \
         {} reanudado(s), {} ajuste(s) de intervalo, {} omitido(s)",
        c.login, c.altas, c.pausados, c.reanudados, c.ajustados, c.omitidos
    );
    if c.fallos > 0 {
        resumen.push_str(&format!(", {} fallo(s)", c.fallos));
    }
    if c.listados_fallidos > 0 {
        resumen.push_str(&format!(", {} listado(s) fallido(s)", c.listados_fallidos));
    }
    if c.alerta {
        resumen.push_str(", alerta: demasiados huérfanos, no se ha marcado ninguno");
    }
    resumen.push('.');
    resumen
}
