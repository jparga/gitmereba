//! Ayuda de la CLI en el idioma elegido.
//!
//! El español vive en los comentarios de documentación de `cli.rs` (los usa clap). Aquí
//! está el catálogo inglés, indexado por ruta (`cuenta.add.login`), y la función que lo
//! aplica al árbol de comandos. Los textos internos de clap («Usage:») no se tocan.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::Command;
use gitmereba_core::idioma::Idioma;

/// Ayuda en inglés por ruta. `""` es el comando raíz; `cuenta.add` un subcomando;
/// `cuenta.add.login` un argumento (por su identificador). El sufijo `.long` es la ayuda
/// larga, si el comentario de documentación tiene varios párrafos.
const CATALOGO_EN: &[(&str, &str)] = &[
    (
        "",
        "Local, working clone of GitHub accounts on top of Gitea",
    ),
    (
        "datos",
        "Overrides the app data root (`~/.local/share/gitmereba`). For tests only: the secrets keyring is still the system one",
    ),
    (
        "verbose",
        "More detail on stderr (same as `GITMEREBA_LOG=info`)",
    ),
    ("cuenta", "Adds, lists or removes cloned GitHub accounts"),
    ("cuenta.add", "Adds a new account"),
    ("cuenta.add.login", "GitHub login of the account"),
    (
        "cuenta.add.carpeta",
        "Folder where the account will live (empty or nonexistent)",
    ),
    ("cuenta.add.forks", "Includes the account's forks"),
    (
        "cuenta.add.organizaciones",
        "Organization to include (repeatable)",
    ),
    ("cuenta.add.intervalo", "Minutes between syncs (minimum 10)"),
    (
        "cuenta.add.si",
        "Does not ask for confirmation after the preview",
    ),
    (
        "cuenta.add.primer_plano",
        "Runs Gitea in the foreground (no systemd) until Ctrl-C",
    ),
    ("cuenta.list", "Lists the accounts that have been added"),
    ("cuenta.rm", "Removes an account"),
    ("cuenta.rm.login", "Login of the account to remove"),
    (
        "cuenta.rm.borrar_datos",
        "Also deletes the account's data folder",
    ),
    (
        "cuenta.rm.si",
        "Does not ask for confirmation by typing the login",
    ),
    (
        "cuenta.lan",
        "Enables, disables or shows access to the account from the LAN over HTTPS",
    ),
    ("cuenta.lan.login", "Login of the account"),
    (
        "cuenta.lan.activar",
        "Enables LAN access (or changes its host if already enabled)",
    ),
    (
        "cuenta.lan.host",
        "`.internal` name to use with `--activar`; defaults to `<login>.gitmereba.internal`. Only with `--activar`",
    ),
    (
        "cuenta.lan.desactivar",
        "Disables LAN access: Gitea goes back to listening only on 127.0.0.1 over HTTP",
    ),
    (
        "cuenta.lan.estado",
        "Shows the current state without changing anything",
    ),
    (
        "cuenta.usuario",
        "Creates, deletes or lists Gitea users for other people on the LAN",
    ),
    ("cuenta.usuario.login", "Login of the account"),
    (
        "cuenta.usuario.crear",
        "Creates the user and shows, only once, the password Gitea generates",
    ),
    ("cuenta.usuario.eliminar", "Deletes the user"),
    ("cuenta.usuario.listar", "Lists the LAN users"),
    (
        "sync",
        "Syncs an account (or all of them) with GitHub. This is what the timer runs",
    ),
    (
        "sync.login",
        "Login of the account to sync. If omitted, `--todas` is required",
    ),
    ("sync.todas", "Syncs all accounts"),
    (
        "sync.simulacro",
        "Applies nothing: only shows what it would do",
    ),
    (
        "sync.forzar",
        "Forces an immediate sync of the mirrors it touches",
    ),
    (
        "sync.repo",
        "Before the pass, forces this repository (`owner/name`). If its initial clone failed, discards the empty mirror so the pass clones it again",
    ),
    (
        "sync.sin_avisos",
        "Does not send desktop notifications (the notification state is still saved)",
    ),
    (
        "status",
        "Shows the state of an account (or of all of them)",
    ),
    (
        "status.login",
        "Login of the account. If omitted, shows all of them",
    ),
    ("status.json", "Output as JSON instead of a table"),
    (
        "doctor",
        "Checks the health of the system and of each account",
    ),
];

/// Nombre del valor (`--repo <DUEÑO/NOMBRE>`) en inglés, por ruta de argumento. Los
/// nombres iguales en los dos idiomas (`DIR`, `ORG`...) no necesitan entrada.
const VALORES_EN: &[(&str, &str)] = &[
    ("cuenta.add.carpeta", "FOLDER"),
    ("cuenta.add.intervalo", "INTERVAL"),
    ("cuenta.usuario.crear", "NAME"),
    ("cuenta.usuario.eliminar", "NAME"),
    ("sync.repo", "OWNER/NAME"),
];

fn valor_en(ruta: &str) -> Option<&'static str> {
    VALORES_EN
        .iter()
        .find(|(clave, _)| *clave == ruta)
        .map(|(_, texto)| *texto)
}

