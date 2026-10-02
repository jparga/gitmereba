//! [`LlaveroDelSistema`]: implementación real de [`Llavero`] sobre el Secret Service
//! de freedesktop (GNOME Keyring, KWallet) usando el crate `keyring`.
//!
//! ## Qué backend se usa y por qué es determinista
//!
//! `crates/core/Cargo.toml` fija `keyring = { version = "4.2.0", default-features =
//! false, features = ["v1", "zbus-secret-service-keyring-store"] }`. Con la API `v1`
//! de `keyring` (`keyring-4.2.0/src/v1.rs`, función `set_credential_store`, líneas
//! 109-129), la primera vez que se crea un [`Entry`] la biblioteca elige un backend
//! por plataforma: en cualquier Unix que no sea macOS/iOS/Android construye un
//! `zbus_secret_service_keyring_store::Store` y lo fija como tienda por defecto. Como
//! `default-features = false` y solo activamos el feature `v1` (que a su vez activa,
//! para Unix, únicamente el feature implícito `zbus-secret-service-keyring-store`,
//! ver `keyring-4.2.0/Cargo.toml` sección `[features]`), ninguna otra tienda
//! (`dbus-secret-service-keyring-store`, `linux-keyutils-keyring-store`,
//! `db-keystore`) se compila: la tienda de Secret Service vía `zbus` es la única
//! posible en este binario. La selección es, por tanto, determinista aunque no la
//! invoquemos nosotros mismos línea por línea.
//!
//! La API `v1` (la única que tenemos habilitada) no expone una función para elegir
//! el backend explícitamente desde la aplicación: esa función
//! (`keyring::cli::use_zbus_secret_service_store`, ver `keyring-4.2.0/src/cli.rs`
//! líneas 26-78) solo existe bajo el feature `cli`, y ese feature arrastra, además de
//! la propia tienda, soporte opcional para Android, Apple Keychain/Protected,
//! `db-keystore`, `dbus-secret-service-keyring-store` y `linux-keyutils-keyring-store`
//! (`keyring-4.2.0/Cargo.toml`, `[features] cli = [...]`), muchas más dependencias de
//! las mínimas que exige el proyecto. Por eso no se activa `cli`. En su lugar,
//! [`LlaveroDelSistema::nuevo`] fuerza y comprueba esa inicialización única de forma
//! explícita y temprana con [`Entry::store_status`], en vez de descubrir el fallo en
//! el primer `guardar`/`leer`.
//!
//! ## Hilos y `tokio`
//!
//! La API bloqueante de `secret-service` espera sobre el ejecutor con el que se haya
//! compilado `zbus`. En este workspace la unificación de features lo decide el binario:
//! al entrar Tauri (`ashpd`), `zbus` pasa a compilarse con `tokio`, y entonces su
//! `block_on` entra en pánico («Cannot start a runtime from within a runtime») si se le
//! llama desde un hilo que ya está dentro de un ejecutor de `tokio`, que es justo desde
//! donde lo llaman la CLI y la ventana. Por eso **toda** llamada a `keyring` se hace en
//! un hilo del sistema recién creado ([`fuera_del_ejecutor`]), que no pertenece a ningún
//! ejecutor. El hilo que llama queda bloqueado mientras dura el diálogo por D-Bus.

use keyring::Entry;

use crate::modelo::Nombre;
use crate::secretos::Secreto;

use super::llavero::{ClaveSecreto, ErrorLlavero, Llavero};

/// Nombre de servicio con el que se identifican todos los secretos de gitmereba
/// en el llavero del sistema.
const SERVICIO: &str = "gitmereba";

/// Implementación de [`Llavero`] sobre el Secret Service del sistema (D-Bus,
/// GNOME Keyring / KWallet), vía el crate `keyring`.
///
/// Es síncrona en su firma y bloquea el hilo que la llama mientras dialoga por D-Bus
/// con el servicio de secretos. Se puede invocar desde dentro de un ejecutor de `tokio`
/// (ver el módulo, sección «Hilos y `tokio`»).
pub struct LlaveroDelSistema {
    _privado: (),
}

