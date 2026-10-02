//! Saneado defensivo de texto libre antes de guardarlo.
//!
//! El módulo `almacen` nunca recibe un [`crate::secretos::Secreto`]: el llamador no debe
//! meter secretos en `detalle` ni en `resumen`. Como defensa adicional, antes de guardar
//! cualquier texto libre se sustituyen dos patrones que delatarían una credencial:
//! credenciales embebidas en una URL (`esquema://usuario:clave@host`) y tokens con forma
//! de token de GitHub. No hay dependencia de expresiones regulares: el escaneo es manual.

/// Sanea `texto`: quita credenciales de URL y tokens de GitHub. Se aplica a todo texto
/// libre que se guarda en el almacén (`detalle` de auditoría y de estado, `resumen`).
pub(super) fn sanear_detalle(texto: &str) -> String {
    sanear_tokens_github(&sanear_credenciales_url(texto))
}

/// Sustituye `esquema://usuario:clave@host...` por `esquema://***@host...`.
fn sanear_credenciales_url(texto: &str) -> String {
    let mut resultado = String::with_capacity(texto.len());
    let mut resto = texto;
    loop {
        let Some(posicion) = resto.find("://") else {
            resultado.push_str(resto);
            break;
        };
        let (antes, despues_marcador) = resto.split_at(posicion);
        resultado.push_str(antes);
        let despues_esquema = &despues_marcador[3..];

        let fin_autoridad = despues_esquema
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(despues_esquema.len());
        let posible_autoridad = &despues_esquema[..fin_autoridad];

        match posible_autoridad.rfind('@') {
            Some(posicion_arroba) => {
                resultado.push_str("://***@");
                resultado.push_str(&posible_autoridad[posicion_arroba + '@'.len_utf8()..]);
                resto = &despues_esquema[fin_autoridad..];
            }
            None => {
                resultado.push_str("://");
                resto = despues_esquema;
            }
        }
    }
    resultado
}

/// Prefijos de token clásico de GitHub: `ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`.
const PREFIJOS_TOKEN_CLASICO: [&str; 5] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"];
/// Prefijo de token «fine-grained» de GitHub.
const PREFIJO_TOKEN_FINO: &str = "github_pat_";
/// Longitud mínima del resto del token tras el prefijo, para considerarlo un token real.
const LONGITUD_MINIMA_RESTO: usize = 20;

/// Sustituye cualquier token con forma `gh[pousr]_[A-Za-z0-9]{20,}` o
/// `github_pat_[A-Za-z0-9_]{20,}` por `***`.
fn sanear_tokens_github(texto: &str) -> String {
    let caracteres: Vec<char> = texto.chars().collect();
    let mut resultado = String::with_capacity(texto.len());
    let mut indice = 0;
    while indice < caracteres.len() {
        if let Some(longitud) = coincidencia_token(&caracteres[indice..]) {
            resultado.push_str("***");
            indice += longitud;
        } else {
            resultado.push(caracteres[indice]);
            indice += 1;
        }
    }
    resultado
}

/// Si `resto` empieza por un token de GitHub reconocible, devuelve su longitud en
/// caracteres.
fn coincidencia_token(resto: &[char]) -> Option<usize> {
    for prefijo in PREFIJOS_TOKEN_CLASICO {
        if empieza_por(resto, prefijo) {
            let base = prefijo.chars().count();
            let extra = contar_mientras(&resto[base..], |c| c.is_ascii_alphanumeric());
            if extra >= LONGITUD_MINIMA_RESTO {
                return Some(base + extra);
            }
        }
    }
    if empieza_por(resto, PREFIJO_TOKEN_FINO) {
        let base = PREFIJO_TOKEN_FINO.chars().count();
        let extra = contar_mientras(&resto[base..], |c| c.is_ascii_alphanumeric() || c == '_');
        if extra >= LONGITUD_MINIMA_RESTO {
            return Some(base + extra);
        }
    }
    None
}

/// Compara los primeros caracteres de `resto` con `prefijo`.
fn empieza_por(resto: &[char], prefijo: &str) -> bool {
    let prefijo: Vec<char> = prefijo.chars().collect();
    resto.len() >= prefijo.len() && resto[..prefijo.len()] == prefijo[..]
}

/// Cuenta cuántos caracteres consecutivos desde el principio cumplen `condicion`.
fn contar_mientras(resto: &[char], condicion: impl Fn(char) -> bool) -> usize {
    resto.iter().take_while(|c| condicion(**c)).count()
}

/// Recorta `texto` a `limite` caracteres (no bytes) y lo sanea.
pub(super) fn sanear_y_recortar(texto: &str, limite: usize) -> String {
    let recortado: String = texto.chars().take(limite).collect();
    sanear_detalle(&recortado)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanea_credenciales_de_una_url() {
        let entrada = "fallo al clonar https://usuario:token-secreto@github.com/x/y.git";
        let saneado = sanear_detalle(entrada);
        assert_eq!(saneado, "fallo al clonar https://***@github.com/x/y.git");
        assert!(!saneado.contains("token-secreto"));
    }

    #[test]
    fn no_toca_urls_sin_credenciales() {
        let entrada = "ver https://github.com/x/y";
        assert_eq!(sanear_detalle(entrada), entrada);
    }

    #[test]
    fn sanea_token_clasico_de_github() {
        let entrada = "token usado: ghp_abcdefghijklmnopqrstuvwxyz0123456789 fin";
        let saneado = sanear_detalle(entrada);
        assert_eq!(saneado, "token usado: *** fin");
    }

    #[test]
    fn sanea_token_fino_de_github() {
        let entrada = "github_pat_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
        let saneado = sanear_detalle(entrada);
        assert_eq!(saneado, "***");
    }

    #[test]
    fn no_sanea_tokens_demasiado_cortos() {
        let entrada = "ghp_corto";
        assert_eq!(sanear_detalle(entrada), entrada);
    }

    #[test]
    fn recorta_a_dos_mil_caracteres() {
        let entrada = "a".repeat(3000);
        assert_eq!(sanear_y_recortar(&entrada, 2000).chars().count(), 2000);
    }

    #[test]
    fn la_inyeccion_sql_no_se_altera() {
        let entrada = "'); DROP TABLE auditoria;--";
        assert_eq!(sanear_detalle(entrada), entrada);
    }
}
