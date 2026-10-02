//! Definición de la CLI con `clap` derive. Sin lógica: solo el árbol de comandos.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// gitmereba: mantiene un clon local y operativo de cuentas de GitHub sobre Gitea.
#[derive(Debug, Parser)]
#[command(name = "gitmereba", version, about, disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub comando: Option<Comando>,

    /// Sustituye la raíz de datos de la app (`~/.local/share/gitmereba`). Solo para
    /// pruebas: el llavero de secretos sigue siendo el del sistema.
    #[arg(long, global = true, hide = true, value_name = "DIR")]
    pub datos: Option<PathBuf>,

    /// Más detalle en stderr (equivale a `GITMEREBA_LOG=info`).
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum Comando {
    /// Da de alta, lista o quita cuentas de GitHub clonadas.
    Cuenta {
        #[command(subcommand)]
        accion: ComandoCuenta,
    },
    /// Sincroniza una cuenta (o todas) con GitHub. Es lo que ejecuta el timer.
    Sync(SyncArgs),
    /// Muestra el estado de una cuenta (o de todas).
    Status(StatusArgs),
    /// Comprueba la salud del sistema y de cada cuenta.
    Doctor,
}

#[derive(Debug, Subcommand)]
pub enum ComandoCuenta {
    /// Da de alta una cuenta nueva.
    Add(CuentaAddArgs),
    /// Lista las cuentas dadas de alta.
    List,
    /// Da de baja una cuenta.
    Rm(CuentaRmArgs),
    /// Activa, desactiva o consulta el acceso a la cuenta desde la LAN por HTTPS.
    Lan(CuentaLanArgs),
    /// Crea, elimina o lista los usuarios de Gitea para otras personas de la LAN.
    Usuario(CuentaUsuarioArgs),
}

#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("accion-usuario")
        .required(true)
        .args(["crear", "eliminar", "listar"])
))]
pub struct CuentaUsuarioArgs {
    /// Login de la cuenta.
    #[arg(long)]
    pub login: String,

    /// Crea el usuario y muestra, una sola vez, la contraseña que genera Gitea.
    #[arg(long, value_name = "NOMBRE")]
    pub crear: Option<String>,

    /// Elimina el usuario.
    #[arg(long, value_name = "NOMBRE")]
    pub eliminar: Option<String>,

    /// Lista los usuarios de la LAN.
    #[arg(long)]
    pub listar: bool,
}

#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("accion-lan")
        .required(true)
        .args(["activar", "desactivar", "estado"])
))]
pub struct CuentaLanArgs {
    /// Login de la cuenta.
    #[arg(long)]
    pub login: String,

    /// Activa el acceso LAN (o cambia su host si ya estaba activo).
    #[arg(long)]
    pub activar: bool,

    /// Nombre `.internal` a usar con `--activar`; por defecto
    /// `<login>.gitmereba.internal`. Solo con `--activar`.
    //
    // Que solo valga con `--activar` se comprueba en `comandos::cuenta::lan`: el
    // `requires` de clap sobre un miembro de un `ArgGroup` obligatorio y exclusivo no se
    // aplica de forma fiable.
    #[arg(long, value_name = "HOST")]
    pub host: Option<String>,

    /// Desactiva el acceso LAN: Gitea vuelve a escuchar solo en 127.0.0.1 por HTTP.
    #[arg(long)]
    pub desactivar: bool,

    /// Muestra el estado actual sin cambiar nada.
    #[arg(long)]
    pub estado: bool,
}

#[derive(Debug, Args)]
pub struct CuentaAddArgs {
    /// Login de GitHub de la cuenta.
    #[arg(long)]
    pub login: String,

    /// Carpeta donde vivirá la cuenta (vacía o inexistente).
    #[arg(long)]
    pub carpeta: PathBuf,

    /// Incluye los forks de la cuenta.
    #[arg(long)]
    pub forks: bool,

    /// Organización a incluir (repetible).
    #[arg(long = "org", value_name = "ORG")]
    pub organizaciones: Vec<String>,

    /// Minutos entre sincronizaciones (mínimo 10).
    #[arg(long, default_value_t = 30)]
    pub intervalo: u32,

    /// No pide confirmación tras la previsualización.
    #[arg(long = "si")]
    pub si: bool,

    /// Arranca Gitea en primer plano (sin systemd) hasta Ctrl-C.
    #[arg(long = "primer-plano")]
    pub primer_plano: bool,
}

#[derive(Debug, Args)]
pub struct CuentaRmArgs {
    /// Login de la cuenta a dar de baja.
    pub login: String,

    /// Borra también la carpeta de datos de la cuenta.
    #[arg(long = "borrar-datos")]
    pub borrar_datos: bool,

    /// No pide confirmación escribiendo el login.
    #[arg(long = "si")]
    pub si: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Login de la cuenta a sincronizar. Si se omite, hace falta `--todas`.
    pub login: Option<String>,

    /// Sincroniza todas las cuentas.
    #[arg(long)]
    pub todas: bool,

    /// No aplica nada: solo muestra qué haría.
    #[arg(long)]
    pub simulacro: bool,

    /// Fuerza una sincronización inmediata de los mirrors tocados.
    #[arg(long)]
    pub forzar: bool,

