//! Baja de una cuenta.

use std::path::{Path, PathBuf};

use crate::config::{self, RutasCuenta};
use crate::instancia;
use crate::modelo::Nombre;
use crate::secretos::Llavero;

use super::contexto::Contexto;
use super::error::ErrorCuentas;
use super::lanzador::Lanzador;

/// Da de baja `login`: para el servicio, borra los ficheros de las unidades del timer de
/// sincronización periódica (si existen), la quita del índice, borra sus secretos del
/// llavero y sus estados del almacén, y deja una entrada de auditoría.
///
/// La carpeta de datos solo se borra si `borrar_datos` es `true`, y en ese caso se
/// comprueba **antes** de tocar nada que la carpeta contiene un `gitmereba.toml` cuyo
/// login coincide con `login` (defensa contra borrar una carpeta equivocada) y que su
/// ruta canónica no es `/`, el `HOME` ni un ancestro del `HOME`. Si esa comprobación
/// falla, la baja entera se rechaza: no se quita nada del índice, del llavero ni del
/// almacén.
pub async fn baja<L: Llavero, Ln: Lanzador>(
    contexto: &Contexto<'_, L>,
    lanzador: &Ln,
    login: &Nombre,
    borrar_datos: bool,
) -> Result<(), ErrorCuentas> {
    let mut indice = config::leer_indice_cuentas(contexto.rutas)?;
    let entrada = indice
        .cuentas
        .get(login.as_str())
        .cloned()
        .ok_or_else(|| ErrorCuentas::CuentaNoExiste(login.clone()))?;

    if borrar_datos {
        verificar_carpeta_borrable(contexto.rutas, login, &entrada.carpeta)?;
    }

    lanzador.parar(contexto.rutas, login).await?;

    // Borrado puro de ficheros (sin `systemctl`, ver `instancia::temporizador`): seguro
    // de llamar aquí sin importar qué `Lanzador` se haya usado, y sin tocar systemd real
    // en las pruebas (que usan un `Rutas::con_raiz` temporal).
    instancia::borrar_unidades(contexto.rutas, login)?;

    indice.cuentas.remove(login.as_str());
    config::escribir_indice_cuentas(contexto.rutas, &indice)?;

    contexto.llavero.borrar_cuenta(login)?;
    contexto.almacen.borrar_estados_de(login)?;
    contexto.almacen.borrar_origenes_de(login)?;

    if borrar_datos {
        std::fs::remove_dir_all(&entrada.carpeta)
            .map_err(|error| ErrorCuentas::Io(error.to_string()))?;
    }

    contexto.almacen.auditar(
        Some(login),
        "cuenta.baja",
        &format!("borrar_datos={borrar_datos}"),
    )?;

    Ok(())
}

/// Comprueba que `carpeta` es de verdad la carpeta de datos de `login` y que borrarla es
/// seguro (no es la raíz, el HOME ni un ancestro del HOME).
fn verificar_carpeta_borrable(
    rutas: &crate::config::Rutas,
    login: &Nombre,
    carpeta: &Path,
) -> Result<(), ErrorCuentas> {
    let canonica = carpeta
        .canonicalize()
        .map_err(|error| ErrorCuentas::Io(error.to_string()))?;

    if es_ruta_protegida(rutas, &canonica) {
        return Err(ErrorCuentas::RutaProtegida(canonica));
    }

    let rutas_cuenta = RutasCuenta::nueva(&canonica);
    let cuenta = config::leer_cuenta(&rutas_cuenta)
        .map_err(|_| ErrorCuentas::CarpetaNoValidaParaBorrar(canonica.clone(), login.clone()))?;
    if cuenta.login != *login {
        return Err(ErrorCuentas::CarpetaNoValidaParaBorrar(
            canonica,
            login.clone(),
        ));
    }
    Ok(())
}

