//! `gitmereba cuenta add|list|rm`.

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};

use super::abrir_almacen;
use gitmereba_core::config::{self, Rutas};
use gitmereba_core::cuentas::{
    self, AprovisionadorReal, Contexto, ErrorCuentas, EstadoPaso, InformeLan, Lanzador,
    LanzadorPrimerPlano, LanzadorSystemd, PasoAlta, Previsualizacion, SolicitudAlta,
};
use gitmereba_core::gitea::ClienteGitea;
use gitmereba_core::github::ClienteGithub;
use gitmereba_core::idioma::{Idioma, Localizable, idioma_actual};
use gitmereba_core::modelo::{Alcance, Nombre, NombreHostInterno};
use gitmereba_core::secretos::{ClaveSecreto, Llavero, LlaveroDelSistema, Secreto};

use crate::cli::{ComandoCuenta, CuentaAddArgs, CuentaLanArgs, CuentaRmArgs, CuentaUsuarioArgs};
use crate::salida;
use crate::textos_cli::TextoCli;

use super::CODIGO_ERROR_DE_USO;

pub async fn ejecutar(accion: ComandoCuenta, rutas: &Rutas) -> u8 {
    match accion {
        ComandoCuenta::Add(args) => add(args, rutas).await,
        ComandoCuenta::List => list(rutas).await,
        ComandoCuenta::Rm(args) => rm(args, rutas).await,
        ComandoCuenta::Lan(args) => lan(args, rutas).await,
        ComandoCuenta::Usuario(args) => usuario(args, rutas).await,
    }
}

async fn add(args: CuentaAddArgs, rutas: &Rutas) -> u8 {
    let idioma = idioma_actual(rutas);
    let login = match Nombre::nuevo(args.login.as_str()) {
        Ok(login) => login,
        Err(error) => {
            return fallo_uso(&TextoCli::LoginInvalido(error.localizar(idioma)).texto(idioma));
        }
    };
    if !args.carpeta.is_absolute() {
        return fallo_uso(&TextoCli::CarpetaNoAbsoluta.texto(idioma));
    }
    let organizaciones: Result<Vec<Nombre>, _> = args
        .organizaciones
        .iter()
        .map(|org| Nombre::nuevo(org.as_str()))
        .collect();
    let organizaciones = match organizaciones {
        Ok(organizaciones) => organizaciones,
        Err(error) => {
            return fallo_uso(
                &TextoCli::OrganizacionInvalida(error.localizar(idioma)).texto(idioma),
            );
        }
    };

    let token = match leer_token(idioma) {
        Ok(token) => token,
        Err(mensaje) => return fallo(&mensaje),
    };

    let solicitud = SolicitudAlta {
        login,
        token: token.clone(),
        carpeta: args.carpeta.clone(),
        alcance: Alcance {
            incluir_forks: args.forks,
            organizaciones,
            excluidos: Vec::new(),
        },
        intervalo_minutos: args.intervalo,
        // La CLI nunca expone forma alguna de activar esto: solo tiene sentido en la
        // prueba de integración con un Gitea real.
        modo_pruebas: false,
    };

    let github = match ClienteGithub::nuevo(token) {
        Ok(github) => github,
        Err(error) => return emitir_error(idioma, &ErrorCuentas::from(error)),
    };

    let previsualizacion = match cuentas::previsualizar_alta(&github, &solicitud).await {
        Ok(previsualizacion) => previsualizacion,
        Err(error) => return emitir_error(idioma, &error),
    };
    mostrar_previsualizacion(&previsualizacion, idioma);

    if !args.si && !confirmar(&TextoCli::ConfirmarAlta.texto(idioma), idioma) {
        let mut stdout = std::io::stdout().lock();
        salida::linea(&mut stdout, &TextoCli::AltaCancelada.texto(idioma));
        return 0;
    }

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
    let aprovisionador = AprovisionadorReal;
    let mut progreso = |paso: PasoAlta, estado: EstadoPaso| imprimir_progreso(paso, estado, idioma);

    let resultado = if args.primer_plano {
        let lanzador = LanzadorPrimerPlano::nuevo();
        let resultado = cuentas::alta(
            &contexto,
            &solicitud,
            &github,
            ClienteGitea::nuevo,
            &lanzador,
            &aprovisionador,
            &mut progreso,
        )
        .await;
        if resultado.is_ok() {
            let mut stdout = std::io::stdout().lock();
            salida::linea(&mut stdout, &TextoCli::GiteaPrimerPlano.texto(idioma));
            drop(stdout);
            let _ = tokio::signal::ctrl_c().await;
            let _ = lanzador.parar(rutas, &solicitud.login).await;
        }
        resultado
    } else {
        let lanzador = LanzadorSystemd;
        cuentas::alta(
            &contexto,
            &solicitud,
            &github,
            ClienteGitea::nuevo,
            &lanzador,
            &aprovisionador,
            &mut progreso,
        )
        .await
    };

    match resultado {
        Ok(informe) => {
            let mut stdout = std::io::stdout().lock();
            salida::linea(&mut stdout, &informe.localizar(idioma));
            0
        }
        Err(error) => emitir_error(idioma, &error),
    }
}

