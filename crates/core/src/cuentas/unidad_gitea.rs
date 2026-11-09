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
use super::comun::ruta_binario_cacheado;
use super::error::ErrorCuentas;
use super::marca_mover::leer_marca;

/// Resultado de [`reparar_unidad_gitea`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReparacionUnidad {
    /// La unidad ya era la de hoy, o no está instalada (no se crea aquí).
    AlDia,
    /// Se reescribió la unidad y se aplicó.
    Reescrita,
    /// La cuenta estaba ocupada (sincronización, movimiento...): se reintenta otra vez.
    Aplazada,
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

/// Reescribe la unidad de Gitea de `cuenta` si no está al día y reinicia el servicio solo
/// si estaba activo. Ver [`ReparacionUnidad`] para los resultados.
pub async fn reparar_unidad_gitea(
    rutas: &Rutas,
    cuenta: &Cuenta,
) -> Result<ReparacionUnidad, ErrorCuentas> {
    reparar_con(rutas, cuenta, aplicar_con_systemctl).await
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

async fn reparar_con<F, Fut>(
    rutas: &Rutas,
    cuenta: &Cuenta,
    aplicar: F,
) -> Result<ReparacionUnidad, ErrorCuentas>
where
    F: FnOnce(Rutas, Nombre, String) -> Fut,
    Fut: Future<Output = Result<(), ErrorCuentas>>,
{
    let ruta = ruta_unidad(rutas, cuenta);
    if !ruta.exists() || unidad_gitea_al_dia(rutas, cuenta) {
        return Ok(ReparacionUnidad::AlDia);
    }
    let _cerrojo = match CerrojoCuenta::tomar(rutas, &cuenta.login) {
        Ok(cerrojo) => cerrojo,
        Err(ErrorCuentas::CuentaOcupada(_)) => return Ok(ReparacionUnidad::Aplazada),
        Err(error) => return Err(error),
    };
    // Un movimiento interrumpido (o ilegible) deja la carpeta en un estado que decide la
    // persona: no se toca la unidad hasta entonces.
    if !matches!(leer_marca(rutas, &cuenta.login), Ok(None)) {
        return Ok(ReparacionUnidad::Aplazada);
    }
    let texto = texto_esperado(rutas, cuenta);
    aplicar(rutas.clone(), cuenta.login.clone(), texto).await?;
    Ok(ReparacionUnidad::Reescrita)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuentas::marca_mover::{MarcaMover, escribir_marca};
    use crate::modelo::Alcance;
    use std::path::Path;

    const UNIDAD_ANTIGUA: &str = "[Service]\nExecStart=/x web\nNoNewPrivileges=yes\n";

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
        let llamadas = std::cell::Cell::new(0);

        assert!(!unidad_gitea_al_dia(&rutas, &cuenta));
        let r = reparar_con(&rutas, &cuenta, |r, l, t| aplicar_falso(r, l, t, &llamadas)).await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
        assert!(!ruta_unidad(&rutas, &cuenta).exists());
    }

    #[tokio::test]
    async fn con_el_texto_exacto_esta_al_dia_y_no_se_aplica() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        escribir(&rutas, &cuenta, &texto_esperado(&rutas, &cuenta));
        let llamadas = std::cell::Cell::new(0);

        assert!(unidad_gitea_al_dia(&rutas, &cuenta));
        let r = reparar_con(&rutas, &cuenta, |r, l, t| aplicar_falso(r, l, t, &llamadas)).await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::AlDia);
        assert_eq!(llamadas.get(), 0);
    }

    #[tokio::test]
    async fn una_unidad_antigua_se_reescribe() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        escribir(&rutas, &cuenta, UNIDAD_ANTIGUA);
        let llamadas = std::cell::Cell::new(0);

        assert!(!unidad_gitea_al_dia(&rutas, &cuenta));
        let r = reparar_con(&rutas, &cuenta, |r, l, t| aplicar_falso(r, l, t, &llamadas)).await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Reescrita);
        assert_eq!(llamadas.get(), 1);
        assert!(leer(&rutas, &cuenta).contains("KeyringMode=private"));
        assert!(unidad_gitea_al_dia(&rutas, &cuenta));
    }

    #[tokio::test]
    async fn con_la_cuenta_ocupada_se_aplaza_sin_tocar_el_fichero() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        escribir(&rutas, &cuenta, UNIDAD_ANTIGUA);
        let _otro = CerrojoCuenta::tomar(&rutas, &cuenta.login).expect("cerrojo");
        let llamadas = std::cell::Cell::new(0);

        let r = reparar_con(&rutas, &cuenta, |r, l, t| aplicar_falso(r, l, t, &llamadas)).await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Aplazada);
        assert_eq!(llamadas.get(), 0);
        assert_eq!(leer(&rutas, &cuenta), UNIDAD_ANTIGUA);
    }

    #[tokio::test]
    async fn con_un_movimiento_en_curso_se_aplaza() {
        let raiz = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(raiz.path());
        let cuenta = cuenta(&raiz.path().join("cuenta"));
        escribir(&rutas, &cuenta, UNIDAD_ANTIGUA);
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

        let r = reparar_con(&rutas, &cuenta, |r, l, t| aplicar_falso(r, l, t, &llamadas)).await;
        assert_eq!(r.expect("reparar"), ReparacionUnidad::Aplazada);
        assert_eq!(llamadas.get(), 0);
        assert_eq!(leer(&rutas, &cuenta), UNIDAD_ANTIGUA);
    }
}
