//! Trait [`Llavero`] para guardar los secretos de una cuenta, y un doble en memoria.
//!
//! La implementación real sobre Secret Service es otra tarea; aquí solo vive el
//! contrato y el doble que usan las pruebas y el resto del `core` mientras tanto.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::modelo::Nombre;
use crate::secretos::Secreto;

/// Qué secreto de una cuenta se guarda en el llavero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClaveSecreto {
    TokenGithub,
    PasswordAdminGitea,
    TokenGitea,
}

impl ClaveSecreto {
    /// Las tres claves que puede tener una cuenta, en el orden en que se borran.
    const TODAS: [ClaveSecreto; 3] = [
        ClaveSecreto::TokenGithub,
        ClaveSecreto::PasswordAdminGitea,
        ClaveSecreto::TokenGitea,
    ];

    /// Nombre corto y estable de la clave, usado como parte del atributo de
    /// usuario con el que se identifica el secreto en el llavero del sistema.
    pub fn etiqueta(&self) -> &'static str {
        match self {
            ClaveSecreto::TokenGithub => "token-github",
            ClaveSecreto::PasswordAdminGitea => "password-admin-gitea",
            ClaveSecreto::TokenGitea => "token-gitea",
        }
    }
}

/// Error al acceder al llavero. El mensaje nunca incluye el valor del secreto.
#[derive(Debug, thiserror::Error)]
pub enum ErrorLlavero {
    #[error("no se pudo acceder al llavero del sistema: {0}")]
    Backend(String),

    /// No hay un llavero del sistema accesible: no hay bus de sesión D-Bus, no
    /// hay demonio de Secret Service, o la plataforma no está soportada.
    #[error("no hay llavero del sistema disponible: {0}")]
    NoDisponible(String),

    /// El llavero del sistema existe pero está bloqueado o el acceso fue denegado.
    #[error("el llavero del sistema está bloqueado: {0}")]
    Bloqueado(String),
}

/// Acceso al llavero de secretos, indexado por cuenta y clave.
///
/// Las implementaciones deben ser seguras entre hilos: la app las comparte entre las
/// tareas de sincronización y la interfaz.
pub trait Llavero: Send + Sync {
    /// Guarda (o reemplaza) el valor de `clave` para `cuenta`.
    fn guardar(
        &self,
        cuenta: &Nombre,
        clave: ClaveSecreto,
        valor: &Secreto,
    ) -> Result<(), ErrorLlavero>;

    /// Lee el valor de `clave` para `cuenta`; `None` si no se guardó.
    fn leer(&self, cuenta: &Nombre, clave: ClaveSecreto) -> Result<Option<Secreto>, ErrorLlavero>;

    /// Borra `clave` de `cuenta`. Borrar algo que no existe no es un error.
    fn borrar(&self, cuenta: &Nombre, clave: ClaveSecreto) -> Result<(), ErrorLlavero>;

    /// Borra los tres secretos posibles de `cuenta` (token de GitHub, contraseña y
    /// token de administración de Gitea).
    fn borrar_cuenta(&self, cuenta: &Nombre) -> Result<(), ErrorLlavero> {
        for clave in ClaveSecreto::TODAS {
            self.borrar(cuenta, clave)?;
        }
        Ok(())
    }
}

/// Doble en memoria de [`Llavero`]: para pruebas y para que el resto del `core`
/// compile sin depender de un Secret Service real.
#[derive(Default)]
pub struct LlaveroEnMemoria {
    valores: Mutex<HashMap<(Nombre, ClaveSecreto), Secreto>>,
}

impl LlaveroEnMemoria {
    /// Crea un llavero en memoria vacío.
    pub fn nuevo() -> Self {
        Self::default()
    }
}

impl Llavero for LlaveroEnMemoria {
    fn guardar(
        &self,
        cuenta: &Nombre,
        clave: ClaveSecreto,
        valor: &Secreto,
    ) -> Result<(), ErrorLlavero> {
        let mut valores = bloquear(&self.valores)?;
        valores.insert((cuenta.clone(), clave), valor.clone());
        Ok(())
    }

    fn leer(&self, cuenta: &Nombre, clave: ClaveSecreto) -> Result<Option<Secreto>, ErrorLlavero> {
        let valores = bloquear(&self.valores)?;
        Ok(valores.get(&(cuenta.clone(), clave)).cloned())
    }

    fn borrar(&self, cuenta: &Nombre, clave: ClaveSecreto) -> Result<(), ErrorLlavero> {
        let mut valores = bloquear(&self.valores)?;
        valores.remove(&(cuenta.clone(), clave));
        Ok(())
    }
}

