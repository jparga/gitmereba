//! DTOs de los comandos de acción del contrato con la interfaz.
//!
//! Solo estructuras de datos, sin lógica: los tipos de `core` que ya derivan
//! `Serialize`/`Deserialize` con la forma exacta del contrato se usan directamente
//! (`modelo::Alcance`, `modelo::Cuenta`, `modelo::IdRepo`); estos DTOs cubren los sitios
//! donde el tipo de `core` no deriva esos traits (`github::Identidad`) o donde su forma
//! no coincide con la del contrato (`Previsualizacion`, el progreso del alta).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use gitmereba_core::cuentas::{Descubrimiento, EstadoPaso, PasoAlta};
use gitmereba_core::idioma::{Idioma, Localizable};
use gitmereba_core::modelo::{Cuenta, RepoOrigen};
use gitmereba_core::sync::{FaseSync, ProgresoSync};

/// `{ ok: true }`: la respuesta de éxito sin más datos de varios comandos del contrato.
#[derive(Debug, Clone, Serialize)]
pub struct RespuestaOk {
    pub ok: bool,
}

impl RespuestaOk {
    pub fn si() -> Self {
        Self { ok: true }
    }
}

/// `elegir_carpeta`: `{ carpeta: string | null }`.
#[derive(Debug, Clone, Serialize)]
pub struct RespuestaCarpeta {
    pub carpeta: Option<String>,
}

/// Argumento `ajustes` de `ajustes_guardar`.
#[derive(Debug, Deserialize)]
pub struct AjustesCuentaEntrada {
    pub intervalo_minutos: u32,
    pub carpeta: String,
    pub alcance: gitmereba_core::modelo::Alcance,
}

/// Resumen de `sync::InformeSync` (o de una sincronización dirigida a un solo repo) que
/// necesita la UI: ver el comando `sincronizar` del contrato con la interfaz.
#[derive(Debug, Clone, Serialize)]
pub struct InformeSincResumen {
    pub cuenta: Cuenta,
    #[serde(with = "time::serde::rfc3339")]
    pub inicio: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub fin: OffsetDateTime,
    pub resultado: String,
    pub resumen: String,
}

/// `github::Identidad` en el asistente de alta: hoy no deriva `Serialize`, así que este
/// es el DTO fino que exige la capa de ventana (comando `validar_alta` del contrato con la interfaz).
#[derive(Debug, Clone, Serialize)]
pub struct IdentidadDto {
    pub login: String,
    pub scopes: Vec<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub caduca: Option<OffsetDateTime>,
}

/// Un repo descubierto en el asistente de alta, con el subconjunto de campos de
/// `modelo::RepoOrigen` que pide el contrato.
#[derive(Debug, Clone, Serialize)]
pub struct RepoAltaDto {
    pub dueno: String,
    pub nombre: String,
    pub privado: bool,
    pub es_fork: bool,
    pub tamano_kb: u64,
}

impl From<&RepoOrigen> for RepoAltaDto {
    fn from(repo: &RepoOrigen) -> Self {
        Self {
            dueno: repo.id.dueno.to_string(),
            nombre: repo.id.nombre.to_string(),
            privado: repo.privado,
            es_fork: repo.es_fork,
            tamano_kb: repo.tamano_kb,
        }
    }
}

/// `validar_alta`: paso 1-2 del asistente, según el contrato con la interfaz.
#[derive(Debug, Clone, Serialize)]
pub struct PreviaAlta {
    pub identidad: IdentidadDto,
    pub repos: Vec<RepoAltaDto>,
    pub organizaciones: Vec<String>,
    pub total_tamano_kb: u64,
}

impl From<Descubrimiento> for PreviaAlta {
    fn from(descubrimiento: Descubrimiento) -> Self {
        let total_tamano_kb = descubrimiento.repos.iter().map(|repo| repo.tamano_kb).sum();
        Self {
            identidad: IdentidadDto {
                login: descubrimiento.identidad.login.to_string(),
                scopes: descubrimiento.identidad.scopes,
                caduca: descubrimiento.identidad.caduca,
            },
            repos: descubrimiento.repos.iter().map(RepoAltaDto::from).collect(),
            organizaciones: descubrimiento
                .organizaciones
                .iter()
                .map(ToString::to_string)
                .collect(),
            total_tamano_kb,
        }
    }
}

