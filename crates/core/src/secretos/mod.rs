//! Secretos: el tipo [`Secreto`] y el acceso al llavero del sistema.

use std::fmt;

use zeroize::Zeroizing;

pub mod llavero;
pub mod sistema;

pub use llavero::{ClaveSecreto, ErrorLlavero, Llavero, LlaveroEnMemoria};
pub use sistema::LlaveroDelSistema;

/// Valor secreto (token o contraseña).
///
/// No implementa `Display` ni `Serialize`, su `Debug` no muestra el contenido y la memoria
/// se borra al soltarlo. El único acceso al valor es [`Secreto::exponer`], para que cada
/// uso sea visible en una revisión.
#[derive(Clone)]
pub struct Secreto(Zeroizing<String>);

impl Secreto {
    pub fn nuevo(valor: impl Into<String>) -> Self {
        Self(Zeroizing::new(valor.into()))
    }

    /// Devuelve el valor en claro. No lo registres ni lo incluyas en errores.
    pub fn exponer(&self) -> &str {
        &self.0
    }

    pub fn esta_vacio(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secreto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secreto(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_no_muestra_el_valor() {
        let secreto = Secreto::nuevo("ghp_abc123");
        let texto = format!("{secreto:?} {:?}", Some(&secreto));
        assert!(!texto.contains("ghp_abc123"));
        assert!(texto.contains("***"));
    }

    #[test]
    fn exponer_devuelve_el_valor() {
        assert_eq!(Secreto::nuevo("x").exponer(), "x");
        assert!(Secreto::nuevo("").esta_vacio());
    }
}
