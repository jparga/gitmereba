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

/// Reglas propias de la carpeta de destino: absoluta, no sensible, vacía y libre.
fn validar_carpeta(
    rutas: &Rutas,
    indice: &IndiceCuentas,
    carpeta: &Path,
) -> Result<(), ErrorConfig> {
    if !carpeta.is_absolute() {
        return Err(ErrorConfig::CarpetaNoAbsoluta);
    }
    if carpeta == Path::new("/") {
        return Err(ErrorConfig::CarpetaRaiz);
    }
    if carpeta == rutas.directorio_home() {
        return Err(ErrorConfig::CarpetaHome);
    }
    if carpeta.starts_with(rutas.directorio_datos()) {
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
