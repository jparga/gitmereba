//! `gitmereba status`: tabla legible o JSON del estado de una cuenta o de todas.

use std::io::Write;

use super::abrir_almacen;
use gitmereba_core::config::Rutas;
use gitmereba_core::cuentas::{self, Contexto, EstadoCuenta};
use gitmereba_core::idioma::{Idioma, Localizable, idioma_actual};
use gitmereba_core::modelo::Nombre;
use gitmereba_core::secretos::LlaveroDelSistema;

use crate::cli::StatusArgs;
use crate::salida;
use crate::textos_cli::TextoCli;

use super::CODIGO_ERROR_DE_USO;

pub async fn ejecutar(args: StatusArgs, rutas: &Rutas) -> u8 {
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

    let estados = match &args.login {
        Some(login) => {
            let login = match Nombre::nuevo(login.as_str()) {
                Ok(login) => login,
                Err(error) => {
                    return fallo_uso(
                        &TextoCli::LoginInvalido(error.localizar(idioma)).texto(idioma),
                    );
                }
            };
            match cuentas::estado(&contexto, &login).await {
                Ok(estado) => vec![estado],
                Err(error) => return emitir_error(idioma, &error),
            }
        }
        None => {
            let cuentas_dadas_de_alta = match cuentas::listar(&contexto) {
                Ok(cuentas) => cuentas,
                Err(error) => return emitir_error(idioma, &error),
            };
            let mut estados = Vec::with_capacity(cuentas_dadas_de_alta.len());
            for cuenta in cuentas_dadas_de_alta {
                match cuentas::estado(&contexto, &cuenta.login).await {
                    Ok(estado) => estados.push(estado),
                    Err(error) => return emitir_error(idioma, &error),
                }
            }
            estados
        }
    };

    if args.json {
        imprimir_json(&estados, idioma)
    } else {
        imprimir_tabla(&estados, idioma);
        0
    }
}

fn imprimir_json(estados: &[EstadoCuenta], idioma: Idioma) -> u8 {
    match serde_json::to_string_pretty(estados) {
        Ok(json) => {
            let mut stdout = std::io::stdout().lock();
            salida::linea(&mut stdout, &json);
            0
        }
        Err(error) => fallo(&TextoCli::NoSerializaEstado(error.to_string()).texto(idioma)),
    }
}

fn imprimir_tabla(estados: &[EstadoCuenta], idioma: Idioma) {
    let mut stdout = std::io::stdout().lock();
    if estados.is_empty() {
        salida::linea(&mut stdout, &TextoCli::NoHayCuentas.texto(idioma));
        return;
    }
    let filas: Vec<Vec<String>> = estados
        .iter()
        .map(|estado| {
            vec![
                estado.cuenta.login.to_string(),
                si_no(estado.gitea_responde, idioma),
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
                    .unwrap_or_else(|| TextoCli::Nunca.texto(idioma)),
            ]
        })
        .collect();
    salida::tabla(
        &mut stdout,
        &[
            &TextoCli::ColLogin.texto(idioma),
            &TextoCli::ColGitea.texto(idioma),
            &TextoCli::ColVersion.texto(idioma),
            &TextoCli::ColRepos.texto(idioma),
            &TextoCli::ColFallos.texto(idioma),
            &TextoCli::ColHuerfanos.texto(idioma),
            &TextoCli::ColUltimaSync.texto(idioma),
        ],
        &filas,
    );
}

fn si_no(valor: bool, idioma: Idioma) -> String {
    if valor {
        TextoCli::Si.texto(idioma)
    } else {
        TextoCli::No.texto(idioma)
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

fn emitir_error(idioma: Idioma, error: &gitmereba_core::cuentas::ErrorCuentas) -> u8 {
    let mut stderr = std::io::stderr().lock();
    salida::error(&mut stderr, error, idioma);
    1
}
