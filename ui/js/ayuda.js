// Ayuda contextual: un desplegable «¿Cómo funciona esta pantalla?» bajo el título de cada
// vista. Los textos viven en ui/i18n/ayuda.es.js y ayuda.en.js, juntos, para que sea fácil
// mantenerlos al día con los manuales. F1 lo abre o lo cierra.
// Los dos módulos se importan de forma estática: la CSP no permite `fetch` ni scripts inline.

import { h } from './dom.js';
import { idioma, t } from './i18n.js';
import ayudaEs from '../i18n/ayuda.es.js';
import ayudaEn from '../i18n/ayuda.en.js';

const AYUDA = { es: ayudaEs, en: ayudaEn };

function crearAyuda(ruta) {
  const datos = (AYUDA[idioma()] ?? AYUDA.es)[ruta];
  if (!datos) return null;
  return h(
    'details',
    { clase: 'disclosure ayuda', 'data-ayuda': ruta },
    h('summary', null, t('ayuda.titulo')),
    h(
      'div',
      null,
      h('p', null, datos.intro),
      h('ul', null, datos.puntos.map((punto) => h('li', null, punto))),
      h('p', { clase: 'hint' }, t('ayuda.pie')),
    ),
  );
}

/** Coloca la ayuda de `ruta` bajo el primer título de `contenedor`, si aún no está. */
function colocarAyuda(contenedor, ruta) {
  if (contenedor.querySelector(':scope > .ayuda, :scope .ayuda[data-ayuda]')) return;
  const titulo = contenedor.querySelector('h1');
  const ayuda = crearAyuda(ruta);
  if (!titulo || !ayuda) return;
  // Si el título va dentro de una barra (p. ej. junto a un botón), la ayuda se coloca
  // bajo esa barra, no dentro de ella.
  let ancla = titulo;
  while (ancla.parentElement && ancla.parentElement !== contenedor) ancla = ancla.parentElement;
  ancla.after(ayuda);
}

/**
 * Mantiene la ayuda de la ruta activa en `contenedor`, también cuando una vista se repinta
 * por su cuenta. Devuelve la función que el router llama en cada cambio de ruta.
 */
export function iniciarAyuda(contenedor) {
  let rutaActual = null;
  new MutationObserver(() => {
    if (rutaActual) colocarAyuda(contenedor, rutaActual);
  }).observe(contenedor, { childList: true, subtree: true });

  globalThis.addEventListener('keydown', (evento) => {
    if (evento.key !== 'F1') return;
    const ayuda = contenedor.querySelector('.ayuda');
    if (!ayuda) return;
    evento.preventDefault();
    ayuda.open = !ayuda.open;
    if (ayuda.open) ayuda.scrollIntoView({ block: 'nearest' });
  });

  return (ruta) => {
    rutaActual = ruta;
    colocarAyuda(contenedor, ruta);
  };
}
