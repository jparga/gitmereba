//! Despacho de subcomandos. Sin lógica de negocio: todo vive en `gitmereba_core`.

pub mod cuenta;
pub mod doctor;
pub mod status;
pub mod sync;

use std::io::Write;
use std::path::Path;

use clap::CommandFactory;
use gitmereba_core::almacen::{Almacen, ErrorAlmacen};
use gitmereba_core::config::Rutas;
use gitmereba_core::idioma::{Localizable, Preferencia, resolver};

use crate::cli::{Cli, Comando};
use crate::salida;

/// Código de salida de un uso incorrecto de la CLI (argumentos, no un fallo de negocio).
pub const CODIGO_ERROR_DE_USO: u8 = 1;
/// Código de salida de `sync` cuando hubo fallos parciales (no todo, pero algo falló).
pub const CODIGO_FALLOS_PARCIALES: u8 = 2;

/// Abre el almacén de `rutas`, asegurando antes que su directorio (`directorio_datos()`)
/// existe con permisos 0700.
///
/// `Almacen::abrir` (que no se puede tocar desde esta tarea) crea ese directorio con
/// `std::fs::create_dir_all` si hace falta, sin fijar su modo: queda con el umask del
/// proceso en vez de 0700 si es la primera vez que se toca (p. ej. un `doctor` o
/// `status` antes de cualquier alta). `config::escribir_indice_cuentas` sí lo crea con
/// 0700 (usa `config::fichero::crear_directorio_privado`), así que el problema solo se
/// nota cuando el almacén llega antes que el índice.
pub(crate) fn abrir_almacen(rutas: &Rutas) -> Result<Almacen, ErrorAlmacen> {
    asegurar_directorio_datos_0700(rutas);
    Almacen::abrir(&rutas.fichero_bd())
}

fn asegurar_directorio_datos_0700(rutas: &Rutas) {
    use std::os::unix::fs::DirBuilderExt;
    let directorio = rutas.directorio_datos();
    if directorio.exists() {
        return;
    }
    let _ = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directorio);
}

/// Ejecuta el comando pedido y devuelve el código de salida del proceso (0-255, como
/// exige `std::process::ExitCode`; el `main` hace la conversión final).
pub async fn ejecutar(cli: Cli) -> u8 {
    let Some(comando) = cli.comando else {
        return sin_subcomando();
    };

    let rutas = construir_rutas(cli.datos.as_deref());

    match comando {
        Comando::Cuenta { accion } => cuenta::ejecutar(accion, &rutas).await,
        Comando::Sync(args) => sync::ejecutar(args, &rutas).await,
        Comando::Status(args) => status::ejecutar(args, &rutas).await,
        Comando::Doctor => doctor::ejecutar(&rutas).await,
    }
}

pub(crate) fn construir_rutas(datos: Option<&Path>) -> Rutas {
    match datos {
        Some(raiz) => Rutas::con_raiz(raiz),
        None => match Rutas::del_sistema() {
            Ok(rutas) => rutas,
            Err(error) => {
                let mut stderr = std::io::stderr().lock();
                // Sin HOME no hay fichero de preferencias: el idioma sale solo del entorno.
                let idioma = resolver(Preferencia::Auto, &|nombre| std::env::var(nombre).ok());
                let _ = writeln!(stderr, "error: {}", error.localizar(idioma));
                // No hay HOME: no hay una raíz razonable. Se usa el directorio actual
                // para que los comandos sigan pudiendo ejecutarse (y fallar con un
                // mensaje claro) en vez de entrar en pánico aquí.
                Rutas::con_raiz(".")
            }
        },
    }
}

/// Sin subcomando: imprime la ayuda y el aviso de que la ventana llega en F3.
fn sin_subcomando() -> u8 {
    let mut stdout = std::io::stdout().lock();
    let ayuda = Cli::command().render_help();
    let _ = write!(stdout, "{ayuda}");
    salida::linea(&mut stdout, "");
    salida::linea(&mut stdout, "Sin subcomando, gitmereba abre su ventana.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[tokio::test]
    async fn sin_subcomando_sale_con_exito() {
        let cli = Cli::try_parse_from(["gitmereba"]).expect("parseo válido");
        assert_eq!(ejecutar(cli).await, 0);
    }
}
