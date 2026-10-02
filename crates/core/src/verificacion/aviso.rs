//! Avisos de una pasada de verificación que no son el fallo de un mirror concreto, pero
//! conviene mostrar al usuario (caducidad del token, límite de peticiones, anomalías).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::modelo::IdRepo;

/// Algo que conviene que el usuario sepa, sin ser el fallo de un mirror concreto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Aviso {
    /// El token de GitHub caduca en `dias` días o menos.
    TokenCaducaPronto { dias: u32 },
    /// El token de GitHub ya ha caducado.
    TokenCaducado,
    /// GitHub ha agotado el límite de peticiones: se ha dejado de consultar SHAs en esta
    /// pasada, sin marcar ningún mirror como fallo por ello.
    LimiteDePeticiones,
    /// La ruta calculada del bare de un mirror quedaría fuera de `gitea_repositorios()`
    /// (posible enlace simbólico): no se ha ejecutado ningún comando de git sobre ella.
    RutaFueraDeRepositorios { id: IdRepo },
    /// Fallo puntual al consultar el SHA remoto de un repo (que no sea agotar el límite
    /// de peticiones): no se marca un fallo del mirror por ello.
    ErrorRepo { id: IdRepo, mensaje: String },
    /// No se ha podido consultar la identidad del token en uso, así que no se ha podido
    /// comprobar su caducidad en esta pasada.
    ErrorIdentidad { mensaje: String },
}

impl fmt::Display for Aviso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Aviso::TokenCaducaPronto { dias } => {
                write!(f, "el token de GitHub caduca en {dias} día(s)")
            }
            Aviso::TokenCaducado => write!(f, "el token de GitHub ha caducado"),
            Aviso::LimiteDePeticiones => write!(
                f,
                "se ha agotado el límite de peticiones a GitHub; no se han consultado más SHA en esta pasada"
            ),
            Aviso::RutaFueraDeRepositorios { id } => write!(
                f,
                "la ruta local de «{id}» queda fuera de la carpeta de repositorios; no se ha tocado"
            ),
            Aviso::ErrorRepo { id, mensaje } => write!(f, "«{id}»: {mensaje}"),
            Aviso::ErrorIdentidad { mensaje } => write!(
                f,
                "no se ha podido consultar la identidad del token: {mensaje}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_avisos_se_muestran_en_espanol() {
        assert!(Aviso::TokenCaducado.to_string().contains("token de GitHub"));
        assert!(Aviso::LimiteDePeticiones.to_string().contains("límite"));
    }
}
