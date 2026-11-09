//! Mantiene al día la unidad systemd de Gitea de una cuenta ya dada de alta.
//!
//! La unidad `gitmereba-gitea-<login>.service` se escribe en el alta con el
//! endurecimiento de esa versión. Las versiones posteriores añaden directivas y nada
//! reescribía las unidades ya instaladas: aquí se detecta la diferencia con el texto que
//! se generaría hoy y se corrige sin crear nunca una unidad que no existía.

use std::path::PathBuf;

use crate::config::{Rutas, RutasCuenta};
use crate::instancia::{self, ParametrosUnidad};
use crate::modelo::{Cuenta, Nombre};

use super::cerrojo::CerrojoCuenta;
use super::comun::{cargar_cuenta, ruta_binario_cacheado};
use super::error::ErrorCuentas;
use super::marca_mover::leer_marca;

/// Resultado de [`reparar_unidad_gitea`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReparacionUnidad {
    /// La unidad ya era la de hoy, o no está instalada (no se crea aquí).
    AlDia,
    /// Se reescribió la unidad y se aplicó.
    Reescrita,
    /// La cuenta estaba ocupada (sincronización, movimiento...) o falta el binario: se
    /// reintenta otra vez.
    Aplazada,
    /// La unidad instalada arranca otra versión de Gitea (o no se pudo leer cuál): no se
    /// reescribe nunca sola, porque Gitea migraría su base de datos sin que nadie lo decida.
    OtraVersion,
}

fn ruta_unidad(rutas: &Rutas, cuenta: &Cuenta) -> PathBuf {
    rutas
        .directorio_systemd_usuario()
        .join(instancia::nombre_unidad(&cuenta.login))
}

fn texto_esperado(rutas: &Rutas, cuenta: &Cuenta) -> String {
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let app_ini = rutas_cuenta.gitea_app_ini();
    let trabajo = rutas_cuenta.gitea();
    instancia::generar_unidad(&ParametrosUnidad {
        login: &cuenta.login,
        binario_gitea: &ruta_binario_cacheado(rutas),
        app_ini: &app_ini,
        directorio_trabajo: &trabajo,
        carpeta_cuenta: &cuenta.carpeta,
    })
}

/// `true` si la unidad de Gitea de `cuenta` existe y es exactamente la que se generaría
/// hoy. Solo lee ficheros.
pub fn unidad_gitea_al_dia(rutas: &Rutas, cuenta: &Cuenta) -> bool {
    std::fs::read_to_string(ruta_unidad(rutas, cuenta))
        .is_ok_and(|actual| actual == texto_esperado(rutas, cuenta))
}

/// Primer token del valor de `ExecStart=`, descomillado con la convención de
/// `escapar_argumento_exec` (comillas dobles con `\\` y `\"`, `%%` y `$$`). `None` si falta,
/// está mal formado o no es una ruta absoluta.
fn binario_de_la_unidad(texto: &str) -> Option<PathBuf> {
    let valor = texto
        .lines()
        .find_map(|linea| linea.strip_prefix("ExecStart="))?;
    let mut caracteres = valor.chars().peekable();
    let mut token = String::new();
    if caracteres.peek() == Some(&'"') {
        caracteres.next();
        let mut cerrado = false;
        while let Some(c) = caracteres.next() {
            match c {
                '"' => {
                    cerrado = true;
                    break;
                }
                '\\' => token.push(caracteres.next().filter(|s| matches!(s, '\\' | '"'))?),
                _ => token.push(c),
            }
        }
        if !cerrado || caracteres.next().is_some_and(|c| !c.is_whitespace()) {
            return None;
        }
    } else {
        while let Some(&c) = caracteres.peek() {
            if c.is_whitespace() {
                break;
            }
            if matches!(c, '"' | '\'' | '\\') {
                return None;
            }
            token.push(c);
            caracteres.next();
        }
    }
    let token = token.replace("%%", "%").replace("$$", "$");
    let ruta = PathBuf::from(token);
    ruta.is_absolute().then_some(ruta)
}