    /// Antes de la pasada, fuerza este repositorio (`dueño/nombre`). Si su clonado inicial
    /// falló, descarta el mirror vacío para que la pasada lo vuelva a clonar.
    #[arg(long, value_name = "DUEÑO/NOMBRE", requires = "login")]
    pub repo: Option<String>,

    /// No envía notificaciones de escritorio (el estado de avisos se sigue guardando).
    #[arg(long = "sin-avisos")]
    pub sin_avisos: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Login de la cuenta. Si se omite, muestra todas.
    pub login: Option<String>,

    /// Salida en JSON en vez de tabla.
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn la_definicion_de_la_cli_es_valida() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parsea_cuenta_add_con_organizaciones_repetidas() {
        let cli = Cli::try_parse_from([
            "gitmereba",
            "cuenta",
            "add",
            "--login",
            "jparga",
            "--carpeta",
            "/tmp/x",
            "--org",
            "acme",
            "--org",
            "otra",
            "--forks",
        ])
        .expect("parseo válido");
        match cli.comando {
            Some(Comando::Cuenta {
                accion: ComandoCuenta::Add(args),
            }) => {
                assert_eq!(args.login, "jparga");
                assert_eq!(args.organizaciones, vec!["acme", "otra"]);
                assert!(args.forks);
                assert!(!args.si);
            }
            otro => panic!("se esperaba Cuenta::Add, llegó {otro:?}"),
        }
    }

    #[test]
    fn parsea_cuenta_lan_activar_con_host() {
        let cli = Cli::try_parse_from([
            "gitmereba",
            "cuenta",
            "lan",
            "--login",
            "jparga",
            "--activar",
            "--host",
            "jparga.internal",
        ])
        .expect("parseo válido");
        match cli.comando {
            Some(Comando::Cuenta {
                accion: ComandoCuenta::Lan(args),
            }) => {
                assert_eq!(args.login, "jparga");
                assert!(args.activar);
                assert_eq!(args.host.as_deref(), Some("jparga.internal"));
                assert!(!args.desactivar);
                assert!(!args.estado);
            }
            otro => panic!("se esperaba Cuenta::Lan, llegó {otro:?}"),
        }
    }

    #[test]
    fn cuenta_lan_exige_exactamente_una_accion() {
        // Ninguna de las tres acciones.
        assert!(Cli::try_parse_from(["gitmereba", "cuenta", "lan", "--login", "jparga"]).is_err());
        // Dos a la vez.
        assert!(
            Cli::try_parse_from([
                "gitmereba",
                "cuenta",
                "lan",
                "--login",
                "jparga",
                "--activar",
                "--desactivar",
            ])
            .is_err()
        );
    }

    #[test]
    fn cuenta_lan_host_se_admite_a_nivel_de_cli_sin_activar() {
        // El «requires» de clap no se aplica de forma fiable sobre un miembro de un
        // `ArgGroup` obligatorio y exclusivo (ver el comentario de `CuentaLanArgs::host`):
        // la validación real de que `--host` exige `--activar` vive en
        // `comandos::cuenta::lan`, no aquí.
        let cli = Cli::try_parse_from([
            "gitmereba",
            "cuenta",
            "lan",
            "--login",
            "jparga",
            "--desactivar",
            "--host",
            "x.internal",
        ])
        .expect("el parseo de clap lo admite; se valida en el manejador");
        match cli.comando {
            Some(Comando::Cuenta {
                accion: ComandoCuenta::Lan(args),
            }) => assert!(args.host.is_some()),
            otro => panic!("se esperaba Cuenta::Lan, llegó {otro:?}"),
        }
    }

    #[test]
    fn cuenta_usuario_exige_exactamente_una_accion() {
        let base = ["gitmereba", "cuenta", "usuario", "--login", "jparga"];
        assert!(Cli::try_parse_from(base).is_err());
        let mut dos = base.to_vec();
        dos.extend(["--crear", "ana", "--listar"]);
        assert!(Cli::try_parse_from(dos).is_err());
        let mut una = base.to_vec();
        una.extend(["--crear", "ana"]);
        match Cli::try_parse_from(una).expect("parsea").comando {
            Some(Comando::Cuenta {
                accion: ComandoCuenta::Usuario(args),
            }) => assert_eq!(args.crear.as_deref(), Some("ana")),
            otro => panic!("se esperaba Cuenta::Usuario, llegó {otro:?}"),
        }
    }

    #[test]
    fn parsea_cuenta_lan_estado() {
        let cli = Cli::try_parse_from([
            "gitmereba",
            "cuenta",
            "lan",
            "--login",
            "jparga",
            "--estado",
        ])
        .expect("parseo válido");
        match cli.comando {
            Some(Comando::Cuenta {
                accion: ComandoCuenta::Lan(args),
            }) => assert!(args.estado),
            otro => panic!("se esperaba Cuenta::Lan, llegó {otro:?}"),
        }
    }

    #[test]
    fn el_datos_global_funciona_antes_o_despues_del_subcomando() {
        let cli = Cli::try_parse_from(["gitmereba", "--datos", "/tmp/x", "status"])
            .expect("parseo válido");
        assert_eq!(cli.datos, Some(PathBuf::from("/tmp/x")));
    }
}