impl LlaveroDelSistema {
    /// Comprueba que hay un Secret Service accesible y deja el llavero listo para usar.
    ///
    /// # Errores
    /// [`ErrorLlavero::NoDisponible`] si no hay bus de sesión D-Bus, no hay demonio
    /// de Secret Service, o la plataforma no está soportada por el backend de
    /// `keyring` con las features activas en este workspace.
    pub fn nuevo() -> Result<Self, ErrorLlavero> {
        fuera_del_ejecutor(|| {
            Entry::store_status()
                .as_ref()
                .map_err(error_keyring)
                .copied()
        })?;
        Ok(Self { _privado: () })
    }

    fn entrada(cuenta: &Nombre, clave: ClaveSecreto) -> Result<Entry, ErrorLlavero> {
        Entry::new(SERVICIO, &usuario(cuenta, clave)).map_err(|err| error_keyring(&err))
    }
}

/// Atributo de usuario con el que se guarda un secreto: `"<login>/<clave>"`.
fn usuario(cuenta: &Nombre, clave: ClaveSecreto) -> String {
    format!("{}/{}", cuenta.as_str(), clave.etiqueta())
}

impl Llavero for LlaveroDelSistema {
    fn guardar(
        &self,
        cuenta: &Nombre,
        clave: ClaveSecreto,
        valor: &Secreto,
    ) -> Result<(), ErrorLlavero> {
        fuera_del_ejecutor(|| {
            let entrada = Self::entrada(cuenta, clave)?;
            entrada
                .set_password(valor.exponer())
                .map_err(|err| error_keyring(&err))
        })
    }

    fn leer(&self, cuenta: &Nombre, clave: ClaveSecreto) -> Result<Option<Secreto>, ErrorLlavero> {
        fuera_del_ejecutor(|| {
            let entrada = Self::entrada(cuenta, clave)?;
            match entrada.get_password() {
                Ok(valor) => Ok(Some(Secreto::nuevo(valor))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(error_keyring(&err)),
            }
        })
    }

    fn borrar(&self, cuenta: &Nombre, clave: ClaveSecreto) -> Result<(), ErrorLlavero> {
        fuera_del_ejecutor(|| {
            let entrada = Self::entrada(cuenta, clave)?;
            match entrada.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(err) => Err(error_keyring(&err)),
            }
        })
    }
}

/// Ejecuta `operacion` en un hilo del sistema recién creado y espera a que termine.
///
/// Ese hilo no pertenece a ningún ejecutor de `tokio`, así que el `block_on` interno de
/// `zbus` no entra en pánico aunque quien llame sí esté dentro de uno. Un pánico de
/// `operacion` se convierte en [`ErrorLlavero::Backend`] con un mensaje fijo.
fn fuera_del_ejecutor<T, F>(operacion: F) -> Result<T, ErrorLlavero>
where
    T: Send,
    F: FnOnce() -> Result<T, ErrorLlavero> + Send,
{
    std::thread::scope(|ambito| {
        ambito.spawn(operacion).join().unwrap_or_else(|_| {
            Err(ErrorLlavero::Backend(
                "la operación con el llavero terminó de forma inesperada".to_string(),
            ))
        })
    })
}