/// `true` si la unidad instalada arranca un binario distinto del de hoy (o no se puede
/// saber cuál): reescribirla cambiaría la versión de Gitea de la cuenta.
fn usa_otro_binario(rutas: &Rutas, cuenta: &Cuenta) -> bool {
    let instalado = std::fs::read_to_string(ruta_unidad(rutas, cuenta))
        .ok()
        .and_then(|texto| binario_de_la_unidad(&texto));
    instalado.as_deref() != Some(ruta_binario_cacheado(rutas).as_path())
}

/// `true` si el binario de Gitea al que apuntaría la unidad existe y es un fichero normal
/// (no un enlace). Sin él, reescribir la unidad dejaría la cuenta sin servicio.
fn binario_disponible(rutas: &Rutas) -> bool {
    let binario = ruta_binario_cacheado(rutas);
    let disponible = std::fs::symlink_metadata(&binario).is_ok_and(|m| m.file_type().is_file());
    if !disponible {
        tracing::warn!(
            binario = %binario.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "el binario de Gitea de esta versión no está descargado: no se reescribe la unidad"
        );
    }
    disponible
}

/// Reescribe la unidad de Gitea de la cuenta `login` si no está al día y reinicia el
/// servicio solo si estaba activo. Lee la cuenta del disco, también tras tomar el cerrojo,
/// para no actuar con datos de antes de un `mover` o una `baja`. Ver [`ReparacionUnidad`]
/// para los resultados.
pub async fn reparar_unidad_gitea(
    rutas: &Rutas,
    login: &Nombre,
) -> Result<ReparacionUnidad, ErrorCuentas> {
    reparar_con(rutas, login, aplicar_con_systemctl, || {}).await
}

/// Reescribe con `daemon-reload` y reinicia solo si estaba activa. Es la única parte que
/// toca systemd.
async fn aplicar_con_systemctl(
    rutas: Rutas,
    login: Nombre,
    texto: String,
) -> Result<(), ErrorCuentas> {
    instancia::instalar_unidad(&rutas, &login, &texto).await?;
    instancia::reiniciar_si_activa(&login).await?;
    Ok(())
}

