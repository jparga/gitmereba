//! Listado de las cuentas dadas de alta.

use crate::config::{self, RutasCuenta};
use crate::modelo::Cuenta;
use crate::secretos::Llavero;

use super::contexto::Contexto;
use super::error::ErrorCuentas;

/// Todas las cuentas del índice, ordenadas por login.
pub fn listar<L: Llavero>(contexto: &Contexto<'_, L>) -> Result<Vec<Cuenta>, ErrorCuentas> {
    let indice = config::leer_indice_cuentas(contexto.rutas)?;
    let mut cuentas = Vec::with_capacity(indice.cuentas.len());
    for entrada in indice.cuentas.values() {
        let rutas_cuenta = RutasCuenta::nueva(&entrada.carpeta);
        cuentas.push(config::leer_cuenta(&rutas_cuenta)?);
    }
    cuentas.sort_by(|a, b| a.login.cmp(&b.login));
    Ok(cuentas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::config::Rutas;
    use crate::secretos::LlaveroEnMemoria;

    #[test]
    fn listar_sin_cuentas_devuelve_vacio() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);

        assert!(listar(&contexto).expect("listar no falla").is_empty());
    }
}