/// Traduce un error de `keyring` a [`ErrorLlavero`] usando solo su variante y su
/// `Display`, nunca su `Debug`: para `Error::BadEncoding(Vec<u8>)` y
/// `Error::BadDataFormat(Vec<u8>, _)` el `#[derive(Debug)]` de `keyring_core::Error`
/// (`keyring-core-1.0.0/src/error.rs`) imprime literalmente los bytes del secreto
/// recuperado, mientras que su `impl Display` (mismo fichero) usa mensajes fijos o
/// solo el error de causa, sin los bytes. Por eso aquí solo se usa `{err}`
/// (`Display`/`to_string`), nunca `{err:?}`.
fn error_keyring(err: &keyring::Error) -> ErrorLlavero {
    match err {
        keyring::Error::NoStorageAccess(_) => {
            ErrorLlavero::Bloqueado(format!("el llavero del sistema está bloqueado: {err}"))
        }
        keyring::Error::PlatformFailure(_) | keyring::Error::NoDefaultStore => {
            ErrorLlavero::NoDisponible(format!(
                "no hay llavero del sistema disponible (Secret Service de D-Bus): {err}"
            ))
        }
        _ => ErrorLlavero::Backend(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(valor: &str) -> Nombre {
        Nombre::nuevo(valor).expect("nombre de prueba válido")
    }

    #[test]
    fn usuario_combina_cuenta_y_etiqueta_de_clave() {
        assert_eq!(
            usuario(&nombre("jparga"), ClaveSecreto::TokenGithub),
            "jparga/token-github"
        );
        assert_eq!(
            usuario(&nombre("jparga"), ClaveSecreto::PasswordAdminGitea),
            "jparga/password-admin-gitea"
        );
        assert_eq!(
            usuario(&nombre("jparga"), ClaveSecreto::TokenGitea),
            "jparga/token-gitea"
        );
    }

    #[test]
    fn no_storage_access_se_mapea_a_bloqueado() {
        let err = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("bloqueado")));
        assert!(matches!(error_keyring(&err), ErrorLlavero::Bloqueado(_)));
    }

    #[test]
    fn platform_failure_se_mapea_a_no_disponible() {
        let err = keyring::Error::PlatformFailure(Box::new(std::io::Error::other("sin bus")));
        assert!(matches!(error_keyring(&err), ErrorLlavero::NoDisponible(_)));
    }

    #[test]
    fn no_default_store_se_mapea_a_no_disponible() {
        let err = keyring::Error::NoDefaultStore;
        assert!(matches!(error_keyring(&err), ErrorLlavero::NoDisponible(_)));
    }

    #[test]
    fn otros_errores_se_mapean_a_backend() {
        let err = keyring::Error::Invalid("usuario".to_string(), "vacío".to_string());
        assert!(matches!(error_keyring(&err), ErrorLlavero::Backend(_)));
    }

    #[test]
    fn el_mensaje_de_bad_encoding_no_incluye_los_bytes_del_secreto() {
        // Bytes que no forman UTF-8 válido; simulan un secreto corrupto/no textual.
        let bytes = vec![0xC0_u8, 0x9E, 0x41, 0x99];
        let err = keyring::Error::BadEncoding(bytes.clone());
        let mapeado = error_keyring(&err);
        let ErrorLlavero::Backend(mensaje) = mapeado else {
            panic!("se esperaba ErrorLlavero::Backend");
        };
        for byte in bytes {
            assert!(!mensaje.contains(&byte.to_string()));
        }
        assert_eq!(mensaje, "Password data is not valid UTF-8");
    }
}

/// Pruebas que requieren un runtime `tokio` multi-hilo real, para comprobar que
/// llamar a este llavero desde uno de sus hilos de trabajo no entra en pánico.
#[cfg(test)]
mod tests_tokio {
    use super::*;

    /// Reproduce lo que hace `zbus` compilado con `tokio`: un `block_on` de otro
    /// ejecutor. Directamente desde aquí entraría en pánico; en el hilo aparte, no.
    #[tokio::test(flavor = "multi_thread")]
    async fn fuera_del_ejecutor_admite_un_ejecutor_anidado() {
        let resultado = fuera_del_ejecutor(|| {
            let ejecutor = tokio::runtime::Builder::new_current_thread()
                .build()
                .map_err(|error| ErrorLlavero::Backend(error.to_string()))?;
            Ok(ejecutor.block_on(async { 7 }))
        });
        assert!(matches!(resultado, Ok(7)));
    }

