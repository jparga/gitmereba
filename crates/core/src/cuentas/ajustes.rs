//! Ajustes editables de una cuenta ya dada de alta: intervalo de sincronización y
//! alcance (pantalla Ajustes). El cambio de carpeta no se hace aquí: mover los
//! datos con Gitea en marcha necesita un diseño aparte.

use crate::almacen::Almacen;
use crate::config::{self, RutasCuenta, validar_intervalo};
use crate::modelo::{Alcance, Cuenta};

use super::error::ErrorCuentas;

/// Actualiza el intervalo de sincronización y el alcance de `cuenta`, y lo deja
/// auditado. No toca la carpeta ni ningún otro campo.
pub fn actualizar_ajustes(
    almacen: &Almacen,
    rutas_cuenta: &RutasCuenta,
    cuenta: &Cuenta,
    intervalo_minutos: u32,
    alcance: Alcance,
) -> Result<(), ErrorCuentas> {
    validar_intervalo(intervalo_minutos)?;

    let mut nueva = cuenta.clone();
    nueva.intervalo_minutos = intervalo_minutos;
    nueva.alcance = alcance;
    config::escribir_cuenta(rutas_cuenta, &nueva)?;

    almacen.auditar(
        Some(&cuenta.login),
        "cuenta.ajustes",
        &format!("intervalo_minutos={intervalo_minutos}"),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::Nombre;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn preparar(raiz: &std::path::Path) -> (Almacen, RutasCuenta, Cuenta) {
        let carpeta = raiz.join("cuenta");
        std::fs::create_dir_all(&carpeta).expect("crear carpeta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        let cuenta = Cuenta {
            login: nombre("jparga"),
            carpeta,
            puerto: 3900,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        };
        config::escribir_cuenta(&rutas_cuenta, &cuenta).expect("escribir gitmereba.toml");
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        (almacen, rutas_cuenta, cuenta)
    }

    #[test]
    fn actualiza_intervalo_y_alcance_y_audita() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, cuenta) = preparar(temporal.path());
        let alcance_nuevo = Alcance {
            incluir_forks: true,
            organizaciones: vec![nombre("mereba-oss")],
            excluidos: vec![],
        };

        actualizar_ajustes(&almacen, &rutas_cuenta, &cuenta, 60, alcance_nuevo.clone())
            .expect("actualizar no falla");

        let leida = config::leer_cuenta(&rutas_cuenta).expect("releer cuenta");
        assert_eq!(leida.intervalo_minutos, 60);
        assert_eq!(leida.alcance, alcance_nuevo);
        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "cuenta.ajustes"));
    }

    #[test]
    fn rechaza_un_intervalo_demasiado_corto_sin_tocar_el_fichero() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, cuenta) = preparar(temporal.path());

        let resultado =
            actualizar_ajustes(&almacen, &rutas_cuenta, &cuenta, 5, cuenta.alcance.clone());

        assert!(resultado.is_err());
        let leida = config::leer_cuenta(&rutas_cuenta).expect("releer cuenta");
        assert_eq!(leida.intervalo_minutos, 30);
    }
}
