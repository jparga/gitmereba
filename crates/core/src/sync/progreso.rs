//! Avance real de una pasada de sincronización, para informar a quien la ha pedido (la
//! ventana, tras la tarea «progreso real de la pasada de sincronización»). Sin nada
//! secreto: solo una fase y dos contadores.

use serde::Serialize;

/// En qué fase está una pasada de [`super::ejecutar_con_progreso`]. `Verificando` no la
/// emite `sync` (que no verifica nada): la añade
/// `cuentas::sincronizar_y_verificar_con_progreso`, justo antes de llamar a
/// `verificacion::verificar_cuenta`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaseSync {
    Listando,
    Aplicando,
    Verificando,
}

/// Avance de una pasada de sincronización en el momento de notificarlo. En las fases
/// `Listando` y `Verificando` (indeterminadas: no se sabe cuánto van a tardar) `hechos` y
/// `total` van siempre a `0`; en `Aplicando`, `total` es el número de acciones del plan
/// que de verdad se ejecutan y `hechos` crece de `0` a `total` según se aplica cada una.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgresoSync {
    pub fase: FaseSync,
    pub hechos: usize,
    pub total: usize,
}
