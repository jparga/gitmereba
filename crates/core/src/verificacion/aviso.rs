//! Avisos de una pasada de verificación que no son el fallo de un mirror concreto, pero
//! conviene mostrar al usuario (caducidad del token, límite de peticiones, anomalías).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::idioma::TextoExterno;
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
    ErrorRepo { id: IdRepo, mensaje: TextoExterno },
    /// No se ha podido consultar la identidad del token en uso, así que no se ha podido
    /// comprobar su caducidad en esta pasada.
    ErrorIdentidad { mensaje: TextoExterno },
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
            Aviso::ErrorRepo { id, mensaje } => write!(f, "«{id}»: {}", mensaje.es),
            Aviso::ErrorIdentidad { mensaje } => write!(
                f,
                "no se ha podido consultar la identidad del token: {}",
                mensaje.es
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::Nombre;

    #[test]
    fn un_error_repo_guardado_antes_con_cadena_se_sigue_leyendo() {
        let antiguo =
            r#"{"error-repo":{"id":{"dueno":"jparga","nombre":"r1"},"mensaje":"sin red"}}"#;
        let aviso: Aviso = serde_json::from_str(antiguo).expect("formato antiguo");
        assert_eq!(
            aviso,
            Aviso::ErrorRepo {
                id: IdRepo {
                    dueno: Nombre::nuevo("jparga").expect("nombre"),
                    nombre: Nombre::nuevo("r1").expect("nombre"),
                },
                mensaje: TextoExterno::literal("sin red"),
            }
        );
        let nuevo = serde_json::to_string(&aviso).expect("serializar");
        let leido: Aviso = serde_json::from_str(&nuevo).expect("formato nuevo");
        assert_eq!(leido, aviso);
    }

    #[test]
    fn un_error_identidad_guardado_antes_con_cadena_se_sigue_leyendo() {
        let antiguo = r#"{"error-identidad":{"mensaje":"sin red"}}"#;
        let aviso: Aviso = serde_json::from_str(antiguo).expect("formato antiguo");
        assert_eq!(
            aviso,
            Aviso::ErrorIdentidad {
                mensaje: TextoExterno::literal("sin red"),
            }
        );
        let nuevo = serde_json::to_string(&aviso).expect("serializar");
        let leido: Aviso = serde_json::from_str(&nuevo).expect("formato nuevo");
        assert_eq!(leido, aviso);
    }

    #[test]
    fn un_error_identidad_se_muestra_en_cada_idioma() {
        use crate::github::ErrorGithub;
        use crate::idioma::{Idioma, Localizable};

        let error = ErrorGithub::TokenInvalido;
        let aviso = Aviso::ErrorIdentidad {
            mensaje: TextoExterno::de(&error),
        };
        let es = aviso.localizar(Idioma::Es);
        assert_eq!(es, aviso.to_string());
        assert_eq!(
            es,
            format!(
                "no se ha podido consultar la identidad del token: {}",
                error
            )
        );
        let en = aviso.localizar(Idioma::En);
        assert!(
            en.starts_with("could not check the token identity: "),
            "{en}"
        );
        assert!(!en.contains(&error.to_string()), "{en}");
    }

    #[test]
    fn los_avisos_se_muestran_en_espanol() {
        assert!(Aviso::TokenCaducado.to_string().contains("token de GitHub"));
        assert!(Aviso::LimiteDePeticiones.to_string().contains("límite"));
    }
}
