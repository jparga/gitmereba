//! Validación de los datos de alta de una cuenta nueva.

use std::fs;
use std::path::Path;

use crate::config::cuenta::IndiceCuentas;
use crate::config::error::ErrorConfig;
use crate::config::rutas::Rutas;

/// Puerto mínimo permitido para el Gitea de una cuenta (por debajo son privilegiados).
const PUERTO_MINIMO: u16 = 1024;
/// Intervalo mínimo entre sincronizaciones, en minutos.
const INTERVALO_MINIMO_MINUTOS: u32 = 10;

/// Valida los datos de alta de una cuenta nueva contra las rutas de la app y las
/// cuentas ya existentes. No modifica nada: solo dice si el alta es válida.
pub fn validar_alta(
    rutas: &Rutas,
    indice: &IndiceCuentas,
    login: &str,
    carpeta: &Path,
    puerto: u16,
    intervalo_minutos: u32,
) -> Result<(), ErrorConfig> {
    validar_carpeta(rutas, indice, carpeta)?;

    if indice.cuentas.contains_key(login) {
        return Err(ErrorConfig::LoginDuplicado(login.to_string()));
    }
    if puerto < PUERTO_MINIMO {
        return Err(ErrorConfig::PuertoFueraDeRango);
    }
    if indice
        .cuentas
        .values()
        .any(|entrada| entrada.puerto == puerto)
    {
        return Err(ErrorConfig::PuertoDuplicado(puerto));
    }
    validar_intervalo(intervalo_minutos)?;
    Ok(())
}

/// Valida que `intervalo_minutos` no sea menor que el mínimo permitido entre
/// sincronizaciones. Compartida por el alta ([`validar_alta`]) y por guardar los
/// ajustes de una cuenta ya existente (`cuentas::actualizar_ajustes`).
pub fn validar_intervalo(intervalo_minutos: u32) -> Result<(), ErrorConfig> {
    if intervalo_minutos < INTERVALO_MINIMO_MINUTOS {
        Err(ErrorConfig::IntervaloDemasiadoCorto)
    } else {
        Ok(())
    }
}

/// Caracteres que romperían `app.ini` (`#` y `;` empiezan un comentario) o una unidad de
/// systemd (`%` y `$` se expanden, `"` y `\` son de citado), además de los de control.
const CARACTERES_RESERVADOS: [char; 6] = ['%', '$', '"', '\\', '#', ';'];

/// Primer carácter de `texto` que no puede formar parte de la carpeta de una cuenta
/// (control o reservado), si lo hay. Lo comparten el alta y `mover_carpeta`.
pub(crate) fn caracter_no_permitido_en_carpeta(texto: &str) -> Option<char> {
    texto
        .chars()
        .find(|c| c.is_control() || CARACTERES_RESERVADOS.contains(c))
}

/// Máximo de enlaces simbólicos colgantes que se siguen al resolver una ruta.
const MAX_SALTOS_ENLACE: u32 = 40;

/// Canonicaliza la parte existente de `ruta` (resolviendo enlaces simbólicos, también los
/// colgantes) y le añade el resto, que aún no existe, tal cual. La ruta no debe contener
/// `..`.
///
/// # Errors
///
/// `DemasiadosEnlaces` si se encadenan más de 40 enlaces simbólicos (p. ej. un bucle).
pub(crate) fn canonica_hasta_donde_existe(ruta: &Path) -> Result<std::path::PathBuf, ErrorConfig> {
    let mut saltos = 0;
    canonica_con_saltos(ruta, &mut saltos)
}

