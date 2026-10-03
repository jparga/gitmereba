//! Textos de los avisos de la verificación, en inglés.

use crate::verificacion::Aviso;

pub(crate) fn aviso(aviso: &Aviso) -> String {
    match aviso {
        Aviso::TokenCaducaPronto { dias } => {
            format!("the GitHub token expires in {dias} day(s)")
        }
        Aviso::TokenCaducado => "the GitHub token has expired".to_string(),
        Aviso::LimiteDePeticiones => {
            "the GitHub request limit has been reached; no more SHAs were checked in this pass"
                .to_string()
        }
        Aviso::RutaFueraDeRepositorios { id } => format!(
            "the local path of “{id}” is outside the repositories folder; it was not touched"
        ),
        Aviso::ErrorRepo { id, mensaje } => format!("“{id}”: {}", mensaje.en),
        Aviso::ErrorIdentidad { mensaje } => {
            format!("could not check the token identity: {}", mensaje.en)
        }
    }
}
