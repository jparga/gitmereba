//! `gitmereba doctor`: comprobaciones de salud del sistema y de cada cuenta.

use std::io::Write;

use super::abrir_almacen;
use gitmereba_core::config::Rutas;
use gitmereba_core::cuentas::{self, Contexto, NivelComprobacion};
use gitmereba_core::idioma::{Localizable, idioma_actual};
use gitmereba_core::secretos::LlaveroDelSistema;

use crate::salida;
use crate::textos_cli::TextoCli;

pub async fn ejecutar(rutas: &Rutas) -> u8 {
    let idioma = idioma_actual(rutas);
    let llavero = match LlaveroDelSistema::nuevo() {
        Ok(llavero) => llavero,
        Err(error) => {
            return fallo(&TextoCli::LlaveroInaccesible(error.localizar(idioma)).texto(idioma));
        }
    };
    let almacen = match abrir_almacen(rutas) {
        Ok(almacen) => almacen,
        Err(error) => {
            return fallo(&TextoCli::AlmacenInaccesible(error.localizar(idioma)).texto(idioma));
        }
    };
    let contexto = Contexto::nuevo(rutas, &llavero, &almacen);

    let informe = cuentas::doctor(&contexto).await;

    let mut stdout = std::io::stdout().lock();
    for comprobacion in &informe.comprobaciones {
        let marca = match comprobacion.nivel {
            NivelComprobacion::Ok => TextoCli::MarcaOk,
            NivelComprobacion::Aviso => TextoCli::MarcaAviso,
            NivelComprobacion::Fallo => TextoCli::MarcaFallo,
        }
        .texto(idioma);
        salida::linea(
            &mut stdout,
            &format!(
                "{marca} {}: {}",
                comprobacion.nombre.localizar(idioma),
                comprobacion.mensaje.localizar(idioma)
            ),
        );
        if let Some(consejo) = &comprobacion.consejo {
            salida::linea(
                &mut stdout,
                &format!(
                    "        {}",
                    TextoCli::Consejo(consejo.localizar(idioma)).texto(idioma)
                ),
            );
        }
    }

    if informe.ok() { 0 } else { 1 }
}

fn fallo(mensaje: &str) -> u8 {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "error: {mensaje}");
    1
}
