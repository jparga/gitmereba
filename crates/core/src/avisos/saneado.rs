//! Saneado de texto libre antes de meterlo en el cuerpo de una notificación.
//!
//! Los nombres de repositorio y los mensajes de error que llegan a este módulo vienen,
//! en última instancia, de GitHub o de Gitea: no son de confianza. Se quitan los
//! caracteres de control (incluye saltos de línea y tabuladores) y el marcado tipo HTML
//! (`<`, `>`, `&`), porque algunos servidores de notificaciones de escritorio interpretan
//! HTML en el cuerpo del mensaje.

/// Sanea `texto`: quita caracteres de control y `<`, `>`, `&`, y lo recorta a
/// `longitud_maxima` caracteres (no bytes, para no partir un carácter UTF-8 a medias).
pub(super) fn sanear_texto(texto: &str, longitud_maxima: usize) -> String {
    let limpio: String = texto
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | '&'))
        .collect();
    if limpio.chars().count() > longitud_maxima {
        limpio.chars().take(longitud_maxima).collect()
    } else {
        limpio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quita_marcado_tipo_html() {
        assert_eq!(sanear_texto("<b>x</b>", 100), "bx/b");
    }

    #[test]
    fn quita_el_ampersand() {
        assert_eq!(sanear_texto("uno & dos", 100), "uno  dos");
    }

    #[test]
    fn quita_saltos_de_linea_y_tabuladores() {
        assert_eq!(sanear_texto("uno\ndos\ttres", 100), "unodostres");
    }

    #[test]
    fn recorta_a_la_longitud_maxima_en_caracteres() {
        assert_eq!(sanear_texto("abcdef", 3), "abc");
    }

    #[test]
    fn no_rompe_utf8_multibyte_al_recortar() {
        let resultado = sanear_texto("ñññ", 2);
        assert_eq!(resultado.chars().count(), 2);
    }

    #[test]
    fn texto_limpio_no_cambia() {
        assert_eq!(sanear_texto("repo-normal.git", 100), "repo-normal.git");
    }

    #[test]
    fn texto_vacio_sigue_vacio() {
        assert_eq!(sanear_texto("", 100), "");
    }
}
