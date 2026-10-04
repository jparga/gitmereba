// Capa de datos: si la app corre dentro de Tauri usa `invoke` real; si no (previsualización
// en un navegador normal), cae al doble de ui/js/mock.js. Los nombres de comando y las
// formas de argumentos/respuesta están documentados en el contrato con la interfaz (lado Rust: app/src/ventana/).

import * as mock from './mock.js';

function invocarTauri() {
  return globalThis.__TAURI__?.core?.invoke;
}

async function llamar(comando, args) {
  const invoke = invocarTauri();
  if (invoke) return invoke(comando, args);
  return mock.invocar(comando, args);
}

/** Se suscribe a los eventos de progreso de `crear_cuenta`. Devuelve una promesa con la
 * función para darse de baja. */
export function suscribirProgresoAlta(callback) {
  const listen = globalThis.__TAURI__?.event?.listen;
  if (listen) {
    return listen('alta://progreso', (evento) => callback(evento.payload));
  }
  return mock.suscribirProgreso(callback);
}

/** Se suscribe a los eventos de progreso de `sincronizar` (cuenta entera, sin `id`).
 * Devuelve una promesa con la función para darse de baja. */
export function suscribirProgresoSync(callback) {
  const listen = globalThis.__TAURI__?.event?.listen;
  if (listen) {
    return listen('sync://progreso', (evento) => callback(evento.payload));
  }
  return mock.suscribirProgresoSync(callback);
}

export const api = {
  acercaDe: () => llamar('acerca_de'),
  versionGiteaCuenta: (login) => llamar('version_gitea_cuenta', { login }),
  abrirEnlace: (destino) => llamar('abrir_enlace', { destino }),
  listarCuentas: () => llamar('listar_cuentas'),
  resumenCuenta: (login) => llamar('resumen_cuenta', { login }),
  listarRepos: (login) => llamar('listar_repos', { login }),
  excluirRepo: (login, id, excluido) => llamar('excluir_repo', { login, id, excluido }),
  sincronizar: (login, id) => llamar('sincronizar', { login, id: id ?? null }),
  idioma: () => llamar('idioma'),
  fijarIdioma: (preferencia) => llamar('fijar_idioma', { preferencia }),
  estadoGithub: () => llamar('estado_github'),
  elegirCarpeta: () => llamar('elegir_carpeta'),
  validarAlta: (usuario, token, carpeta) => llamar('validar_alta', { usuario, token, carpeta }),
  crearCuenta: (datos) => llamar('crear_cuenta', datos),
  historial: (login, limite = 50) => llamar('historial', { login: login ?? null, limite }),
  auditoria: (limite = 50, desdeId = null) => llamar('auditoria', { limite, desde_id: desdeId }),
  verificarAuditoria: () => llamar('verificar_auditoria'),
  estadoContingencia: (login) => llamar('estado_contingencia', { login }),
  activarContingencia: (login, id) => llamar('activar_contingencia', { login, id }),
  reconciliar: (login, id, token) => llamar('reconciliar', { login, id, token }),
  ajustesLeer: (login) => llamar('ajustes_leer', { login }),
  ajustesGuardar: (login, ajustes) => llamar('ajustes_guardar', { login, ajustes }),
  rotarToken: (login, token) => llamar('rotar_token', { login, token }),
  bajaCuenta: (login) => llamar('baja_cuenta', { login }),
  abrirGitea: (login) => llamar('abrir_gitea', { login }),
  credencialesGitea: (login) => llamar('credenciales_gitea', { login }),
  actualizarGitea: () => llamar('actualizar_gitea'),
  lanEstado: (login) => llamar('lan_estado', { login }),
  lanActivar: (login, host) => llamar('lan_activar', { login, host: host ?? null }),
  lanDesactivar: (login) => llamar('lan_desactivar', { login }),
  lanUsuariosListar: (login) => llamar('lan_usuarios_listar', { login }),
  lanUsuarioCrear: (login, nombre) => llamar('lan_usuario_crear', { login, nombre }),
  lanUsuarioEliminar: (login, nombre) => llamar('lan_usuario_eliminar', { login, nombre }),
};