async fn list(rutas: &Rutas) -> u8 {
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

    match cuentas::listar(&contexto) {
        Ok(cuentas) => {
            let mut stdout = std::io::stdout().lock();
            if cuentas.is_empty() {
                salida::linea(&mut stdout, &TextoCli::NoHayCuentas.texto(idioma));
                return 0;
            }
            let filas: Vec<Vec<String>> = cuentas
                .iter()
                .map(|cuenta| {
                    vec![
                        cuenta.login.to_string(),
                        cuenta.carpeta.display().to_string(),
                        cuenta.puerto.to_string(),
                        cuenta.intervalo_minutos.to_string(),
                    ]
                })
                .collect();
            salida::tabla(
                &mut stdout,
                &[
                    &TextoCli::ColLogin.texto(idioma),
                    &TextoCli::ColCarpeta.texto(idioma),
                    &TextoCli::ColPuerto.texto(idioma),
                    &TextoCli::ColIntervalo.texto(idioma),
                ],
                &filas,
            );
            0
        }
        Err(error) => emitir_error(idioma, &error),
    }
}

async fn rm(args: CuentaRmArgs, rutas: &Rutas) -> u8 {
    let idioma = idioma_actual(rutas);
    let login = match Nombre::nuevo(args.login.as_str()) {
        Ok(login) => login,
        Err(error) => {
            return fallo_uso(&TextoCli::LoginInvalido(error.localizar(idioma)).texto(idioma));
        }
    };

    if !args.si && !confirmar_login(&args.login, idioma) {
        let mut stdout = std::io::stdout().lock();
        salida::linea(&mut stdout, &TextoCli::BajaCancelada.texto(idioma));
        return 0;
    }

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
    let lanzador = LanzadorSystemd;

    match cuentas::baja(&contexto, &lanzador, &login, args.borrar_datos).await {
        Ok(()) => {
            let mut stdout = std::io::stdout().lock();
            salida::linea(
                &mut stdout,
                &TextoCli::CuentaBaja(login.to_string()).texto(idioma),
            );
            0
        }
        Err(error) => emitir_error(idioma, &error),
    }
}

