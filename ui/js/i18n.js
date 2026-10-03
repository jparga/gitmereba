// Idiomas de la ventana. Los diccionarios son módulos estáticos (ui/i18n/es.js y en.js):
// sin `fetch` ni scripts inline, así que funcionan igual con la CSP de index.html.
//
// `t()` devuelve siempre texto plano: se asigna con `textContent` o se pasa a `h()`, nunca
// se interpreta como HTML. Una clave ausente devuelve la propia clave: se ve en pantalla y
// el test de app/tests/i18n_ui.rs la caza antes.

import es from '../i18n/es.js';
import en from '../i18n/en.js';

const DICCIONARIOS = { es, en };
// `en-GB` y no `en-US`: fechas día-mes y 24 horas, lo más cercano al formato español.
const LOCALES = { es: 'es-ES', en: 'en-GB' };
const IDIOMA_POR_DEFECTO = 'es';

const OPCIONES_FECHA = {
  day: '2-digit',
  month: '2-digit',
  year: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
};

let codigoActual = IDIOMA_POR_DEFECTO;
let diccionario = DICCIONARIOS[IDIOMA_POR_DEFECTO];
let reglasPlural;
let formatoRelativoIntl;
let formatoFechaIntl;
let formatoNumeroIntl;

function configurar(codigo) {
  codigoActual = DICCIONARIOS[codigo] ? codigo : IDIOMA_POR_DEFECTO;
  diccionario = DICCIONARIOS[codigoActual];
  const locale = LOCALES[codigoActual];
  reglasPlural = new Intl.PluralRules(locale);
  formatoRelativoIntl = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  formatoFechaIntl = new Intl.DateTimeFormat(locale, OPCIONES_FECHA);
  formatoNumeroIntl = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 });
}

configurar(IDIOMA_POR_DEFECTO);

/** Idioma activo: `'es'` o `'en'`. */
export function idioma() {
  return codigoActual;
}

/**
 * Texto de `clave` en el idioma activo, con `{nombre}` sustituido por `params.nombre`.
 * Si `params.n` es un número, elige la variante plural `clave_one`/`clave_other` según
 * las reglas del idioma (y cae a `clave` si el diccionario no tiene variantes).
 */
export function t(clave, params) {
  let texto;
  if (typeof params?.n === 'number') {
    texto = diccionario[`${clave}_${reglasPlural.select(params.n)}`] ?? diccionario[`${clave}_other`];
  }
  texto ??= diccionario[clave];
  if (typeof texto !== 'string') return clave;
  if (!params) return texto;
  return texto.replace(/\{(\w+)\}/g, (hueco, nombre) => (nombre in params ? String(params[nombre]) : hueco));
}

/** Fija el idioma (`'es'` | `'en'`), `<html lang>` y los textos estáticos de la página. */
export function iniciarI18n(codigo) {
  configurar(codigo);
  document.documentElement.lang = codigoActual;
  aplicarTextosEstaticos(document);
}

/** Aplica `data-i18n`, `data-i18n-title` y `data-i18n-aria-label` bajo `raiz`. */
export function aplicarTextosEstaticos(raiz) {
  for (const el of raiz.querySelectorAll('[data-i18n]')) {
    el.textContent = t(el.dataset.i18n);
  }
  for (const [atributo, dataset] of [
    ['title', 'i18nTitle'],
    ['aria-label', 'i18nAriaLabel'],
  ]) {
    for (const el of raiz.querySelectorAll(`[data-i18n-${atributo}]`)) {
      el.setAttribute(atributo, t(el.dataset[dataset]));
    }
  }
}

/** «hace 5 minutos» / «5 minutes ago»: `valor` negativo es pasado. */
export function formatoRelativo(valor, unidad) {
  return formatoRelativoIntl.format(valor, unidad);
}

/** Fecha y hora cortas en el locale activo. `fecha` es un `Date` válido. */
export function formatoFecha(fecha) {
  return formatoFechaIntl.format(fecha);
}

/** Número con hasta un decimal en el locale activo. */
export function formatoNumero(numero) {
  return formatoNumeroIntl.format(numero);
}
