//! Interpretación de la cabecera `github-authentication-token-expiration`.
//!
//! GitHub la envía en dos formas: `2026-12-01 10:00:00 UTC` o, con desplazamiento,
//! `2026-12-01 10:00:00 +0100`. Los tokens fine-grained no la envían.

use time::format_description::{self, OwnedFormatItem};
use time::{OffsetDateTime, PrimitiveDateTime};

fn formato_utc() -> Result<OwnedFormatItem, time::error::InvalidFormatDescription> {
    format_description::parse_owned::<2>("[year]-[month]-[day] [hour]:[minute]:[second] UTC")
}

fn formato_con_offset() -> Result<OwnedFormatItem, time::error::InvalidFormatDescription> {
    format_description::parse_owned::<2>(
        "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]",
    )
}

/// Interpreta la cabecera de caducidad; `None` si el formato no se reconoce.
pub(crate) fn interpretar(valor: &str) -> Option<OffsetDateTime> {
    let valor = valor.trim();

    if let Ok(formato) = formato_utc()
        && let Ok(ingenua) = PrimitiveDateTime::parse(valor, &formato)
    {
        return Some(ingenua.assume_utc());
    }

    if let Ok(formato) = formato_con_offset()
        && let Ok(fecha) = OffsetDateTime::parse(valor, &formato)
    {
        return Some(fecha);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month, Time, UtcOffset};

    fn fecha_hora(dia: u8) -> PrimitiveDateTime {
        let fecha = Date::from_calendar_date(2026, Month::December, dia).unwrap();
        let hora = Time::from_hms(10, 0, 0).unwrap();
        PrimitiveDateTime::new(fecha, hora)
    }

    #[test]
    fn interpreta_el_formato_utc() {
        let fecha = interpretar("2026-12-01 10:00:00 UTC").unwrap();
        assert_eq!(fecha, fecha_hora(1).assume_utc());
    }

    #[test]
    fn interpreta_el_formato_con_desplazamiento() {
        let fecha = interpretar("2026-12-01 10:00:00 +0100").unwrap();
        let esperado = fecha_hora(1).assume_offset(UtcOffset::from_hms(1, 0, 0).unwrap());
        assert_eq!(fecha, esperado);
    }

    #[test]
    fn devuelve_none_con_un_formato_desconocido() {
        assert_eq!(interpretar("hace un rato"), None);
    }
}