async fn lan(args: CuentaLanArgs, rutas: &Rutas) -> u8 {
    let idioma = idioma_actual(rutas);
    let login = match Nombre::nuevo(args.login.as_str()) {
        Ok(login) => login,
        Err(error) => {
            return fallo_uso(&TextoCli::LoginInvalido(error.localizar(idioma)).texto(idioma));
        }
    };
    // clap ya exige que se dé exactamente una de `--activar`/`--desactivar`/`--estado`
    // (un `ArgGroup` obligatorio y exclusivo), pero no aplica de forma fiable el
    // `requires` de `--host` sobre `--activar` cuando ambos son miembros de ese mismo
    // grupo: se comprueba a mano aquí.
    if args.host.is_some() && !args.activar {
        return fallo_uso(&TextoCli::HostSoloConActivar.texto(idioma));
    }
    let host = match args.host.as_deref().map(NombreHostInterno::nuevo) {
        Some(Ok(host)) => Some(host),
        Some(Err(error)) => {
            return fallo_uso(&TextoCli::HostInvalido(error.localizar(idioma)).texto(idioma));
        }
        None => None,
    };

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

    if args.estado {
        return match cuentas::estado_lan(&contexto, &login) {
            Ok(informe) => {
                mostrar_informe_lan(&login, &informe, idioma);
                0
            }
            Err(error) => emitir_error(idioma, &error),
        };
    }

    let lanzador = LanzadorSystemd;
    if args.activar {
        return match cuentas::exponer_lan(&contexto, &lanzador, &login, host).await {
            Ok(informe) => {
                mostrar_informe_lan(&login, &informe, idioma);
                0
            }
            Err(error) => emitir_error(idioma, &error),
        };
    }

    debug_assert!(
        args.desactivar,
        "clap exige exactamente una de las tres acciones"
    );
    match cuentas::ocultar_lan(&contexto, &lanzador, &login).await {
        Ok(informe) => {
            mostrar_informe_lan(&login, &informe, idioma);
            0
        }
        Err(error) => emitir_error(idioma, &error),
    }
}

/// `cuenta usuario`: usuarios de Gitea para otras personas de la LAN.
async fn usuario(args: CuentaUsuarioArgs, rutas: &Rutas) -> u8 {
    let idioma = idioma_actual(rutas);
    let login = match Nombre::nuevo(args.login.as_str()) {
        Ok(login) => login,
        Err(error) => {
            return fallo_uso(&TextoCli::LoginInvalido(error.localizar(idioma)).texto(idioma));
        }
    };
    let nombre = match args.crear.as_deref().or(args.eliminar.as_deref()) {
        Some(texto) => match Nombre::nuevo(texto) {
            Ok(nombre) => Some(nombre),
            Err(error) => {
                return fallo_uso(
                    &TextoCli::NombreUsuarioInvalido(error.localizar(idioma)).texto(idioma),
                );
            }
        },
        None => None,
    };

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
    let mut stdout = std::io::stdout().lock();

    let Some(nombre) = nombre else {
        return match cuentas::listar_usuarios_lan(&contexto, &login).await {
            Ok(usuarios) if usuarios.is_empty() => {
                salida::linea(&mut stdout, &TextoCli::SinUsuariosLan.texto(idioma));
                0
            }
            Ok(usuarios) => {
                for usuario in usuarios {
                    salida::linea(&mut stdout, usuario.nombre.as_str());
                }
                0
            }
            Err(error) => emitir_error(idioma, &error),
        };
    };

    let cuenta = match config::leer_indice_cuentas(rutas) {
        Ok(indice) => match indice.cuentas.get(login.as_str()) {
            Some(entrada) => {
                match config::leer_cuenta(&config::RutasCuenta::nueva(entrada.carpeta.clone())) {
                    Ok(cuenta) => cuenta,
                    Err(error) => return emitir_error(idioma, &ErrorCuentas::from(error)),
                }
            }
            None => return emitir_error(idioma, &ErrorCuentas::CuentaNoExiste(login)),
        },
        Err(error) => return emitir_error(idioma, &ErrorCuentas::from(error)),
    };
    let token = match llavero.leer(&login, ClaveSecreto::TokenGitea) {
        Ok(Some(token)) => token,
        Ok(None) => return fallo(&TextoCli::SinTokenAdmin.texto(idioma)),
        Err(error) => {
            return fallo(&TextoCli::LlaveroIlegible(error.localizar(idioma)).texto(idioma));
        }
    };
    let gitea = match cuentas::cliente_gitea_de_cuenta(&cuenta, token) {
        Ok(gitea) => gitea,
        Err(error) => return emitir_error(idioma, &error),
    };

    if args.crear.is_some() {
        return match cuentas::crear_usuario_lan(&contexto, &gitea, &login, &nombre).await {
            Ok(creado) => {
                salida::linea(
                    &mut stdout,
                    &TextoCli::UsuarioCreado(creado.nombre.to_string()).texto(idioma),
                );
                // Única vez que se muestra: ni se guarda ni se puede volver a consultar.
                salida::linea(
                    &mut stdout,
                    &TextoCli::Contrasena(creado.password.exponer().to_string()).texto(idioma),
                );
                salida::linea(&mut stdout, &TextoCli::NoSeVolveraMostrar.texto(idioma));
                0
            }
            Err(error) => emitir_error(idioma, &error),
        };
    }

    match cuentas::eliminar_usuario_lan(&contexto, &gitea, &login, &nombre).await {
        Ok(()) => {
            salida::linea(
                &mut stdout,
                &TextoCli::UsuarioEliminado(nombre.to_string()).texto(idioma),
            );
            0
        }
        Err(error) => emitir_error(idioma, &error),
    }
}

