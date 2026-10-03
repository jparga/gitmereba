// Ayudas de DOM seguras: nada de innerHTML/insertAdjacentHTML. Todo el contenido
// dinámico se construye con createElement/textContent a través de h().

import { t, formatoFecha, formatoNumero, formatoRelativo } from './i18n.js';

/**
 * Crea un elemento sin pasar nunca por HTML como texto.
 *
 * `attrs` admite: `clase` (className), `dataset` (objeto), manejadores `on<Evento>`
 * (se añaden con addEventListener, nunca como atributo inline) y cualquier otro
 * atributo HTML normal. Los hijos pueden ser cadenas (se insertan como nodos de
 * texto), nodos ya creados, `null`/`false` (se ignoran) o arrays anidados.
 *
 * @param {string} etiqueta
 * @param {Object<string, unknown>|null} [attrs]
 * @param {...(Node|string|number|null|false|Array<unknown>)} hijos
 * @returns {HTMLElement}
 */
export function h(etiqueta, attrs, ...hijos) {
  const el = document.createElement(etiqueta);
  for (const [clave, valor] of Object.entries(attrs ?? {})) {
    if (valor === null || valor === undefined || valor === false) continue;
    if (clave === 'clase') {
      el.className = String(valor);
    } else if (clave === 'dataset') {
      Object.assign(el.dataset, valor);
    } else if (clave.startsWith('on') && typeof valor === 'function') {
      el.addEventListener(clave.slice(2).toLowerCase(), valor);
    } else if (valor === true) {
      el.setAttribute(clave, '');
    } else {
      el.setAttribute(clave, String(valor));
    }
  }
  const aplanados = hijos.flat(Infinity);
  for (const hijo of aplanados) {
    if (hijo === null || hijo === undefined || hijo === false) continue;
    el.append(hijo instanceof Node ? hijo : document.createTextNode(String(hijo)));
  }
  return el;
}

/** Vacía un contenedor y, opcionalmente, lo rellena con nodos nuevos. */
export function pintar(contenedor, ...hijos) {
  contenedor.replaceChildren();
  for (const hijo of hijos.flat(Infinity)) {
    if (hijo === null || hijo === undefined || hijo === false) continue;
    contenedor.append(hijo instanceof Node ? hijo : document.createTextNode(String(hijo)));
  }
}

/** Orden de urgencia de EstadoRepo, igual que el `Ord` derivado en `modelo::EstadoRepo`. */
export const ORDEN_ESTADOS = ['fallo', 'obsoleto', 'huerfano', 'contingencia', 'excluido', 'ok'];

// Funciones y no textos: el idioma se fija al arrancar, después de cargar este módulo.
const ETIQUETAS_ESTADO = {
  fallo: () => t('estado.fallo'),
  obsoleto: () => t('estado.obsoleto'),
  huerfano: () => t('estado.huerfano'),
  contingencia: () => t('estado.contingencia'),
  excluido: () => t('estado.excluido'),
  ok: () => t('estado.ok'),
};

const CLASES_BADGE_ESTADO = {
  fallo: 'crit',
  obsoleto: 'warn',
  huerfano: 'muted',
  contingencia: 'purple',
  excluido: 'offline',
  ok: 'ok',
};

export function textoEstado(estado) {
  return ETIQUETAS_ESTADO[estado]?.() ?? estado;
}

export function claseBadgeEstado(estado) {
  return CLASES_BADGE_ESTADO[estado] ?? 'muted';
}

/** `<span class="badge …">` para un EstadoRepo. */
export function badgeEstado(estado) {
  return h('span', { clase: `badge ${claseBadgeEstado(estado)}` }, textoEstado(estado));
}

const UNIDADES_RELATIVAS = [
  ['year', 31536000],
  ['month', 2592000],
  ['week', 604800],
  ['day', 86400],
  ['hour', 3600],
  ['minute', 60],
];

/** «hace 5 minutos», «hace 2 días», «en 9 días». Cadena vacía si `iso` no es válido. */
export function fechaRelativa(iso) {
  if (!iso) return '';
  const fecha = new Date(iso);
  const ms = fecha.getTime();
  if (Number.isNaN(ms)) return '';
  const diffSeg = Math.round((ms - Date.now()) / 1000);
  for (const [unidad, segundos] of UNIDADES_RELATIVAS) {
    if (Math.abs(diffSeg) >= segundos) {
      return formatoRelativo(Math.round(diffSeg / segundos), unidad);
    }
  }
  return formatoRelativo(diffSeg, 'second');
}

/** «19/09/2026 20:14». Cadena vacía si `iso` no es válido. */
export function fechaAbsoluta(iso) {
  if (!iso) return '';
  const fecha = new Date(iso);
  if (Number.isNaN(fecha.getTime())) return '';
  return formatoFecha(fecha);
}

/** Formatea un tamaño en KiB como KB/MB/GB legibles. */
export function tamanoLegible(kb) {
  if (kb < 1024) return `${formatoNumero(kb)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${formatoNumero(mb)} MB`;
  return `${formatoNumero(mb / 1024)} GB`;
}

/** Días completos entre ahora y `iso` (negativo si ya pasó). */
export function diasHasta(iso) {
  if (!iso) return null;
  const fecha = new Date(iso).getTime();
  if (Number.isNaN(fecha)) return null;
  return Math.ceil((fecha - Date.now()) / 86400000);
}

/** Publica un aviso accesible (aria-live) en el contenedor `#avisos`. */
export function avisar(mensaje, tipo = 'info') {
  const contenedor = document.getElementById('avisos');
  if (!contenedor) return;
  const aviso = h('div', { clase: `banner show ${tipo}` }, h('span', null, mensaje));
  contenedor.replaceChildren(aviso);
}

export function limpiarAviso() {
  const contenedor = document.getElementById('avisos');
  if (contenedor) contenedor.replaceChildren();
}

/** Pone `boton` en estado «ocupado» (deshabilitado, con icono de carga y `texto`) y
 * devuelve la función que lo restaura. */
export function ocupar(boton, texto) {
  const anterior = [...boton.childNodes];
  boton.disabled = true;
  boton.setAttribute('aria-busy', 'true');
  boton.replaceChildren(h('span', { clase: 'spinner', 'aria-hidden': 'true' }), texto);
  return () => {
    boton.disabled = false;
    boton.removeAttribute('aria-busy');
    boton.replaceChildren(...anterior);
  };
}

/** Barra de progreso. Con `hechos`/`total` muestra el porcentaje; sin ellos, es
 * indeterminada (hay trabajo en curso, pero no se sabe cuánto falta). */
export function barraProgreso(texto, hechos, total) {
  const determinada = Number.isFinite(hechos) && total > 0;
  const porcentaje = determinada ? Math.round((hechos / total) * 100) : null;
  const relleno = h('span');
  if (determinada) relleno.style.width = `${porcentaje}%`;
  return h(
    'div',
    {
      clase: determinada ? 'progreso' : 'progreso indeterminado',
      role: 'progressbar',
      'aria-label': texto,
      'aria-valuemin': '0',
      'aria-valuemax': '100',
      ...(determinada ? { 'aria-valuenow': String(porcentaje) } : {}),
    },
    h('div', { clase: 'etiqueta' }, h('span', null, texto), determinada ? h('span', { clase: 'tabular' }, `${porcentaje} %`) : null),
    h('div', { clase: 'barra' }, relleno),
  );
}
