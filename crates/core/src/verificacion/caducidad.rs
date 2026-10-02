//! Aviso de caducidad del token de GitHub.

use time::OffsetDateTime;

use super::aviso::Aviso;

/// Días de aviso por defecto antes de que caduque el token.
pub const DIAS_AVISO_POR_DEFECTO: u32 = 14;

/// Aviso de caducidad del token, si procede.
///
/// `None` si no se conoce la caducidad (tokens fine-grained sin fecha) o si todavía
/// faltan más de `dias_aviso` días para que llegue.
pub fn aviso_caducidad(
    caduca: Option<OffsetDateTime>,
    ahora: OffsetDateTime,
    dias_aviso: u32,
) -> Option<Aviso> {
    let caduca = caduca?;
    if caduca <= ahora {
        return Some(Aviso::TokenCaducado);
    }
    let dias_restantes = (caduca - ahora).whole_days();
    if dias_restantes <= i64::from(dias_aviso) {
        let dias = u32::try_from(dias_restantes).unwrap_or(0);
        Some(Aviso::TokenCaducaPronto { dias })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    const AHORA: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    #[test]
    fn sin_fecha_de_caducidad_no_hay_aviso() {
        assert_eq!(aviso_caducidad(None, AHORA, 14), None);
    }

    #[test]
    fn caducidad_lejana_no_avisa() {
        let caduca = AHORA + Duration::days(15);
        assert_eq!(aviso_caducidad(Some(caduca), AHORA, 14), None);
    }

    #[test]
    fn caducidad_exactamente_en_el_borde_de_catorce_dias_avisa() {
        let caduca = AHORA + Duration::days(14);
        assert_eq!(
            aviso_caducidad(Some(caduca), AHORA, 14),
            Some(Aviso::TokenCaducaPronto { dias: 14 })
        );
    }

    #[test]
    fn caducidad_a_menos_de_un_dia_avisa_con_cero_dias() {
        let caduca = AHORA + Duration::hours(3);
        assert_eq!(
            aviso_caducidad(Some(caduca), AHORA, 14),
            Some(Aviso::TokenCaducaPronto { dias: 0 })
        );
    }

    #[test]
    fn caducidad_pasada_es_token_caducado() {
        let caduca = AHORA - Duration::hours(1);
        assert_eq!(
            aviso_caducidad(Some(caduca), AHORA, 14),
            Some(Aviso::TokenCaducado)
        );
    }

    #[test]
    fn caducidad_exactamente_ahora_es_token_caducado() {
        assert_eq!(
            aviso_caducidad(Some(AHORA), AHORA, 14),
            Some(Aviso::TokenCaducado)
        );
    }
}