/// Imprime el informe de acceso LAN en texto claro, incluida la línea de `/etc/hosts`
/// para los otros equipos de la LAN y para el propio anfitrión.
fn mostrar_informe_lan(login: &Nombre, informe: &InformeLan, idioma: Idioma) {
    let mut stdout = std::io::stdout().lock();
    match &informe.host {
        Some(host) => {
            salida::linea(
                &mut stdout,
                &TextoCli::AccesoLanActivo(login.to_string()).texto(idioma),
            );
            salida::linea(
                &mut stdout,
                &TextoCli::UrlPublica(informe.url_publica.to_string()).texto(idioma),
            );
            if let Some(huella) = &informe.huella_sha256 {
                salida::linea(
                    &mut stdout,
                    &TextoCli::HuellaCertificado(huella.to_string()).texto(idioma),
                );
            }
            if let Some(ruta) = &informe.ruta_certificado {
                salida::linea(
                    &mut stdout,
                    &TextoCli::Certificado(ruta.display().to_string()).texto(idioma),
                );
            }
            salida::linea(&mut stdout, "");
            salida::linea(&mut stdout, &TextoCli::AnadeAHosts.texto(idioma));
            salida::linea(
                &mut stdout,
                &TextoCli::LineaHostsLocal(host.to_string()).texto(idioma),
            );
            match &informe.linea_hosts {
                Some(linea) => salida::linea(
                    &mut stdout,
                    &TextoCli::LineaHostsLan(linea.to_string()).texto(idioma),
                ),
                None => salida::linea(&mut stdout, &TextoCli::SinIpLan.texto(idioma)),
            }
            if let Some(comando) = &informe.comando_git_cliente {
                salida::linea(&mut stdout, "");
                salida::linea(&mut stdout, &TextoCli::GitConfiaEnCertificado.texto(idioma));
                salida::linea(&mut stdout, &format!("  {comando}"));
            }
            if let Some(regla) = &informe.regla_cortafuegos_sugerida {
                salida::linea(&mut stdout, "");
                salida::linea(&mut stdout, &TextoCli::ReglaCortafuegos.texto(idioma));
                salida::linea(&mut stdout, &format!("  {regla}"));
            }
        }
        None => {
            salida::linea(
                &mut stdout,
                &TextoCli::AccesoLanInactivo(login.to_string()).texto(idioma),
            );
            salida::linea(
                &mut stdout,
                &TextoCli::UrlLocal(informe.url_publica.to_string()).texto(idioma),
            );
            if let Some(huella) = &informe.huella_sha256 {
                salida::linea(
                    &mut stdout,
                    &TextoCli::CertificadoConservado(huella.to_string()).texto(idioma),
                );
            }
        }
    }
}

/// Lee el token de lectura de GitHub: sin eco si stdin es un terminal (con
/// `rpassword`); si no lo es, una sola línea de stdin (para `cuenta add ... < fichero`).
fn leer_token(idioma: Idioma) -> Result<Secreto, String> {
    if std::io::stdin().is_terminal() {
        rpassword::prompt_password(TextoCli::PedirToken.texto(idioma))
            .map(Secreto::nuevo)
            .map_err(|error| error.to_string())
    } else {
        let mut linea = String::new();
        std::io::stdin()
            .read_line(&mut linea)
            .map_err(|error| error.to_string())?;
        let valor = linea.trim_end_matches(['\n', '\r']);
        if valor.is_empty() {
            Err(TextoCli::TokenNoRecibido.texto(idioma))
        } else {
            Ok(Secreto::nuevo(valor.to_string()))
        }
    }
}

