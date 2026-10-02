//! Mantiene al día el temporizador de sincronización de una cuenta ya dada de alta.
//!
//! Las unidades `gitmereba-sync-<login>.{service,timer}` se escriben en el alta con la
//! ruta del ejecutable y el intervalo de ese momento. Las dos cosas cambian después: el
//! intervalo desde Ajustes, y la ruta al instalar la app como paquete (quien dio de alta
//! la cuenta desde `target/debug` tendría el timer apuntando ahí para siempre).

use std::path::Path;

use crate::config::Rutas;
use crate::instancia::{
    self, ParametrosTemporizador, generar_servicio_sync, generar_timer_sync, nombre_servicio_sync,
    nombre_timer_sync,
};
use crate::modelo::Cuenta;

use super::error::ErrorCuentas;

fn textos_esperados(rutas: &Rutas, cuenta: &Cuenta, ruta_ejecutable: &Path) -> (String, String) {
    let parametros = ParametrosTemporizador {
        login: &cuenta.login,
        ruta_ejecutable,
        carpeta_cuenta: &cuenta.carpeta,
        directorio_datos: rutas.directorio_datos(),
        intervalo_minutos: cuenta.intervalo_minutos,
    };
    (
        generar_servicio_sync(&parametros),
        generar_timer_sync(&parametros),
    )
}

/// `true` si las dos unidades de `cuenta` existen y son exactamente las que se generarían
/// hoy para `ruta_ejecutable` y el intervalo actual. Solo lee ficheros.
pub fn temporizador_al_dia(rutas: &Rutas, cuenta: &Cuenta, ruta_ejecutable: &Path) -> bool {
    let (servicio, timer) = textos_esperados(rutas, cuenta, ruta_ejecutable);
    let directorio = rutas.directorio_systemd_usuario();
    let coincide = |nombre: String, esperado: &str| {
        std::fs::read_to_string(directorio.join(nombre)).is_ok_and(|actual| actual == esperado)
    };
    coincide(nombre_servicio_sync(&cuenta.login), &servicio)
        && coincide(nombre_timer_sync(&cuenta.login), &timer)
}

/// Reescribe y reinicia el temporizador de `cuenta` si no está al día (ver
/// [`temporizador_al_dia`]). Devuelve `true` si ha cambiado algo; si ya estaba bien no
/// ejecuta `systemctl`.
pub async fn reparar_temporizador(
    rutas: &Rutas,
    cuenta: &Cuenta,
    ruta_ejecutable: &Path,
) -> Result<bool, ErrorCuentas> {
    if temporizador_al_dia(rutas, cuenta, ruta_ejecutable) {
        return Ok(false);
    }
    let (servicio, timer) = textos_esperados(rutas, cuenta, ruta_ejecutable);
    instancia::instalar_temporizador(rutas, &cuenta.login, &servicio, &timer).await?;
    // `enable --now` no relee el intervalo de un timer que ya está activo: se reinicia.
    instancia::reiniciar_temporizador(&cuenta.login).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{Alcance, Nombre};

    fn cuenta(carpeta: &Path, intervalo_minutos: u32) -> Cuenta {
        Cuenta {
            login: Nombre::nuevo("jparga").expect("nombre de prueba válido"),
            carpeta: carpeta.to_path_buf(),
            puerto: 3900,
            intervalo_minutos,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: Vec::new(),
                excluidos: Vec::new(),
            },
            lan: None,
        }
    }

    fn escribir_unidades(rutas: &Rutas, cuenta: &Cuenta, ejecutable: &Path) {
        let (servicio, timer) = textos_esperados(rutas, cuenta, ejecutable);
        let directorio = rutas.directorio_systemd_usuario();
        std::fs::create_dir_all(directorio).expect("crear directorio de unidades");
        std::fs::write(
            directorio.join(nombre_servicio_sync(&cuenta.login)),
            servicio,
        )
        .expect("escribir service");
        std::fs::write(directorio.join(nombre_timer_sync(&cuenta.login)), timer)
            .expect("escribir timer");
    }

    #[test]
    fn sin_unidades_no_esta_al_dia() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"), 30);

        assert!(!temporizador_al_dia(
            &rutas,
            &cuenta,
            Path::new("/usr/bin/gitmereba")
        ));
    }

    #[test]
    fn recien_escritas_estan_al_dia() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"), 30);
        let ejecutable = Path::new("/usr/bin/gitmereba");
        escribir_unidades(&rutas, &cuenta, ejecutable);

        assert!(temporizador_al_dia(&rutas, &cuenta, ejecutable));
    }

    #[test]
    fn otro_ejecutable_u_otro_intervalo_lo_dejan_obsoleto() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let original = cuenta(&raiz.path().join("cuenta"), 30);
        escribir_unidades(
            &rutas,
            &original,
            Path::new("/home/u/proyecto/target/debug/gitmereba"),
        );

        // La app se instaló como paquete: el ejecutable es otro.
        assert!(!temporizador_al_dia(
            &rutas,
            &original,
            Path::new("/usr/bin/gitmereba")
        ));
        // El intervalo se cambió en Ajustes.
        let con_otro_intervalo = cuenta(&raiz.path().join("cuenta"), 60);
        assert!(!temporizador_al_dia(
            &rutas,
            &con_otro_intervalo,
            Path::new("/home/u/proyecto/target/debug/gitmereba")
        ));
    }
}
