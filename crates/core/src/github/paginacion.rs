//! Utilidades para seguir la paginación por cabecera `Link` de GitHub.

use url::Url;

/// Tope de seguridad de páginas a seguir en una única llamada.
pub(crate) const MAX_PAGINAS: u32 = 100;

/// Extrae la URL marcada como `rel="next"` de una cabecera `Link`, si la hay.
///
/// Formato esperado: `<url>; rel="next", <url>; rel="last"`.
pub(crate) fn url_siguiente(cabecera: &str) -> Option<String> {
    for enlace in cabecera.split(',') {
        let mut partes = enlace.split(';');
        let url = partes.next()?.trim();
        let es_next = partes.any(|parametro| parametro.trim() == "rel=\"next\"");
        if es_next {
            let url = url.trim_start_matches('<').trim_end_matches('>');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Compara esquema, host y puerto: el mismo origen exigido para seguir la paginación.
pub(crate) fn mismo_origen(base: &Url, otra: &Url) -> bool {
    base.scheme() == otra.scheme()
        && base.host_str() == otra.host_str()
        && base.port_or_known_default() == otra.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrae_la_url_next_entre_varios_enlaces() {
        let cabecera = concat!(
            "<https://api.github.com/user/repos?page=2>; rel=\"next\", ",
            "<https://api.github.com/user/repos?page=5>; rel=\"last\""
        );
        assert_eq!(
            url_siguiente(cabecera),
            Some("https://api.github.com/user/repos?page=2".to_string())
        );
    }

    #[test]
    fn devuelve_none_sin_rel_next() {
        let cabecera = "<https://api.github.com/user/repos?page=1>; rel=\"prev\"";
        assert_eq!(url_siguiente(cabecera), None);
    }

    #[test]
    fn distingue_origenes_por_esquema_host_y_puerto() {
        let base = Url::parse("https://api.github.com/user/repos").unwrap();
        let mismo = Url::parse("https://api.github.com/user/repos?page=2").unwrap();
        let otro_host = Url::parse("https://evil.example/user/repos?page=2").unwrap();
        let otro_puerto = Url::parse("https://api.github.com:8443/user/repos").unwrap();
        assert!(mismo_origen(&base, &mismo));
        assert!(!mismo_origen(&base, &otro_host));
        assert!(!mismo_origen(&base, &otro_puerto));
    }
}
