//! `gitmereba status`: tabla legible o JSON del estado de una cuenta o de todas.

use std::io::Write;

use super::abrir_almacen;
use gitmereba_core::config::Rutas;
use gitmereba_core::cuentas::{self, Contexto, EstadoCuenta};
use gitmereba_core::modelo::Nombre;
use gitmereba_core::secretos::LlaveroDelSistema;

use crate::cli::StatusArgs;
use crate::salida;

use super::CODIGO_ERROR_DE_USO;

pub async fn ejecutar(args: StatusArgs, rutas: &Rutas) -> u8 {
    let llavero = match LlaveroDelSistema::nuevo() {
        Ok(llavero) => llavero,
        Err(error) => return fallo(&format!("no se pudo acceder al llavero: {error}")),
    };
    let almacen = match abrir_almacen(rutas) {
        Ok(almacen) => almacen,
        Err(error) => return fallo(&format!("no se pudo abrir el almacén: {error}")),
    };
    let contexto = Contexto::nuevo(rutas, &llavero, &almacen);

    let estados = match &args.login {
        Some(login) => {
            let login = match Nombre::nuevo(login.as_str()) {
                Ok(login) => login,
                Err(error) => return fallo_uso(&format!("login inválido: {error}")),
            };
            match cuentas::estado(&contexto, &login).await {
                Ok(estado) => vec![estado],
                Err(error) => return emitir_error(&error),
            }
        }
        None => {
            let cuentas_dadas_de_alta = match cuentas::listar(&contexto) {
                Ok(cuentas) => cuentas,
                Err(error) => return emitir_error(&error),
            };
            let mut estados = Vec::with_capacity(cuentas_dadas_de_alta.len());
            for cuenta in cuentas_dadas_de_alta {
                match cuentas::estado(&contexto, &cuenta.login).await {
                    Ok(estado) => estados.push(estado),
                    Err(error) => return emitir_error(&error),
                }
            }
            estados
        }
    };

    if args.json {
        imprimir_json(&estados)
    } else {
        imprimir_tabla(&estados);
        0
    }
}

fn imprimir_json(estados: &[EstadoCuenta]) -> u8 {
    match serde_json::to_string_pretty(estados) {
        Ok(json) => {
            let mut stdout = std::io::stdout().lock();
            salida::linea(&mut stdout, &json);
            0
        }
        Err(error) => fallo(&format!("no se pudo serializar el estado: {error}")),
    }
}

fn imprimir_tabla(estados: &[EstadoCuenta]) {
    let mut stdout = std::io::stdout().lock();
    if estados.is_empty() {
        salida::linea(&mut stdout, "No hay ninguna cuenta dada de alta.");
        return;
    }
    let filas: Vec<Vec<String>> = estados
        .iter()
        .map(|estado| {
            vec![
                estado.cuenta.login.to_string(),
                si_no(estado.gitea_responde),
                estado
                    .version_gitea
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                estado.repos.total().to_string(),
                estado.repos.fallo.to_string(),
                estado.repos.huerfano.to_string(),
                estado
                    .ultima_sincronizacion
                    .as_ref()
                    .map(|s| s.fin.to_string())
                    .unwrap_or_else(|| "nunca".to_string()),
            ]
        })
        .collect();
    salida::tabla(
        &mut stdout,
        &[
            "login",
            "gitea",
            "versión",
            "repos",
            "fallos",
            "huérfanos",
            "última sync",
        ],
        &filas,
    );
}

fn si_no(valor: bool) -> String {
    if valor {
        "sí".to_string()
    } else {
        "no".to_string()
    }
}

fn fallo(mensaje: &str) -> u8 {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "error: {mensaje}");
    1
}

fn fallo_uso(mensaje: &str) -> u8 {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "error: {mensaje}");
    CODIGO_ERROR_DE_USO
}

fn emitir_error(error: &gitmereba_core::cuentas::ErrorCuentas) -> u8 {
    let mut stderr = std::io::stderr().lock();
    salida::error(&mut stderr, error);
    1
}
