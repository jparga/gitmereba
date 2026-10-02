//! Saneado de mensajes de error antes de meterlos en un [`super::ErrorGitea`].
//!
//! Gitea a veces devuelve en el mensaje de un fallo de migración la URL de clonado con
//! credenciales incrustadas, o el propio token. Nada de eso debe llegar a un error, un
//! `Debug` ni un log.

const LONGITUD_MAXIMA: usize = 500;

/// Sustituye `://usuario@` por `://***@` en cualquier URL que aparezca en el texto.
pub(crate) fn ocultar_credenciales_url(texto: &str) -> String {
    const MARCADOR: &str = "://";
    let mut resultado = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(pos) = resto.find(MARCADOR) {
        let (antes, desde_marcador) = resto.split_at(pos);
        resultado.push_str(antes);
        resultado.push_str(MARCADOR);
        let tras_marcador = &desde_marcador[MARCADOR.len()..];
        match tras_marcador.find('@') {
            // Solo se trata como credenciales si antes de la '@' no hay separadores de
            // ruta ni espacios: así no se confunde con una '@' cualquiera del mensaje.
            Some(arroba)
                if !tras_marcador[..arroba].is_empty()
                    && !tras_marcador[..arroba].contains(['/', ' ', '"', '\'']) =>
            {
                resultado.push_str("***@");
                resto = &tras_marcador[arroba + 1..];
            }
            _ => {
                resto = tras_marcador;
            }
        }
    }
    resultado.push_str(resto);
    resultado
}

/// Elimina toda aparición literal de `secreto` en `texto`, si no está vacío.
pub(crate) fn ocultar_literal(texto: &str, secreto: &str) -> String {
    if secreto.is_empty() {
        texto.to_string()
    } else {
        texto.replace(secreto, "***")
    }
}

/// Recorta a `LONGITUD_MAXIMA` caracteres (no bytes, para no partir un carácter UTF-8).
pub(crate) fn recortar(texto: &str) -> String {
    match texto.char_indices().nth(LONGITUD_MAXIMA) {
        Some((limite, _)) => texto[..limite].to_string(),
        None => texto.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oculta_usuario_y_clave_en_url() {
        let saneado = ocultar_credenciales_url("fallo en https://x:ghp_SECRETO@github.com/a/b");
        assert_eq!(saneado, "fallo en https://***@github.com/a/b");
    }

    #[test]
    fn no_toca_arrobas_que_no_son_credenciales() {
        let saneado = ocultar_credenciales_url("contacta con soporte@ejemplo.com, ver https://x/y");
        assert_eq!(saneado, "contacta con soporte@ejemplo.com, ver https://x/y");
    }

    #[test]
    fn oculta_literal_sustituye_el_token() {
        assert_eq!(
            ocultar_literal("token ghp_x usado", "ghp_x"),
            "token *** usado"
        );
        assert_eq!(ocultar_literal("sin cambios", ""), "sin cambios");
    }

    #[test]
    fn recortar_respeta_limites_utf8() {
        let texto = "á".repeat(600);
        let recortado = recortar(&texto);
        assert_eq!(recortado.chars().count(), LONGITUD_MAXIMA);
    }

    #[test]
    fn recortar_no_toca_textos_cortos() {
        assert_eq!(recortar("corto"), "corto");
    }
}
