//! Umbrales de tiempo usados por [`super::evaluar`].

use time::Duration;

/// Gracia inicial por defecto: cuánto puede tardar un mirror recién creado en hacer su
/// primera sincronización antes de considerarlo un fallo.
pub const GRACIA_INICIAL_POR_DEFECTO: Duration = Duration::minutes(30);

/// Umbral mínimo de obsolescencia, con independencia de lo corto que sea el intervalo de
/// sincronización de la cuenta.
pub const OBSOLETO_MINIMO: Duration = Duration::minutes(60);

/// Umbrales de tiempo para [`super::evaluar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Umbrales {
    /// Tiempo de gracia tras el cual un mirror que no ha sincronizado nunca es un fallo.
    pub gracia_inicial: Duration,
    /// A partir de qué antigüedad de la última sincronización se considera obsoleta.
    pub obsoleto: Duration,
}

impl Umbrales {
    /// Umbrales para una cuenta cuyo intervalo de sincronización es `intervalo_minutos`:
    /// el umbral de obsolescencia es el doble de ese intervalo, con un mínimo de
    /// [`OBSOLETO_MINIMO`].
    pub fn para_intervalo(intervalo_minutos: u32) -> Self {
        let doble = Duration::minutes(i64::from(intervalo_minutos) * 2);
        Self {
            gracia_inicial: GRACIA_INICIAL_POR_DEFECTO,
            obsoleto: if doble > OBSOLETO_MINIMO {
                doble
            } else {
                OBSOLETO_MINIMO
            },
        }
    }
}

impl Default for Umbrales {
    /// Umbrales sin conocer el intervalo de ninguna cuenta: el mínimo siempre aplicable.
    fn default() -> Self {
        Self::para_intervalo(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_umbral_de_obsolescencia_es_el_doble_del_intervalo() {
        let umbrales = Umbrales::para_intervalo(45);
        assert_eq!(umbrales.obsoleto, Duration::minutes(90));
    }

    #[test]
    fn el_umbral_de_obsolescencia_nunca_baja_del_minimo() {
        assert_eq!(Umbrales::para_intervalo(10).obsoleto, OBSOLETO_MINIMO);
        // 2 × 20 = 40 min, por debajo del mínimo de 60.
        assert_eq!(Umbrales::para_intervalo(20).obsoleto, OBSOLETO_MINIMO);
    }

    #[test]
    fn el_doble_del_intervalo_manda_cuando_supera_el_minimo() {
        assert_eq!(Umbrales::para_intervalo(40).obsoleto, Duration::minutes(80));
    }

    #[test]
    fn la_gracia_inicial_por_defecto_es_treinta_minutos() {
        assert_eq!(Umbrales::default().gracia_inicial, Duration::minutes(30));
    }

    #[test]
    fn el_umbral_por_defecto_es_el_minimo() {
        assert_eq!(Umbrales::default().obsoleto, OBSOLETO_MINIMO);
    }
}