fn ingles(ruta: &str) -> Option<&'static str> {
    CATALOGO_EN
        .iter()
        .find(|(clave, _)| *clave == ruta)
        .map(|(_, texto)| *texto)
}

fn unir(ruta: &str, nombre: &str) -> String {
    if ruta.is_empty() {
        nombre.to_string()
    } else {
        format!("{ruta}.{nombre}")
    }
}

/// Devuelve `cmd` con la ayuda en `idioma`. En español no cambia nada: el texto ya es
/// el de los comentarios de documentación.
pub fn localizar(cmd: Command, idioma: Idioma) -> Command {
    match idioma {
        Idioma::Es => cmd,
        Idioma::En => localizar_en(cmd, ""),
    }
}

fn localizar_en(mut cmd: Command, ruta: &str) -> Command {
    if let Some(texto) = ingles(ruta) {
        cmd = cmd.about(texto);
    }
    if let Some(texto) = ingles(&format!("{ruta}.long")) {
        cmd = cmd.long_about(texto);
    }

    let argumentos: Vec<String> = cmd
        .get_arguments()
        .map(|arg| arg.get_id().to_string())
        .collect();
    for id in argumentos {
        let clave = unir(ruta, &id);
        if let Some(texto) = ingles(&clave) {
            cmd = cmd.mut_arg(id.as_str(), |arg| arg.help(texto));
        }
        if let Some(texto) = ingles(&format!("{clave}.long")) {
            cmd = cmd.mut_arg(id.as_str(), |arg| arg.long_help(texto));
        }
        if let Some(nombre) = valor_en(&clave) {
            cmd = cmd.mut_arg(id.as_str(), |arg| arg.value_name(nombre));
        }
    }

    let subcomandos: Vec<String> = cmd
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();
    for nombre in subcomandos {
        let ruta_hija = unir(ruta, &nombre);
        cmd = cmd.mut_subcommand(nombre.as_str(), |sub| localizar_en(sub, &ruta_hija));
    }
    cmd
}