/// Como [`canonica_hasta_donde_existe`], con un contador de enlaces compartido entre las
/// resoluciones anidadas para que un bucle no recurra sin fin.
fn canonica_con_saltos(ruta: &Path, saltos: &mut u32) -> Result<std::path::PathBuf, ErrorConfig> {
    let mut resto = Vec::new();
    let mut actual = ruta.to_path_buf();
    loop {
        if let Ok(canonica) = actual.canonicalize() {
            let mut resultado = canonica;
            resultado.extend(resto.iter().rev());
            return Ok(resultado);
        }
        // Un enlace colgante no se puede canonicalizar: se sigue a mano para que no pase
        // por un componente inexistente.
        if fs::symlink_metadata(&actual).is_ok_and(|m| m.file_type().is_symlink())
            && let Ok(destino) = fs::read_link(&actual)
        {
            *saltos += 1;
            if *saltos > MAX_SALTOS_ENLACE {
                return Err(ErrorConfig::DemasiadosEnlaces);
            }
            // El destino relativo cuelga del padre REAL del enlace: si el padre contiene
            // enlaces, un `..` posterior sube por el directorio al que apuntan, no por el
            // nombre léxico. Por eso el padre se canonicaliza antes y cada `..` se resuelve
            // sobre lo ya resuelto.
            let mut acumulado = match actual.parent() {
                Some(padre) if destino.is_relative() => padre
                    .canonicalize()
                    .map_err(|error| ErrorConfig::Io(error.to_string()))?,
                _ => std::path::PathBuf::from("/"),
            };
            for componente in destino.components() {
                match componente {
                    std::path::Component::RootDir => acumulado = std::path::PathBuf::from("/"),
                    std::path::Component::ParentDir => {
                        acumulado = canonica_con_saltos(&acumulado, saltos)?;
                        acumulado.pop();
                    }
                    std::path::Component::CurDir => {}
                    otro => acumulado.push(otro.as_os_str()),
                }
            }
            actual = acumulado;
            continue;
        }
        match (actual.parent(), actual.file_name()) {
            (Some(padre), Some(nombre)) => {
                resto.push(nombre.to_os_string());
                actual = padre.to_path_buf();
            }
            _ => return Ok(ruta.to_path_buf()),
        }
    }
}

