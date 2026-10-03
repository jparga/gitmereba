//! Comandos del idioma de la ventana: consultar el idioma resuelto y la preferencia, y
//! cambiar la preferencia (se guarda en `preferencias.toml`).

use gitmereba_core::config::Rutas;
use gitmereba_core::idioma::{
    LecturaPreferencia, Preferencia, guardar_preferencia, idioma_actual, leer_preferencia,
};
use serde::Serialize;
use tauri::State;

use super::error::{ErrorUi, texto};
use super::estado::EstadoApp;

/// Respuesta de `idioma`: el idioma ya resuelto (para pintar) y la preferencia guardada
/// (para el selector de Ajustes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdiomaDto {
    pub idioma: String,
    pub preferencia: String,
}

fn codigo_preferencia(pref: Preferencia) -> &'static str {
    match pref {
        Preferencia::Auto => "auto",
        Preferencia::Es => "es",
        Preferencia::En => "en",
    }
}

/// Lee idioma y preferencia. Pura sobre `Rutas` para probarla sin ventana.
fn leer_idioma(rutas: &Rutas) -> IdiomaDto {
    let lectura: LecturaPreferencia = leer_preferencia(rutas);
    IdiomaDto {
        idioma: idioma_actual(rutas).codigo().to_string(),
        preferencia: codigo_preferencia(lectura.preferencia()).to_string(),
    }
}

/// Valida `valor` (`auto`, `es` o `en`) y lo guarda como preferencia.
fn guardar_idioma(rutas: &Rutas, valor: &str) -> Result<(), ErrorUi> {
    let idioma = idioma_actual(rutas);
    let pref = match valor {
        "auto" => Preferencia::Auto,
        "es" => Preferencia::Es,
        "en" => Preferencia::En,
        _ => {
            return Err(ErrorUi::nuevo(
                "datos_invalidos",
                texto(
                    idioma,
                    "el idioma debe ser «auto», «es» o «en»",
                    "the language must be \"auto\", \"es\" or \"en\"",
                ),
            ));
        }
    };
    guardar_preferencia(rutas, pref).map_err(|error| ErrorUi::de(&error, idioma))
}

#[tauri::command]
pub fn idioma(estado: State<'_, EstadoApp>) -> IdiomaDto {
    tracing::debug!(comando = "idioma");
    leer_idioma(&estado.rutas)
}

#[tauri::command]
pub fn fijar_idioma(estado: State<'_, EstadoApp>, preferencia: String) -> Result<(), ErrorUi> {
    tracing::debug!(comando = "fijar_idioma");
    guardar_idioma(&estado.rutas, &preferencia)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rutas() -> (tempfile::TempDir, Rutas) {
        let dir = tempfile::tempdir().expect("tempdir");
        let rutas = Rutas::con_raiz(dir.path());
        (dir, rutas)
    }

    #[test]
    fn un_valor_invalido_se_rechaza_y_no_escribe_nada() {
        let (_dir, rutas) = rutas();
        let error = guardar_idioma(&rutas, "fr").expect_err("fr no es válido");
        assert_eq!(error.codigo, "datos_invalidos");
        assert!(!rutas.fichero_preferencias().exists());
    }

    #[test]
    fn un_valor_valido_se_guarda_y_se_lee() {
        let (_dir, rutas) = rutas();
        guardar_idioma(&rutas, "es").expect("es es válido");
        let leido = leer_idioma(&rutas);
        assert_eq!(leido.preferencia, "es");
        assert_eq!(leido.idioma, "es");
        guardar_idioma(&rutas, "en").expect("en es válido");
        let leido = leer_idioma(&rutas);
        assert_eq!(leido.preferencia, "en");
        assert_eq!(leido.idioma, "en");
        guardar_idioma(&rutas, "auto").expect("auto es válido");
        assert_eq!(leer_idioma(&rutas).preferencia, "auto");
    }

    #[test]
    fn sin_fichero_la_preferencia_es_auto() {
        let (_dir, rutas) = rutas();
        assert_eq!(leer_idioma(&rutas).preferencia, "auto");
    }
}