/// Busca `--datos <dir>` / `--datos=<dir>` a mano. El idioma hace falta antes de que clap
/// parsee (`--help` sale antes de entregar nada) y depende de la raíz de datos.
pub fn datos_de_argumentos(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    let mut args = args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            return None;
        }
        if arg == "--datos" {
            return args.next().map(PathBuf::from);
        }
        if let Some(valor) = arg.to_str().and_then(|a| a.strip_prefix("--datos=")) {
            return Some(PathBuf::from(valor));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::CommandFactory;
    use std::collections::BTreeSet;

    /// Rutas con texto de ayuda en español: (ruta, es_ayuda_larga).
    fn rutas_con_ayuda(cmd: &Command, ruta: &str, salida: &mut BTreeSet<String>) {
        if cmd.get_about().is_some() {
            salida.insert(ruta.to_string());
        }
        if cmd.get_long_about().is_some() {
            salida.insert(format!("{ruta}.long"));
        }
        for arg in cmd.get_arguments() {
            let clave = unir(ruta, arg.get_id().as_str());
            if arg.get_help().is_some() {
                salida.insert(clave.clone());
            }
            if arg.get_long_help().is_some() {
                salida.insert(format!("{clave}.long"));
            }
        }
        for sub in cmd.get_subcommands() {
            rutas_con_ayuda(sub, &unir(ruta, sub.get_name()), salida);
        }
    }

    #[test]
    fn cada_texto_de_ayuda_tiene_traduccion_inglesa() {
        let mut esperadas = BTreeSet::new();
        rutas_con_ayuda(&Cli::command(), "", &mut esperadas);
        let faltan: Vec<&String> = esperadas.iter().filter(|r| ingles(r).is_none()).collect();
        assert!(faltan.is_empty(), "sin traducción inglesa: {faltan:?}");
    }

    #[test]
    fn el_catalogo_no_apunta_a_rutas_inexistentes() {
        let mut existentes = BTreeSet::new();
        rutas_con_ayuda(&Cli::command(), "", &mut existentes);
        let sobran: Vec<&str> = CATALOGO_EN
            .iter()
            .map(|(clave, _)| *clave)
            .filter(|clave| !existentes.contains(*clave))
            .collect();
        assert!(sobran.is_empty(), "entradas sin destino: {sobran:?}");
    }

    /// Nombre del valor tal como lo pinta clap: `value_name` o, si no hay, el id en mayúsculas.
    fn nombre_de_valor(arg: &clap::Arg) -> String {
        match arg.get_value_names() {
            Some(nombres) => nombres
                .iter()
                .map(|n| n.as_str().to_string())
                .collect::<Vec<_>>()
                .join(" "),
            None => arg.get_id().as_str().to_uppercase(),
        }
    }

    fn argumentos_con_valor(cmd: &Command, ruta: &str, salida: &mut Vec<(String, String)>) {
        for arg in cmd.get_arguments() {
            if arg.get_action().takes_values() {
                salida.push((unir(ruta, arg.get_id().as_str()), nombre_de_valor(arg)));
            }
        }
        for sub in cmd.get_subcommands() {
            argumentos_con_valor(sub, &unir(ruta, sub.get_name()), salida);
        }
    }

    /// Nombres de valor iguales en español y en inglés.
    const VALORES_NEUTROS: &[&str] = &["DIR", "ORG", "HOST", "LOGIN"];

    #[test]
    fn cada_nombre_de_valor_esta_traducido_o_es_neutro() {
        let mut valores = Vec::new();
        argumentos_con_valor(&Cli::command(), "", &mut valores);
        let sin_decidir: Vec<&(String, String)> = valores
            .iter()
            .filter(|(ruta, nombre)| {
                valor_en(ruta).is_none() && !VALORES_NEUTROS.contains(&nombre.as_str())
            })
            .collect();
        assert!(
            sin_decidir.is_empty(),
            "nombres de valor sin decidir: {sin_decidir:?}"
        );
    }

    #[test]
    fn el_catalogo_de_valores_no_apunta_a_argumentos_inexistentes() {
        let mut valores = Vec::new();
        argumentos_con_valor(&Cli::command(), "", &mut valores);
        let sobran: Vec<&str> = VALORES_EN
            .iter()
            .map(|(ruta, _)| *ruta)
            .filter(|ruta| !valores.iter().any(|(r, _)| r == ruta))
            .collect();
        assert!(sobran.is_empty(), "entradas sin destino: {sobran:?}");
    }

    #[test]
    fn el_nombre_de_valor_ingles_sale_en_la_ayuda_y_en_el_uso() {
        let mut cmd = localizar(Cli::command(), Idioma::En);
        let sync = cmd.find_subcommand_mut("sync").expect("sync existe");
        let ayuda = sync.render_help().to_string();
        assert!(ayuda.contains("--repo <OWNER/NAME>"), "{ayuda}");
        assert!(!ayuda.contains("DUEÑO"), "{ayuda}");
        let mut es = localizar(Cli::command(), Idioma::Es);
        let ayuda_es = es
            .find_subcommand_mut("sync")
            .expect("sync existe")
            .render_help()
            .to_string();
        assert!(ayuda_es.contains("--repo <DUEÑO/NOMBRE>"), "{ayuda_es}");
        let add = cmd
            .find_subcommand_mut("cuenta")
            .and_then(|c| c.find_subcommand_mut("add"))
            .expect("cuenta add existe")
            .render_help()
            .to_string();
        assert!(add.contains("--carpeta <FOLDER>") && add.contains("--intervalo <INTERVAL>"));
    }

    #[test]
    fn el_catalogo_no_repite_rutas() {
        let claves: BTreeSet<&str> = CATALOGO_EN.iter().map(|(c, _)| *c).collect();
        assert_eq!(claves.len(), CATALOGO_EN.len());
    }

    #[test]
    fn la_ayuda_inglesa_se_aplica_y_la_espanola_no_cambia() {
        let es = localizar(Cli::command(), Idioma::Es)
            .render_help()
            .to_string();
        let en = localizar(Cli::command(), Idioma::En)
            .render_help()
            .to_string();
        assert!(es.contains("Sincroniza una cuenta"));
        assert!(en.contains("Syncs an account"));
        assert!(!en.contains("Sincroniza"));
        let mut original = Cli::command();
        assert_eq!(es, original.render_help().to_string());
    }

    #[test]
    fn la_ayuda_de_un_argumento_anidado_sale_en_ingles() {
        let mut cmd = localizar(Cli::command(), Idioma::En);
        let add = cmd
            .find_subcommand_mut("cuenta")
            .and_then(|c| c.find_subcommand_mut("add"))
            .expect("cuenta add existe");
        let ayuda = add.render_help().to_string();
        assert!(ayuda.contains("GitHub login of the account"));
    }

    #[test]
    fn localizar_deja_la_cli_valida() {
        localizar(Cli::command(), Idioma::En).debug_assert();
    }

    fn os(partes: &[&str]) -> Vec<OsString> {
        partes.iter().map(OsString::from).collect()
    }

    #[test]
    fn datos_se_encuentra_separado_o_con_igual() {
        assert_eq!(
            datos_de_argumentos(os(&["g", "--datos", "/tmp/a", "doctor"])),
            Some(PathBuf::from("/tmp/a"))
        );
        assert_eq!(
            datos_de_argumentos(os(&["g", "doctor", "--datos=/tmp/b"])),
            Some(PathBuf::from("/tmp/b"))
        );
    }

    #[test]
    fn datos_ausente_o_tras_doble_guion_es_none() {
        assert_eq!(datos_de_argumentos(os(&["g", "doctor"])), None);
        assert_eq!(datos_de_argumentos(os(&["g", "--", "--datos", "/x"])), None);
        assert_eq!(datos_de_argumentos(os(&["g", "--datos"])), None);
    }

    #[test]
    fn datos_ignora_el_nombre_del_programa() {
        assert_eq!(datos_de_argumentos(os(&["--datos=/x"])), None);
    }
}
