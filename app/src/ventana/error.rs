//! Error común de los comandos de la ventana: `{ codigo, mensaje }` (contrato con la interfaz).

use gitmereba_core::idioma::{Idioma, Localizable};
use serde::Serialize;

/// Forma con la que se rechaza cualquier comando. `codigo` es estable y apto para la
/// lógica de la interfaz; `mensaje` es el texto en español del error de `core`, que por
/// construcción no contiene secretos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorUi {
    pub codigo: String,
    pub mensaje: String,
}

impl ErrorUi {
    pub fn nuevo(codigo: &str, mensaje: impl Into<String>) -> Self {
        Self {
            codigo: codigo.to_string(),
            mensaje: mensaje.into(),
        }
    }

    /// Deriva `codigo` del nombre de la variante del error (`TokenInvalido` →
    /// `token_invalido`), tomado de su `Debug`, y `mensaje` de su texto en `idioma`.
    ///
    /// Solo se usa con errores de `core`, cuyos `Debug` no contienen secretos (el tipo
    /// `Secreto` se enmascara y los clientes sanean sus detalles).
    pub fn de<E: std::fmt::Debug + Localizable>(error: &E, idioma: Idioma) -> Self {
        Self {
            codigo: codigo_de_variante(&format!("{error:?}")),
            mensaje: error.localizar(idioma),
        }
    }

    /// Error de una librería ajena a `core` (io, ejecutor): no puede ser `Localizable`,
    /// así que conserva su `Display` tal cual, anteponiendo un prefijo ya localizado.
    pub fn externo(codigo: &str, prefijo: Option<&str>, error: &impl std::fmt::Display) -> Self {
        match prefijo {
            Some(prefijo) => Self::nuevo(codigo, format!("{prefijo}: {error}")),
            None => Self::nuevo(codigo, error.to_string()),
        }
    }

    pub fn cuenta_no_encontrada(login: &str, idioma: Idioma) -> Self {
        Self::nuevo(
            "cuenta_no_encontrada",
            match idioma {
                Idioma::Es => format!("no hay ninguna cuenta «{login}»"),
                Idioma::En => format!("there is no account «{login}»"),
            },
        )
    }

    // Provisional: sin uso hasta los comandos de escritura.
    #[allow(dead_code)]
    pub fn repo_no_encontrado(dueno: &str, nombre: &str, idioma: Idioma) -> Self {
        Self::nuevo(
            "repo_no_encontrado",
            match idioma {
                Idioma::Es => format!("no existe el repositorio «{dueno}/{nombre}» en esta cuenta"),
                Idioma::En => {
                    format!("the repository «{dueno}/{nombre}» does not exist in this account")
                }
            },
        )
    }

    #[allow(dead_code)]
    pub fn token_vacio(idioma: Idioma) -> Self {
        Self::nuevo(
            "token_vacio",
            texto(idioma, "falta el token", "the token is missing"),
        )
    }

    pub fn interno(mensaje: impl Into<String>) -> Self {
        Self::nuevo("interno", mensaje)
    }
}

/// Texto propio de `app` (no de `core`) en el idioma pedido.
pub fn texto(idioma: Idioma, es: &str, en: &str) -> String {
    match idioma {
        Idioma::Es => es.to_string(),
        Idioma::En => en.to_string(),
    }
}

/// `Variante(..)`/`Variante { .. }` → `variante` en snake_case. Si el error envuelve a
/// otro (`Github(TokenInvalido)`), gana la variante más interna, que es la que la
/// interfaz sabe tratar.
fn codigo_de_variante(debug: &str) -> String {
    let mut nombres = Vec::new();
    let mut actual = String::new();
    for caracter in debug.chars() {
        if caracter.is_ascii_alphanumeric() {
            actual.push(caracter);
            continue;
        }
        if !actual.is_empty() {
            nombres.push(std::mem::take(&mut actual));
        }
        // Solo se siguen variantes anidadas directamente: `A(B(C…`. Una comilla, una
        // llave o una coma significan que empiezan los datos de la variante.
        if caracter != '(' {
            break;
        }
    }
    if !actual.is_empty() {
        nombres.push(actual);
    }
    let variante = nombres
        .iter()
        .rev()
        .find(|nombre| nombre.chars().next().is_some_and(char::is_uppercase))
        .cloned()
        .unwrap_or_else(|| "interno".to_string());
    a_snake_case(&variante)
}

fn a_snake_case(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len() + 4);
    for (indice, caracter) in texto.chars().enumerate() {
        if caracter.is_ascii_uppercase() {
            if indice > 0 {
                salida.push('_');
            }
            salida.push(caracter.to_ascii_lowercase());
        } else {
            salida.push(caracter);
        }
    }
    salida
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    #[allow(dead_code)]
    enum Interno {
        TokenInvalido,
        RespuestaInesperada { estado: u16 },
        DatosInvalidos(String),
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    enum Externo {
        Github(Interno),
        CarpetaNoVacia,
    }

    #[test]
    fn el_codigo_es_la_variante_en_snake_case() {
        assert_eq!(
            codigo_de_variante(&format!("{:?}", Externo::CarpetaNoVacia)),
            "carpeta_no_vacia"
        );
        assert_eq!(
            codigo_de_variante(&format!(
                "{:?}",
                Interno::RespuestaInesperada { estado: 500 }
            )),
            "respuesta_inesperada"
        );
    }

    #[test]
    fn gana_la_variante_mas_interna() {
        assert_eq!(
            codigo_de_variante(&format!("{:?}", Externo::Github(Interno::TokenInvalido))),
            "token_invalido"
        );
    }

    #[test]
    fn los_datos_de_la_variante_no_cuentan() {
        let error = Externo::Github(Interno::DatosInvalidos("Otra Cosa(Rara)".to_string()));
        assert_eq!(codigo_de_variante(&format!("{error:?}")), "datos_invalidos");
    }

    #[test]
    fn el_mensaje_sale_en_el_idioma_pedido_y_el_codigo_no_cambia() {
        let error = gitmereba_core::github::ErrorGithub::TokenInvalido;
        let es = ErrorUi::de(&error, Idioma::Es);
        let en = ErrorUi::de(&error, Idioma::En);
        assert_eq!(es.codigo, en.codigo);
        assert_eq!(es.codigo, "token_invalido");
        assert_eq!(es.mensaje, error.localizar(Idioma::Es));
        assert_eq!(en.mensaje, error.localizar(Idioma::En));
        assert_ne!(es.mensaje, en.mensaje);
    }

    #[test]
    fn se_serializa_con_codigo_y_mensaje() {
        let json = serde_json::to_value(ErrorUi::token_vacio(Idioma::Es)).unwrap();
        assert_eq!(json["codigo"], "token_vacio");
        assert_eq!(json["mensaje"], "falta el token");
    }
}