/// Reglas propias de la carpeta de destino: absoluta, no sensible, vacía y libre.
fn validar_carpeta(
    rutas: &Rutas,
    indice: &IndiceCuentas,
    carpeta: &Path,
) -> Result<(), ErrorConfig> {
    if !carpeta.is_absolute() {
        return Err(ErrorConfig::CarpetaNoAbsoluta);
    }
    if carpeta
        .components()
        .any(|componente| matches!(componente, std::path::Component::ParentDir))
    {
        return Err(ErrorConfig::CarpetaConPuntoPunto);
    }
    if let Some(caracter) = carpeta.to_str().and_then(caracter_no_permitido_en_carpeta) {
        return Err(ErrorConfig::CarpetaConCaracterNoPermitido(caracter));
    }
    if carpeta.to_str().is_none() {
        return Err(ErrorConfig::CarpetaNoUtf8);
    }
    if carpeta == Path::new("/") {
        return Err(ErrorConfig::CarpetaRaiz);
    }
    if carpeta == rutas.directorio_home() {
        return Err(ErrorConfig::CarpetaHome);
    }
    // Comprobación sobre rutas con los enlaces simbólicos ya resueltos: la léxica sola la
    // esquivaría un enlace a la carpeta de datos.
    if carpeta.starts_with(rutas.directorio_datos())
        || canonica_hasta_donde_existe(carpeta)?
            .starts_with(canonica_hasta_donde_existe(rutas.directorio_datos())?)
    {
        return Err(ErrorConfig::CarpetaDentroDeDatos);
    }
    if carpeta.exists() {
        let vacia = fs::read_dir(carpeta)
            .map_err(|error| ErrorConfig::Io(error.to_string()))?
            .next()
            .is_none();
        if !vacia {
            return Err(ErrorConfig::CarpetaNoVacia);
        }
    }
    if indice
        .cuentas
        .values()
        .any(|entrada| entrada.carpeta == carpeta)
    {
        return Err(ErrorConfig::CarpetaDuplicada(carpeta.to_path_buf()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::cuenta::EntradaCuenta;

    fn indice_con_una_cuenta() -> IndiceCuentas {
        let mut indice = IndiceCuentas::default();
        indice.cuentas.insert(
            "existente".to_string(),
            EntradaCuenta {
                carpeta: PathBuf::from("/cuentas/existente"),
                puerto: 3001,
            },
        );
        indice
    }

    #[test]
    fn alta_valida_es_ok() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = indice_con_una_cuenta();
        assert!(
            validar_alta(
                &rutas,
                &indice,
                "nueva",
                Path::new("/cuentas/nueva"),
                3002,
                30
            )
            .is_ok()
        );
    }

    #[test]
    fn rechaza_carpeta_relativa() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", Path::new("relativa"), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaNoAbsoluta)));
    }

    #[test]
    fn rechaza_la_raiz_del_sistema() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", Path::new("/"), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaRaiz)));
    }

    #[test]
    fn rechaza_el_directorio_home() {
        let rutas = Rutas::con_raiz("/raiz-de-prueba");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", rutas.directorio_home(), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaHome)));
    }

    #[test]
    fn rechaza_carpeta_dentro_del_directorio_de_datos() {
        let rutas = Rutas::con_raiz("/raiz-de-prueba");
        let indice = IndiceCuentas::default();
        let dentro = rutas.directorio_datos().join("otra-cosa");
        let resultado = validar_alta(&rutas, &indice, "nueva", &dentro, 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaDentroDeDatos)));
    }

    #[test]
    fn rechaza_caracteres_reservados_y_de_control_en_la_carpeta() {
        let rutas = Rutas::con_raiz("/raiz-de-prueba");
        let indice = IndiceCuentas::default();
        for malo in [
            "/c/a%b", "/c/a$b", "/c/a\"b", "/c/a\\b", "/c/a#b", "/c/a;b", "/c/a\nb",
        ] {
            let resultado = validar_alta(&rutas, &indice, "nueva", Path::new(malo), 3002, 30);
            assert!(
                matches!(
                    resultado,
                    Err(ErrorConfig::CarpetaConCaracterNoPermitido(_))
                ),
                "{malo:?}"
            );
        }
    }

    #[test]
    fn rechaza_componentes_punto_punto_en_la_carpeta() {
        let rutas = Rutas::con_raiz("/raiz-de-prueba");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", Path::new("/c/../x"), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaConPuntoPunto)));
    }

    #[test]
    fn un_enlace_simbolico_a_los_datos_no_esquiva_la_comprobacion() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path().join("raiz"));
        std::fs::create_dir_all(rutas.directorio_datos()).expect("crear datos");
        let enlace = temporal.path().join("enlace");
        std::os::unix::fs::symlink(rutas.directorio_datos(), &enlace).expect("enlace");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", &enlace.join("cuenta"), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaDentroDeDatos)));
    }

    #[test]
    fn un_enlace_colgante_hacia_los_datos_no_esquiva_la_comprobacion() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path().join("raiz"));
        std::fs::create_dir_all(rutas.directorio_datos()).expect("crear datos");
        let enlace = temporal.path().join("enlace");
        std::os::unix::fs::symlink(rutas.directorio_datos().join("nueva"), &enlace)
            .expect("enlace");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", &enlace.join("cuenta"), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaDentroDeDatos)));
    }

    #[test]
    fn un_enlace_colgante_relativo_hacia_los_datos_tambien_se_rechaza() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path().join("raiz"));
        std::fs::create_dir_all(rutas.directorio_datos()).expect("crear datos");
        let enlace = temporal.path().join("enlace");
        let destino = rutas.directorio_datos().join("nueva");
        let relativo = destino
            .strip_prefix(temporal.path())
            .expect("dentro del temporal");
        std::os::unix::fs::symlink(relativo, &enlace).expect("enlace");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", &enlace.join("cuenta"), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaDentroDeDatos)));
    }

    #[test]
    fn un_enlace_colgante_fuera_de_los_datos_se_acepta() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path().join("raiz"));
        std::fs::create_dir_all(rutas.directorio_datos()).expect("crear datos");
        let enlace = temporal.path().join("enlace");
        std::os::unix::fs::symlink(temporal.path().join("otro-sitio"), &enlace).expect("enlace");
        let indice = IndiceCuentas::default();
        assert!(validar_alta(&rutas, &indice, "nueva", &enlace.join("cuenta"), 3002, 30).is_ok());
    }

    #[test]
    fn un_enlace_colgante_relativo_tras_un_componente_enlazado_no_esquiva_el_filtro() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let base = temporal.path().canonicalize().expect("canónica");
        // datos = <base>/gitmereba/datos; foo/l -> ../gitmereba/datos/nueva (colgante) y
        // s -> foo: la carpeta s/l/cuenta cae dentro de datos, pero la ruta léxica
        // resultante (<base>/gitmereba/datos/nueva) solo coincide si se resuelve s antes.
        let rutas = Rutas::con_raiz(base.join("raiz"));
        let datos = rutas.directorio_datos().to_path_buf();
        std::fs::create_dir_all(&datos).expect("crear datos");
        let hondo = base.join("a").join("b");
        std::fs::create_dir_all(&hondo).expect("crear hondo");
        // `hondo/l` sube dos niveles: solo lleva a `base` si `hondo` se resuelve de verdad.
        let destino_rel = datos
            .strip_prefix(&base)
            .expect("dentro de la base")
            .join("nueva");
        std::os::unix::fs::symlink(Path::new("../..").join(&destino_rel), hondo.join("l"))
            .expect("enlace colgante");
        // `s` apunta a `hondo`; léxicamente s/l/.. resolvería distinto que en el kernel.
        let s = base.join("s");
        std::os::unix::fs::symlink(&hondo, &s).expect("enlace al padre");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(
            &rutas,
            &indice,
            "nueva",
            &s.join("l").join("cuenta"),
            3002,
            30,
        );
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaDentroDeDatos)));
    }

    #[test]
    fn un_bucle_de_enlaces_se_rechaza() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(temporal.path().join("raiz"));
        std::fs::create_dir_all(rutas.directorio_datos()).expect("crear datos");
        let a = temporal.path().join("a");
        let b = temporal.path().join("b");
        std::os::unix::fs::symlink(&b, &a).expect("enlace a");
        std::os::unix::fs::symlink(&a, &b).expect("enlace b");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", &a.join("cuenta"), 3002, 30);
        assert!(resultado.is_err());
    }

    #[test]
    fn rechaza_carpeta_no_vacia() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        std::fs::write(temporal.path().join("algo"), b"x").expect("escribir fichero de prueba");
        let rutas = Rutas::con_raiz("/raiz-de-prueba-sin-relacion");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(&rutas, &indice, "nueva", temporal.path(), 3002, 30);
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaNoVacia)));
    }

    #[test]
    fn acepta_carpeta_vacia_existente() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz("/raiz-de-prueba-sin-relacion");
        let indice = IndiceCuentas::default();
        assert!(validar_alta(&rutas, &indice, "nueva", temporal.path(), 3002, 30).is_ok());
    }

    #[test]
    fn rechaza_login_duplicado() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = indice_con_una_cuenta();
        let resultado = validar_alta(
            &rutas,
            &indice,
            "existente",
            Path::new("/cuentas/nueva"),
            3002,
            30,
        );
        assert!(matches!(resultado, Err(ErrorConfig::LoginDuplicado(_))));
    }

    #[test]
    fn rechaza_carpeta_duplicada() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = indice_con_una_cuenta();
        let resultado = validar_alta(
            &rutas,
            &indice,
            "nueva",
            Path::new("/cuentas/existente"),
            3002,
            30,
        );
        assert!(matches!(resultado, Err(ErrorConfig::CarpetaDuplicada(_))));
    }

    #[test]
    fn rechaza_puerto_por_debajo_del_minimo() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(
            &rutas,
            &indice,
            "nueva",
            Path::new("/cuentas/nueva"),
            1023,
            30,
        );
        assert!(matches!(resultado, Err(ErrorConfig::PuertoFueraDeRango)));
    }

    #[test]
    fn rechaza_puerto_duplicado() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = indice_con_una_cuenta();
        let resultado = validar_alta(
            &rutas,
            &indice,
            "nueva",
            Path::new("/cuentas/nueva"),
            3001,
            30,
        );
        assert!(matches!(resultado, Err(ErrorConfig::PuertoDuplicado(3001))));
    }

    #[test]
    fn validar_intervalo_rechaza_por_debajo_del_minimo() {
        assert!(matches!(
            validar_intervalo(9),
            Err(ErrorConfig::IntervaloDemasiadoCorto)
        ));
        assert!(validar_intervalo(10).is_ok());
    }

    #[test]
    fn rechaza_intervalo_demasiado_corto() {
        let rutas = Rutas::con_raiz("/raiz");
        let indice = IndiceCuentas::default();
        let resultado = validar_alta(
            &rutas,
            &indice,
            "nueva",
            Path::new("/cuentas/nueva"),
            3002,
            9,
        );
        assert!(matches!(
            resultado,
            Err(ErrorConfig::IntervaloDemasiadoCorto)
        ));
    }
}
