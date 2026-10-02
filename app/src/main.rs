//! Binario `gitmereba`: CLI (clap) sobre `gitmereba_core`. Sin lógica propia.

mod cli;
mod comandos;
mod salida;
mod ventana;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    inicializar_registro(cli.verbose);

    // Sin subcomando se abre la ventana; con subcomando, la CLI (lo que usa el timer).
    if cli.comando.is_none() {
        let rutas = comandos::construir_rutas(cli.datos.as_deref());
        return match ventana::abrir(rutas) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(%error, "no se pudo abrir la ventana");
                ExitCode::FAILURE
            }
        };
    }

    let ejecutor = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(ejecutor) => ejecutor,
        Err(error) => {
            tracing::error!(%error, "no se pudo iniciar el ejecutor asíncrono");
            return ExitCode::FAILURE;
        }
    };
    ExitCode::from(ejecutor.block_on(comandos::ejecutar(cli)))
}

/// `tracing-subscriber` a stderr; filtro por `GITMEREBA_LOG` (por defecto `warn`,
/// `-v`/`--verbose` sube a `info`).
fn inicializar_registro(verbose: bool) {
    // `zbus` avisa (WARN) de propiedades del portal del diálogo de carpetas que no existen
    // en todos los escritorios; no es accionable, así que solo se dejan pasar sus errores.
    let por_defecto = if verbose {
        "info,zbus=error"
    } else {
        "warn,zbus=error"
    };
    let filtro =
        EnvFilter::try_from_env("GITMEREBA_LOG").unwrap_or_else(|_| EnvFilter::new(por_defecto));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filtro)
        .with_writer(std::io::stderr)
        .try_init();
}
