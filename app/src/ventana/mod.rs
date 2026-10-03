//! Ventana Tauri: carga `ui/` (estático, empaquetado en el binario) y expone los
//! comandos del contrato con la interfaz (`ui/js/api.js`) como envoltorios finos sobre `gitmereba_core`.

mod acciones;
mod dto;
mod dto_acciones;
mod error;
mod estado;
mod idioma;
mod lan;
mod lectura;

use gitmereba_core::config::{self, Rutas, RutasCuenta};
use gitmereba_core::cuentas;

use estado::EstadoApp;

/// Abre la ventana principal y bloquea hasta que se cierra.
pub fn abrir(rutas: Rutas) -> Result<(), tauri::Error> {
    reparar_temporizadores_en_segundo_plano(rutas.clone());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(EstadoApp { rutas })
        .invoke_handler(tauri::generate_handler![
            idioma::idioma,
            idioma::fijar_idioma,
            lectura::listar_cuentas,
            lectura::resumen_cuenta,
            lectura::listar_repos,
            lectura::historial,
            lectura::auditoria,
            lectura::verificar_auditoria,
            lectura::estado_github,
            lectura::estado_contingencia,
            lectura::ajustes_leer,
            acciones::excluir_repo,
            acciones::sincronizar,
            acciones::elegir_carpeta,
            acciones::validar_alta,
            acciones::crear_cuenta,
            acciones::activar_contingencia,
            acciones::reconciliar,
            acciones::ajustes_guardar,
            acciones::rotar_token,
            acciones::baja_cuenta,
            acciones::abrir_gitea,
            acciones::actualizar_gitea,
            acciones::credenciales_gitea,
            lan::lan_estado,
            lan::lan_activar,
            lan::lan_desactivar,
            lan::lan_usuarios_listar,
            lan::lan_usuario_crear,
            lan::lan_usuario_eliminar,
        ])
        .run(tauri::generate_context!())
}

/// Al abrir la ventana, deja el temporizador de cada cuenta apuntando a este ejecutable.
///
/// Tras instalar la app como paquete, las cuentas dadas de alta con otro binario (p. ej.
/// `target/debug`) seguirían sincronizando con el antiguo. No bloquea el arranque y un
/// fallo solo se registra: la ventana funciona igual.
fn reparar_temporizadores_en_segundo_plano(rutas: Rutas) {
    std::thread::spawn(move || {
        let Ok(ejecutor) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        let Ok(ejecutable) = std::env::current_exe() else {
            return;
        };
        let Ok(indice) = config::leer_indice_cuentas(&rutas) else {
            return;
        };
        ejecutor.block_on(async {
            for entrada in indice.cuentas.values() {
                let rutas_cuenta = RutasCuenta::nueva(entrada.carpeta.clone());
                let Ok(cuenta) = config::leer_cuenta(&rutas_cuenta) else {
                    continue;
                };
                match cuentas::reparar_temporizador(&rutas, &cuenta, &ejecutable).await {
                    Ok(true) => tracing::info!(cuenta = %cuenta.login, "temporizador actualizado"),
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(cuenta = %cuenta.login, %error, "no se pudo actualizar el temporizador");
                    }
                }
            }
        });
    });
}