fn confirmar(pregunta: &str, idioma: Idioma) -> bool {
    let mut stdout = std::io::stdout().lock();
    let _ = write!(
        stdout,
        "{pregunta} {}: ",
        TextoCli::SufijoSiNo.texto(idioma)
    );
    let _ = stdout.flush();
    drop(stdout);
    let mut linea = String::new();
    if std::io::stdin().read_line(&mut linea).is_err() {
        return false;
    }
    matches!(
        linea.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    )
}

fn confirmar_login(login: &str, idioma: Idioma) -> bool {
    let mut stdout = std::io::stdout().lock();
    let _ = write!(
        stdout,
        "{}",
        TextoCli::ConfirmarBaja(login.to_string()).texto(idioma)
    );
    let _ = stdout.flush();
    drop(stdout);
    let mut linea = String::new();
    if std::io::stdin().read_line(&mut linea).is_err() {
        return false;
    }
    linea.trim() == login
}

fn mostrar_previsualizacion(previsualizacion: &Previsualizacion, idioma: Idioma) {
    let mut stdout = std::io::stdout().lock();
    salida::linea(
        &mut stdout,
        &TextoCli::IdentidadConfirmada(previsualizacion.login.to_string()).texto(idioma),
    );
    match previsualizacion.caduca_token {
        Some(fecha) => salida::linea(
            &mut stdout,
            &TextoCli::TokenCaduca(fecha.to_string()).texto(idioma),
        ),
        None => salida::linea(&mut stdout, &TextoCli::TokenSinCaducidad.texto(idioma)),
    }
    if !previsualizacion.organizaciones_disponibles.is_empty() {
        let organizaciones: Vec<&str> = previsualizacion
            .organizaciones_disponibles
            .iter()
            .map(Nombre::as_str)
            .collect();
        salida::linea(
            &mut stdout,
            &TextoCli::OrganizacionesDisponibles(organizaciones.join(", ")).texto(idioma),
        );
    }

    let mut por_dueno: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for repo in &previsualizacion.repos_a_clonar {
        let entrada = por_dueno.entry(repo.id.dueno.to_string()).or_default();
        entrada.0 += 1;
        entrada.1 += repo.tamano_kb;
    }
    let filas: Vec<Vec<String>> = por_dueno
        .into_iter()
        .map(|(dueno, (repos, tamano_kb))| {
            vec![dueno, repos.to_string(), format!("{tamano_kb} KB")]
        })
        .collect();
    salida::tabla(
        &mut stdout,
        &[
            &TextoCli::ColDueno.texto(idioma),
            &TextoCli::ColRepos.texto(idioma),
            &TextoCli::ColTamano.texto(idioma),
        ],
        &filas,
    );
    salida::linea(
        &mut stdout,
        &TextoCli::TotalRepos(
            previsualizacion.repos_a_clonar.len().to_string(),
            previsualizacion.tamano_total_kb.to_string(),
        )
        .texto(idioma),
    );
}

fn imprimir_progreso(paso: PasoAlta, estado: EstadoPaso, idioma: Idioma) {
    let mut stdout = std::io::stdout().lock();
    let marca = match estado {
        EstadoPaso::Iniciando => "…",
        EstadoPaso::Hecho => "OK",
    };
    salida::linea(
        &mut stdout,
        &format!("[{marca}] {}", paso.localizar(idioma)),
    );
}

fn fallo(mensaje: &str) -> u8 {
    let mut stderr = std::io::stderr().lock();
    salida::linea(&mut stderr, &format!("error: {mensaje}"));
    1
}

fn fallo_uso(mensaje: &str) -> u8 {
    let mut stderr = std::io::stderr().lock();
    salida::linea(&mut stderr, &format!("error: {mensaje}"));
    CODIGO_ERROR_DE_USO
}

fn emitir_error(idioma: Idioma, error: &ErrorCuentas) -> u8 {
    let mut stderr = std::io::stderr().lock();
    salida::error(&mut stderr, error, idioma);
    1
}