/// Payload del evento `alta://progreso` (comando `crear_cuenta` del contrato con la interfaz).
#[derive(Debug, Clone, Serialize)]
pub struct ProgresoAltaPayload {
    pub paso: u32,
    pub total: u32,
    pub texto: String,
    pub estado: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mensaje: Option<String>,
}

/// Cuántos pasos tiene el alta: el número de variantes de [`PasoAlta`].
pub const TOTAL_PASOS_ALTA: u32 = 9;

/// Posición (1-based) de `paso` entre los pasos del alta, en el orden fijo en que los
/// recorre `cuentas::alta`.
pub fn indice_paso(paso: PasoAlta) -> u32 {
    match paso {
        PasoAlta::Validar => 1,
        PasoAlta::GuardarToken => 2,
        PasoAlta::CrearCarpeta => 3,
        PasoAlta::EscribirConfiguracion => 4,
        PasoAlta::AsegurarBinario => 5,
        PasoAlta::Provisionar => 6,
        PasoAlta::ArrancarGitea => 7,
        PasoAlta::PrimeraSincronizacion => 8,
        PasoAlta::RegistrarEnAlmacen => 9,
    }
}

/// `estado` del contrato para una transición de [`EstadoPaso`]: `"en-curso"` o `"ok"`.
/// El tercer valor posible, `"error"`, no viene de `EstadoPaso` (que no lo distingue):
/// lo añade el llamador cuando `cuentas::alta` termina en `Err` (ver `acciones::crear_cuenta`).
pub fn estado_paso_a_texto(estado: EstadoPaso) -> &'static str {
    match estado {
        EstadoPaso::Iniciando => "en-curso",
        EstadoPaso::Hecho => "ok",
    }
}

/// Construye el payload de un paso en curso o terminado.
pub fn payload_progreso(paso: PasoAlta, estado: EstadoPaso, idioma: Idioma) -> ProgresoAltaPayload {
    ProgresoAltaPayload {
        paso: indice_paso(paso),
        total: TOTAL_PASOS_ALTA,
        texto: paso.localizar(idioma),
        estado: estado_paso_a_texto(estado).to_string(),
        mensaje: None,
    }
}

/// Construye el payload final de error: mismo paso que la última transición emitida,
/// con el mensaje (ya sin secretos) del error de `core`.
pub fn payload_progreso_error(ultimo_paso: u32, mensaje: String) -> ProgresoAltaPayload {
    ProgresoAltaPayload {
        paso: ultimo_paso,
        total: TOTAL_PASOS_ALTA,
        texto: String::new(),
        estado: "error".to_string(),
        mensaje: Some(mensaje),
    }
}

/// Payload del evento `sync://progreso` (comando `sincronizar` del contrato con la interfaz), emitido
/// mientras se sincroniza una cuenta entera desde la ventana (comando `sincronizar` sin
/// `id`). `fase` serializa como `FaseSync` (`"listando" | "aplicando" | "verificando"`,
/// `#[serde(rename_all = "snake_case")]` en `gitmereba_core::sync::FaseSync`).
#[derive(Debug, Clone, Serialize)]
pub struct ProgresoSyncPayload {
    pub login: String,
    pub fase: FaseSync,
    pub hechos: usize,
    pub total: usize,
}

/// Construye el payload de `sync://progreso` a partir de un [`ProgresoSync`] de `core`.
pub fn payload_progreso_sync(login: &str, progreso: ProgresoSync) -> ProgresoSyncPayload {
    ProgresoSyncPayload {
        login: login.to_string(),
        fase: progreso.fase,
        hechos: progreso.hechos,
        total: progreso.total,
    }
}

