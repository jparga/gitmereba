//! Elección de un puerto TCP libre en `127.0.0.1` para el Gitea de una cuenta.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

use super::error::ErrorInstancia;

/// Rango de puertos altos donde se busca un hueco libre.
const RANGO: std::ops::RangeInclusive<u16> = 33000..=33999;

/// Primer puerto de `RANGO` en el que se puede escuchar en `127.0.0.1` y que no está
/// en `excluidos` (los puertos ya asignados a otras cuentas).
pub fn puerto_libre(excluidos: &[u16]) -> Result<u16, ErrorInstancia> {
    for puerto in RANGO {
        if excluidos.contains(&puerto) {
            continue;
        }
        let direccion = SocketAddrV4::new(Ipv4Addr::LOCALHOST, puerto);
        if TcpListener::bind(direccion).is_ok() {
            return Ok(puerto);
        }
    }
    Err(ErrorInstancia::SinPuertoLibre)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encuentra_un_puerto_libre_dentro_del_rango() {
        let puerto = puerto_libre(&[]).expect("hay un puerto libre");
        assert!(RANGO.contains(&puerto));
    }

    #[test]
    fn salta_un_puerto_ya_ocupado() {
        let ocupado = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 33017))
            .expect("puerto de prueba libre en este entorno");

        let elegido = puerto_libre(&[]).expect("hay un puerto libre");

        assert_ne!(elegido, 33017);
        drop(ocupado);
    }

    #[test]
    fn respeta_la_lista_de_exclusion() {
        let primero = puerto_libre(&[]).expect("hay un puerto libre");
        let siguiente = puerto_libre(&[primero]).expect("hay otro puerto libre");
        assert_ne!(primero, siguiente);
    }
}
