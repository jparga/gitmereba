//! Datos de entrada para crear un mirror.

use crate::modelo::Nombre;
use crate::secretos::Secreto;

/// Datos para crear un pull-mirror de un repo de GitHub en el Gitea local.
///
/// No deriva `Serialize`: el cuerpo JSON se construye aparte (ver `dto::cuerpo_migrate`)
/// para que el token nunca quede a la vista en un tipo serializable de uso general. Su
/// `Debug` es seguro porque el de [`Secreto`] ya oculta el valor.
#[derive(Debug, Clone)]
pub struct PeticionMirror {
    /// URL de clonado en GitHub (`https://github.com/dueño/repo.git`).
    pub url_clon: String,
    /// Token de lectura de GitHub. Opcional: no hace falta para repos públicos.
    pub token: Option<Secreto>,
    /// Organización o usuario destino en Gitea.
    pub dueno: Nombre,
    pub nombre: Nombre,
    /// `"30m"`, `"1h"`, etc.
    pub intervalo: String,
    pub privado: bool,
    pub descripcion: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_no_muestra_el_token() {
        let peticion = PeticionMirror {
            url_clon: "https://github.com/acme/demo.git".to_string(),
            token: Some(Secreto::nuevo("ghp_secreto_de_verdad")),
            dueno: Nombre::nuevo("acme").unwrap(),
            nombre: Nombre::nuevo("demo").unwrap(),
            intervalo: "30m".to_string(),
            privado: true,
            descripcion: None,
        };
        let texto = format!("{peticion:?}");
        assert!(!texto.contains("ghp_secreto_de_verdad"));
    }
}