fn es_ruta_protegida(rutas: &crate::config::Rutas, canonica: &Path) -> bool {
    if canonica == Path::new("/") {
        return true;
    }
    let home: PathBuf = rutas
        .directorio_home()
        .canonicalize()
        .unwrap_or_else(|_| rutas.directorio_home().to_path_buf());
    // `canonica` es el HOME o un ancestro suyo si el HOME empieza por `canonica`.
    home.starts_with(canonica)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::config::Rutas;
    use crate::cuentas::dobles::LanzadorDoble;
    use crate::modelo::{Alcance, Cuenta};
    use crate::secretos::{ClaveSecreto, LlaveroEnMemoria, Secreto};

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn cuenta_de_prueba(login: &str, carpeta: &Path) -> Cuenta {
        Cuenta {
            login: nombre(login),
            carpeta: carpeta.to_path_buf(),
            puerto: 33055,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        }
    }

    /// Prepara un índice y una carpeta con `gitmereba.toml` válido para `login`.
    fn preparar_cuenta(raiz: &Path, login: &str) -> (Rutas, PathBuf) {
        let rutas = Rutas::con_raiz(raiz);
        let carpeta = raiz.join("datos-cuenta");
        std::fs::create_dir_all(&carpeta).expect("crear carpeta de la cuenta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        config::escribir_cuenta(&rutas_cuenta, &cuenta_de_prueba(login, &carpeta))
            .expect("escribir gitmereba.toml");

        let mut indice = config::leer_indice_cuentas(&rutas).expect("leer índice");
        indice.cuentas.insert(
            login.to_string(),
            config::EntradaCuenta {
                carpeta: carpeta.clone(),
                puerto: 33055,
            },
        );
        config::escribir_indice_cuentas(&rutas, &indice).expect("escribir índice");
        (rutas, carpeta)
    }

    #[tokio::test]
    async fn baja_sin_borrar_datos_conserva_la_carpeta() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, carpeta) = preparar_cuenta(temporal.path(), "jparga");
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let login = nombre("jparga");
        llavero
            .guardar(&login, ClaveSecreto::TokenGithub, &Secreto::nuevo("x"))
            .expect("guardar token");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();

        baja(&contexto, &lanzador, &login, false)
            .await
            .expect("baja no falla");

        assert!(carpeta.exists());
        assert!(
            config::leer_indice_cuentas(&rutas)
                .expect("leer índice")
                .cuentas
                .is_empty()
        );
        assert!(
            llavero
                .leer(&login, ClaveSecreto::TokenGithub)
                .expect("leer")
                .is_none()
        );
    }

    #[tokio::test]
    async fn baja_con_borrar_datos_borra_la_carpeta() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, carpeta) = preparar_cuenta(temporal.path(), "jparga");
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let login = nombre("jparga");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();

        baja(&contexto, &lanzador, &login, true)
            .await
            .expect("baja no falla");

        assert!(!carpeta.exists());
    }

    #[tokio::test]
    async fn baja_con_borrar_datos_se_niega_si_el_toml_no_coincide() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (rutas, carpeta) = preparar_cuenta(temporal.path(), "jparga");
        // Se sustituye el gitmereba.toml por el de otra cuenta.
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        config::escribir_cuenta(&rutas_cuenta, &cuenta_de_prueba("otra-cuenta", &carpeta))
            .expect("sobrescribir gitmereba.toml");

        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let login = nombre("jparga");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();

        let resultado = baja(&contexto, &lanzador, &login, true).await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::CarpetaNoValidaParaBorrar(..))
        ));
        // Nada se ha tocado: la carpeta sigue existiendo y la cuenta sigue en el índice.
        assert!(carpeta.exists());
        assert!(
            config::leer_indice_cuentas(&rutas)
                .expect("leer índice")
                .cuentas
                .contains_key("jparga")
        );
    }

    #[tokio::test]
    async fn baja_de_una_cuenta_inexistente_es_un_error() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();

        let resultado = baja(&contexto, &lanzador, &nombre("no-existe"), false).await;
        assert!(matches!(resultado, Err(ErrorCuentas::CuentaNoExiste(_))));
    }
}
