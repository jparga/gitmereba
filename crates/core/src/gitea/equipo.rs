//! Equipos de Gitea usados para dar acceso a la LAN: lectura sobre los mirrors,
//! escritura sobre las organizaciones de contingencia. Ver `crate::cuentas::usuarios_lan`.

/// Permiso de un equipo de Gitea sobre las organizaciones que administra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermisoEquipo {
    Lectura,
    Escritura,
}

impl PermisoEquipo {
    /// Valor del campo `permission` que espera la API de Gitea.
    pub fn como_str(self) -> &'static str {
        match self {
            PermisoEquipo::Lectura => "read",
            PermisoEquipo::Escritura => "write",
        }
    }
}

/// Equipo a asegurar en una organización (ver [`super::ApiGitea::asegurar_equipo`]).
#[derive(Debug, Clone, Copy)]
pub struct DefinicionEquipo {
    pub nombre: &'static str,
    pub permiso: PermisoEquipo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_permiso_se_traduce_al_valor_esperado_por_gitea() {
        assert_eq!(PermisoEquipo::Lectura.como_str(), "read");
        assert_eq!(PermisoEquipo::Escritura.como_str(), "write");
    }
}