/// `reconciliar`, según el contrato con la interfaz.
#[derive(Debug, Clone, Serialize)]
pub struct ResultadoReconciliacion {
    pub resultado: String,
    pub mensaje: String,
}

/// `actualizar_gitea`: `{ ok: true, version: string }`.
#[derive(Debug, Clone, Serialize)]
pub struct RespuestaActualizarGitea {
    pub ok: bool,
    pub version: String,
}

/// Respuesta de `credenciales_gitea`. Sin `Debug`: lleva la contraseña en claro camino de
/// la interfaz y no debe poder acabar en una traza.
#[derive(Serialize)]
pub struct CredencialesGiteaDto {
    pub usuario: String,
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indice_paso_recorre_los_nueve_pasos_en_orden() {
        let orden = [
            PasoAlta::Validar,
            PasoAlta::GuardarToken,
            PasoAlta::CrearCarpeta,
            PasoAlta::EscribirConfiguracion,
            PasoAlta::AsegurarBinario,
            PasoAlta::Provisionar,
            PasoAlta::ArrancarGitea,
            PasoAlta::PrimeraSincronizacion,
            PasoAlta::RegistrarEnAlmacen,
        ];
        for (posicion, paso) in orden.into_iter().enumerate() {
            assert_eq!(indice_paso(paso), posicion as u32 + 1);
        }
        assert_eq!(TOTAL_PASOS_ALTA, orden.len() as u32);
    }

    #[test]
    fn estado_paso_a_texto_usa_los_literales_del_contrato() {
        assert_eq!(estado_paso_a_texto(EstadoPaso::Iniciando), "en-curso");
        assert_eq!(estado_paso_a_texto(EstadoPaso::Hecho), "ok");
    }

    #[test]
    fn payload_progreso_usa_el_idioma_pedido() {
        let payload = payload_progreso(PasoAlta::Provisionar, EstadoPaso::Hecho, Idioma::En);
        assert_eq!(payload.texto, "Provisioning Gitea");
    }

    #[test]
    fn payload_progreso_no_lleva_mensaje() {
        let payload =
            payload_progreso(PasoAlta::AsegurarBinario, EstadoPaso::Iniciando, Idioma::Es);
        assert_eq!(payload.paso, 5);
        assert_eq!(payload.texto, "Comprobando el binario de Gitea");
        assert_eq!(payload.estado, "en-curso");
        assert!(payload.mensaje.is_none());
        let json = serde_json::to_value(&payload).expect("serializa");
        assert!(json.get("mensaje").is_none());
    }

    #[test]
    fn payload_progreso_error_lleva_el_mensaje_y_el_ultimo_paso() {
        let payload = payload_progreso_error(5, "fallo simulado".to_string());
        assert_eq!(payload.paso, 5);
        assert_eq!(payload.estado, "error");
        assert_eq!(payload.mensaje.as_deref(), Some("fallo simulado"));
    }

    #[test]
    fn payload_progreso_sync_serializa_la_fase_en_snake_case() {
        let payload = payload_progreso_sync(
            "jparga",
            ProgresoSync {
                fase: FaseSync::Aplicando,
                hechos: 2,
                total: 5,
            },
        );
        let json = serde_json::to_value(&payload).expect("serializa");
        assert_eq!(json["login"], "jparga");
        assert_eq!(json["fase"], "aplicando");
        assert_eq!(json["hechos"], 2);
        assert_eq!(json["total"], 5);
    }

    #[test]
    fn payload_progreso_sync_cubre_las_tres_fases() {
        for (fase, texto) in [
            (FaseSync::Listando, "listando"),
            (FaseSync::Aplicando, "aplicando"),
            (FaseSync::Verificando, "verificando"),
        ] {
            let payload = payload_progreso_sync(
                "jparga",
                ProgresoSync {
                    fase,
                    hechos: 0,
                    total: 0,
                },
            );
            let json = serde_json::to_value(&payload).expect("serializa");
            assert_eq!(json["fase"], texto);
        }
    }
}