    #[test]
    fn fuera_del_ejecutor_convierte_un_panico_en_error() {
        let resultado: Result<(), ErrorLlavero> =
            fuera_del_ejecutor(|| panic!("fallo simulado del backend"));
        assert!(matches!(resultado, Err(ErrorLlavero::Backend(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "usa el llavero real del sistema"]
    async fn nuevo_no_entra_en_panico_dentro_de_un_runtime_tokio() {
        match LlaveroDelSistema::nuevo() {
            Ok(_) => {}
            Err(ErrorLlavero::NoDisponible(_)) => {}
            Err(otro) => panic!("error inesperado desde dentro de tokio: {otro}"),
        }
    }
}

/// Pruebas de integración contra el llavero real del sistema. No se ejecutan por
/// defecto (las pruebas normales nunca tocan el llavero real). Se lanzan
/// con `cargo nextest run -p gitmereba-core --run-ignored ignored-only sistema`.
#[cfg(test)]
mod tests_integracion {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    /// Cuenta de prueba con un login único que se borra del llavero real al
    /// soltarse, incluso si el test entra en pánico a mitad de camino.
    struct CuentaDePrueba(Nombre);

    impl CuentaDePrueba {
        fn nueva() -> Self {
            let sufijo = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let nombre = format!("gitmereba-test-{}-{sufijo}", std::process::id());
            Self(Nombre::nuevo(nombre).expect("nombre de prueba válido"))
        }
    }

    impl Drop for CuentaDePrueba {
        fn drop(&mut self) {
            if let Ok(llavero) = LlaveroDelSistema::nuevo() {
                let _ = llavero.borrar_cuenta(&self.0);
            }
        }
    }

    fn llavero_de_pruebas() -> LlaveroDelSistema {
        LlaveroDelSistema::nuevo().expect("el llavero real del sistema debe estar disponible")
    }

    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn ida_y_vuelta() {
        let cuenta = CuentaDePrueba::nueva();
        let llavero = llavero_de_pruebas();
        llavero
            .guardar(
                &cuenta.0,
                ClaveSecreto::TokenGithub,
                &Secreto::nuevo("ghp_prueba"),
            )
            .expect("guardar no falla");

        let leido = llavero
            .leer(&cuenta.0, ClaveSecreto::TokenGithub)
            .expect("leer no falla");
        assert_eq!(
            leido.map(|s| s.exponer().to_string()),
            Some("ghp_prueba".to_string())
        );
    }

    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn leer_inexistente_es_none() {
        let cuenta = CuentaDePrueba::nueva();
        let llavero = llavero_de_pruebas();
        assert!(
            llavero
                .leer(&cuenta.0, ClaveSecreto::TokenGitea)
                .expect("leer no falla")
                .is_none()
        );
    }

    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn borrar_inexistente_es_ok() {
        let cuenta = CuentaDePrueba::nueva();
        let llavero = llavero_de_pruebas();
        assert!(llavero.borrar(&cuenta.0, ClaveSecreto::TokenGitea).is_ok());
    }

    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn borrar_cuenta_quita_las_tres_claves() {
        let cuenta = CuentaDePrueba::nueva();
        let llavero = llavero_de_pruebas();
        let claves = [
            ClaveSecreto::TokenGithub,
            ClaveSecreto::PasswordAdminGitea,
            ClaveSecreto::TokenGitea,
        ];
        for clave in claves {
            llavero
                .guardar(&cuenta.0, clave, &Secreto::nuevo("valor"))
                .expect("guardar no falla");
        }

        llavero
            .borrar_cuenta(&cuenta.0)
            .expect("borrar_cuenta no falla");

        for clave in claves {
            assert!(
                llavero
                    .leer(&cuenta.0, clave)
                    .expect("leer no falla")
                    .is_none()
            );
        }
    }

    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn sobrescritura_devuelve_el_ultimo_valor() {
        let cuenta = CuentaDePrueba::nueva();
        let llavero = llavero_de_pruebas();
        llavero
            .guardar(
                &cuenta.0,
                ClaveSecreto::TokenGitea,
                &Secreto::nuevo("primero"),
            )
            .expect("guardar no falla");
        llavero
            .guardar(
                &cuenta.0,
                ClaveSecreto::TokenGitea,
                &Secreto::nuevo("segundo"),
            )
            .expect("guardar no falla");

        let leido = llavero
            .leer(&cuenta.0, ClaveSecreto::TokenGitea)
            .expect("leer no falla");
        assert_eq!(
            leido.map(|s| s.exponer().to_string()),
            Some("segundo".to_string())
        );
    }
}