/// Lee la cuenta; `None` si ya no existe.
fn cuenta_actual(rutas: &Rutas, login: &Nombre) -> Result<Option<Cuenta>, ErrorCuentas> {
    match cargar_cuenta(rutas, login) {
        Ok(cuenta) => Ok(Some(cuenta)),
        Err(ErrorCuentas::CuentaNoExiste(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// `entre_pasos` se ejecuta con el cerrojo ya tomado y antes de releer la cuenta: en las
/// pruebas simula lo que otra operación pudo cambiar entre la primera lectura y el cerrojo.
async fn reparar_con<F, Fut, G>(
    rutas: &Rutas,
    login: &Nombre,
    aplicar: F,
    entre_pasos: G,
) -> Result<ReparacionUnidad, ErrorCuentas>
where
    F: FnOnce(Rutas, Nombre, String) -> Fut,
    Fut: Future<Output = Result<(), ErrorCuentas>>,
    G: FnOnce(),
{
    let Some(cuenta) = cuenta_actual(rutas, login)? else {
        return Ok(ReparacionUnidad::AlDia);
    };
    if !ruta_unidad(rutas, &cuenta).exists() || unidad_gitea_al_dia(rutas, &cuenta) {
        return Ok(ReparacionUnidad::AlDia);
    }
    if usa_otro_binario(rutas, &cuenta) {
        return Ok(ReparacionUnidad::OtraVersion);
    }
    if !binario_disponible(rutas) {
        return Ok(ReparacionUnidad::Aplazada);
    }
    let _cerrojo = match CerrojoCuenta::tomar(rutas, login) {
        Ok(cerrojo) => cerrojo,
        Err(ErrorCuentas::CuentaOcupada(_)) => return Ok(ReparacionUnidad::Aplazada),
        Err(error) => return Err(error),
    };
    // Un movimiento interrumpido (o ilegible) deja la carpeta en un estado que decide la
    // persona: no se toca la unidad hasta entonces.
    if !matches!(leer_marca(rutas, login), Ok(None)) {
        return Ok(ReparacionUnidad::Aplazada);
    }
    entre_pasos();
    // Con el cerrojo, la cuenta ya no puede cambiar: se relee y se decide con lo de ahora.
    let Some(cuenta) = cuenta_actual(rutas, login)? else {
        return Ok(ReparacionUnidad::AlDia);
    };
    if !ruta_unidad(rutas, &cuenta).exists() || unidad_gitea_al_dia(rutas, &cuenta) {
        return Ok(ReparacionUnidad::AlDia);
    }
    if usa_otro_binario(rutas, &cuenta) {
        return Ok(ReparacionUnidad::OtraVersion);
    }
    if !binario_disponible(rutas) {
        return Ok(ReparacionUnidad::Aplazada);
    }
    let texto = texto_esperado(rutas, &cuenta);
    aplicar(rutas.clone(), cuenta.login.clone(), texto).await?;
    Ok(ReparacionUnidad::Reescrita)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, EntradaCuenta};
    use crate::cuentas::marca_mover::{MarcaMover, escribir_marca};
    use crate::modelo::Alcance;
    use std::path::Path;

    /// Unidad de una versión sin endurecer, pero con el binario de hoy.
    fn unidad_antigua(rutas: &Rutas) -> String {
        format!(
            "[Service]\nExecStart={} web\nNoNewPrivileges=yes\n",
            ruta_binario_cacheado(rutas).display()
        )
    }

    fn cuenta(carpeta: &Path) -> Cuenta {
        Cuenta {
            login: Nombre::nuevo("jparga").expect("nombre de prueba válido"),
            carpeta: carpeta.to_path_buf(),
            puerto: 3900,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: Vec::new(),
                excluidos: Vec::new(),
            },
            lan: None,
        }
    }

    /// Da de alta la cuenta en el índice y en su carpeta, como lo deja el alta.
    fn registrar(rutas: &Rutas, cuenta: &Cuenta) {
        std::fs::create_dir_all(&cuenta.carpeta).expect("crear carpeta");
        config::escribir_cuenta(&RutasCuenta::nueva(&cuenta.carpeta), cuenta)
            .expect("escribir cuenta");
        let mut indice = config::leer_indice_cuentas(rutas).expect("índice");
        indice.cuentas.insert(
            cuenta.login.to_string(),
            EntradaCuenta {
                carpeta: cuenta.carpeta.clone(),
                puerto: cuenta.puerto,
            },
        );
        config::escribir_indice_cuentas(rutas, &indice).expect("escribir índice");
    }

    fn poner_binario(rutas: &Rutas) {
        let ruta = ruta_binario_cacheado(rutas);
        std::fs::create_dir_all(ruta.parent().expect("padre")).expect("crear bin");
        std::fs::write(ruta, b"#!/bin/sh\n").expect("escribir binario");
    }

    fn escribir(rutas: &Rutas, cuenta: &Cuenta, texto: &str) {
        let ruta = ruta_unidad(rutas, cuenta);
        std::fs::create_dir_all(ruta.parent().expect("padre")).expect("crear directorio");
        std::fs::write(ruta, texto).expect("escribir unidad");
    }

    fn leer(rutas: &Rutas, cuenta: &Cuenta) -> String {
        std::fs::read_to_string(ruta_unidad(rutas, cuenta)).expect("leer unidad")
    }

    /// Aplicación de prueba: escribe sin systemd y cuenta las llamadas.
    async fn aplicar_falso(
        rutas: Rutas,
        login: Nombre,
        texto: String,
        llamadas: &std::cell::Cell<u32>,
    ) -> Result<(), ErrorCuentas> {
        llamadas.set(llamadas.get() + 1);
        instancia::escribir_unidad(&rutas, &login, &texto)?;
        Ok(())
    }

    #[tokio::test]
    async fn sin_fichero_no_esta_al_dia_y_reparar_no_crea_nada() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        let llamadas = std::cell::Cell::new(0);

        assert!(!unidad_gitea_al_dia(&rutas, &cuenta));
        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
        assert!(!ruta_unidad(&rutas, &cuenta).exists());
    }

    #[tokio::test]
    async fn con_el_texto_exacto_esta_al_dia_y_no_se_aplica() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        escribir(&rutas, &cuenta, &texto_esperado(&rutas, &cuenta));
        let llamadas = std::cell::Cell::new(0);

        assert!(unidad_gitea_al_dia(&rutas, &cuenta));
        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
    }

    #[tokio::test]
    async fn una_unidad_antigua_se_reescribe() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        let llamadas = std::cell::Cell::new(0);

        assert!(!unidad_gitea_al_dia(&rutas, &cuenta));
        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Reescrita);
        assert_eq!(llamadas.get(), 1);
        assert!(leer(&rutas, &cuenta).contains("KeyringMode=private"));
        assert!(unidad_gitea_al_dia(&rutas, &cuenta));
    }

    /// Unidad con el endurecimiento de hoy pero con `ExecStart` apuntando a `ejecutable`.
    fn unidad_con_ejecutable(rutas: &Rutas, cuenta: &Cuenta, ejecutable: &str) -> String {
        let esperado = texto_esperado(rutas, cuenta);
        let binario = ruta_binario_cacheado(rutas);
        esperado.replace(binario.to_str().expect("utf8"), ejecutable)
    }

    async fn reparar_y_contar(rutas: &Rutas, cuenta: &Cuenta) -> (ReparacionUnidad, u32) {
        let llamadas = std::cell::Cell::new(0);
        let r = reparar_con(
            rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        (r.expect("reparar"), llamadas.get())
    }

    #[test]
    fn el_execstart_se_descomilla_como_lo_escapa_la_unidad() {
        let con_espacio = "[Service]\nExecStart=\"/a b/gitea\\\\x\" web\n";
        assert_eq!(
            binario_de_la_unidad(con_espacio),
            Some(PathBuf::from("/a b/gitea\\x"))
        );
        assert_eq!(
            binario_de_la_unidad("ExecStart=/a/gitea%% web\n"),
            Some(PathBuf::from("/a/gitea%"))
        );
    }

    #[tokio::test]
    async fn una_unidad_con_otra_version_del_binario_no_se_reescribe() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        let antigua = unidad_con_ejecutable(&rutas, &cuenta, "/x/bin/gitea-0.0.1");
        assert!(antigua.contains("gitea-0.0.1"));
        escribir(&rutas, &cuenta, &antigua);

        let (r, llamadas) = reparar_y_contar(&rutas, &cuenta).await;
        assert_eq!(r, ReparacionUnidad::OtraVersion);
        assert_eq!(llamadas, 0);
        assert_eq!(leer(&rutas, &cuenta), antigua);
    }

    #[tokio::test]
    async fn la_misma_version_sin_endurecimiento_se_reescribe() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        let binario = ruta_binario_cacheado(&rutas);
        let antigua = format!(
            "[Service]\nExecStart={} web --config /x/app.ini\nNoNewPrivileges=yes\n",
            binario.display()
        );
        escribir(&rutas, &cuenta, &antigua);

        let (r, llamadas) = reparar_y_contar(&rutas, &cuenta).await;
        assert_eq!(r, ReparacionUnidad::Reescrita);
        assert_eq!(llamadas, 1);
        assert!(unidad_gitea_al_dia(&rutas, &cuenta));
    }

    #[tokio::test]
    async fn un_execstart_ilegible_cuenta_como_otra_version() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        for ilegible in [
            "[Service]\nNoNewPrivileges=yes\n",
            "[Service]\nExecStart=\n",
            "[Service]\nExecStart=\"sin cerrar web\n",
            "[Service]\nExecStart=relativo/gitea web\n",
        ] {
            escribir(&rutas, &cuenta, ilegible);
            let (r, llamadas) = reparar_y_contar(&rutas, &cuenta).await;
            assert_eq!(r, ReparacionUnidad::OtraVersion, "{ilegible:?}");
            assert_eq!(llamadas, 0);
            assert_eq!(leer(&rutas, &cuenta), ilegible);
        }
    }

    #[tokio::test]
    async fn con_la_cuenta_ocupada_se_aplaza_sin_tocar_el_fichero() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        let _otro = CerrojoCuenta::tomar(&rutas, &cuenta.login).expect("cerrojo");
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Aplazada);
        assert_eq!(llamadas.get(), 0);
        assert_eq!(leer(&rutas, &cuenta), unidad_antigua(&rutas));
    }

    #[tokio::test]
    async fn con_un_movimiento_en_curso_se_aplaza() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        escribir_marca(
            &rutas,
            &MarcaMover {
                login: cuenta.login.to_string(),
                origen: raiz.path().join("a"),
                destino: raiz.path().join("b"),
                modo: "pendiente".to_string(),
            },
        )
        .expect("marca");
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Aplazada);
        assert_eq!(llamadas.get(), 0);
        assert_eq!(leer(&rutas, &cuenta), unidad_antigua(&rutas));
    }

    #[tokio::test]
    async fn sin_el_binario_de_gitea_se_aplaza_sin_tocar_la_unidad() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Aplazada);
        assert_eq!(llamadas.get(), 0);
        assert_eq!(leer(&rutas, &cuenta), unidad_antigua(&rutas));
    }

    #[tokio::test]
    async fn un_enlace_en_lugar_del_binario_tampoco_vale() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        let ruta = ruta_binario_cacheado(&rutas);
        std::fs::create_dir_all(ruta.parent().expect("padre")).expect("crear bin");
        std::fs::write(raiz.path().join("real"), b"x").expect("real");
        std::os::unix::fs::symlink(raiz.path().join("real"), &ruta).expect("enlace");
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Aplazada);
        assert_eq!(llamadas.get(), 0);
    }

    #[tokio::test]
    async fn una_cuenta_que_ya_no_existe_esta_al_dia() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {},
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
    }

    #[tokio::test]
    async fn una_unidad_borrada_tras_la_comprobacion_inicial_no_se_recrea() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {
                std::fs::remove_file(ruta_unidad(&rutas, &cuenta)).expect("borrar unidad");
            },
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
        assert!(!ruta_unidad(&rutas, &cuenta).exists());
    }

    #[tokio::test]
    async fn una_cuenta_dada_de_baja_tras_la_comprobacion_inicial_no_se_recrea() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        registrar(&rutas, &cuenta);
        poner_binario(&rutas);
        escribir(&rutas, &cuenta, &unidad_antigua(&rutas));
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &cuenta.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || {
                config::escribir_indice_cuentas(&rutas, &config::IndiceCuentas::default())
                    .expect("vaciar índice");
            },
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
        assert_eq!(leer(&rutas, &cuenta), unidad_antigua(&rutas));
    }

    #[tokio::test]
    async fn una_carpeta_movida_tras_la_comprobacion_inicial_se_escribe_con_la_nueva() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let antigua = cuenta(&raiz.path().join("antigua"));
        registrar(&rutas, &antigua);
        poner_binario(&rutas);
        escribir(&rutas, &antigua, &unidad_antigua(&rutas));
        let nueva = cuenta(&raiz.path().join("nueva"));
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(
            &rutas,
            &antigua.login,
            |r, l, t| aplicar_falso(r, l, t, &llamadas),
            || registrar(&rutas, &nueva),
        )
        .await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Reescrita);
        assert_eq!(llamadas.get(), 1);
        let escrito = leer(&rutas, &nueva);
        assert_eq!(escrito, texto_esperado(&rutas, &nueva));
        assert!(!escrito.contains(antigua.carpeta.to_str().expect("utf8")));
    }
}