/// Bloquea el mutex traduciendo el envenenamiento a [`ErrorLlavero`].
fn bloquear<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, ErrorLlavero> {
    mutex.lock().map_err(|_| {
        ErrorLlavero::Backend("el llavero en memoria quedó en estado inconsistente".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nombre(valor: &str) -> Nombre {
        Nombre::nuevo(valor).expect("nombre de prueba válido")
    }

    #[test]
    fn guardar_y_leer_devuelve_el_mismo_valor() {
        let llavero = LlaveroEnMemoria::nuevo();
        let cuenta = nombre("jparga");
        llavero
            .guardar(&cuenta, ClaveSecreto::TokenGithub, &Secreto::nuevo("ghp_x"))
            .expect("guardar no falla");

        let leido = llavero
            .leer(&cuenta, ClaveSecreto::TokenGithub)
            .expect("leer no falla");
        assert_eq!(
            leido.map(|s| s.exponer().to_string()),
            Some("ghp_x".to_string())
        );
    }

    #[test]
    fn leer_algo_no_guardado_devuelve_none() {
        let llavero = LlaveroEnMemoria::nuevo();
        let cuenta = nombre("jparga");
        assert!(
            llavero
                .leer(&cuenta, ClaveSecreto::TokenGitea)
                .expect("leer no falla")
                .is_none()
        );
    }

    #[test]
    fn las_cuentas_estan_aisladas_entre_si() {
        let llavero = LlaveroEnMemoria::nuevo();
        let a = nombre("cuenta-a");
        let b = nombre("cuenta-b");
        llavero
            .guardar(&a, ClaveSecreto::TokenGithub, &Secreto::nuevo("token-a"))
            .expect("guardar no falla");

        assert!(
            llavero
                .leer(&b, ClaveSecreto::TokenGithub)
                .expect("leer no falla")
                .is_none()
        );
        assert_eq!(
            llavero
                .leer(&a, ClaveSecreto::TokenGithub)
                .expect("leer no falla")
                .map(|s| s.exponer().to_string()),
            Some("token-a".to_string())
        );
    }

    #[test]
    fn etiqueta_es_distinta_para_cada_clave() {
        let etiquetas: Vec<&str> = ClaveSecreto::TODAS.iter().map(|c| c.etiqueta()).collect();
        assert_eq!(
            etiquetas,
            ["token-github", "password-admin-gitea", "token-gitea"]
        );
    }

    #[test]
    fn borrar_algo_inexistente_es_ok() {
        let llavero = LlaveroEnMemoria::nuevo();
        let cuenta = nombre("jparga");
        assert!(llavero.borrar(&cuenta, ClaveSecreto::TokenGitea).is_ok());
    }

    #[test]
    fn borrar_cuenta_quita_las_tres_claves() {
        let llavero = LlaveroEnMemoria::nuevo();
        let cuenta = nombre("jparga");
        llavero
            .guardar(&cuenta, ClaveSecreto::TokenGithub, &Secreto::nuevo("a"))
            .expect("guardar no falla");
        llavero
            .guardar(
                &cuenta,
                ClaveSecreto::PasswordAdminGitea,
                &Secreto::nuevo("b"),
            )
            .expect("guardar no falla");
        llavero
            .guardar(&cuenta, ClaveSecreto::TokenGitea, &Secreto::nuevo("c"))
            .expect("guardar no falla");

        llavero
            .borrar_cuenta(&cuenta)
            .expect("borrar_cuenta no falla");

        for clave in ClaveSecreto::TODAS {
            assert!(
                llavero
                    .leer(&cuenta, clave)
                    .expect("leer no falla")
                    .is_none()
            );
        }
    }

    #[test]
    fn borrar_cuenta_no_afecta_a_otras() {
        let llavero = LlaveroEnMemoria::nuevo();
        let a = nombre("cuenta-a");
        let b = nombre("cuenta-b");
        llavero
            .guardar(&a, ClaveSecreto::TokenGithub, &Secreto::nuevo("a"))
            .expect("guardar no falla");
        llavero
            .guardar(&b, ClaveSecreto::TokenGithub, &Secreto::nuevo("b"))
            .expect("guardar no falla");

        llavero.borrar_cuenta(&a).expect("borrar_cuenta no falla");

        assert!(
            llavero
                .leer(&a, ClaveSecreto::TokenGithub)
                .expect("leer no falla")
                .is_none()
        );
        assert_eq!(
            llavero
                .leer(&b, ClaveSecreto::TokenGithub)
                .expect("leer no falla")
                .map(|s| s.exponer().to_string()),
            Some("b".to_string())
        );
    }
}
