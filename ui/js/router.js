// Navegación por hash entre vistas. Cada módulo de ui/js/vistas/ exporta
// `render(contenedor, parametros)`.

import { iniciarAyuda } from './ayuda.js';

const RUTA_POR_DEFECTO = 'resumen';

const VISTAS = {
  resumen: () => import('./vistas/resumen.js'),
  repositorios: () => import('./vistas/repositorios.js'),
  contingencia: () => import('./vistas/contingencia.js'),
  actividad: () => import('./vistas/actividad.js'),
  ajustes: () => import('./vistas/ajustes.js'),
  alta: () => import('./vistas/alta.js'),
  acerca: () => import('./vistas/acerca.js'),
};

function leerRuta() {
  const hash = globalThis.location.hash.replace(/^#\/?/, '');
  const [ruta, resto] = hash.split('?');
  return { ruta: VISTAS[ruta] ? ruta : RUTA_POR_DEFECTO, parametros: new URLSearchParams(resto ?? '') };
}

/**
 * @param {HTMLElement} contenedor Elemento donde se pinta la vista activa.
 * @param {NodeListOf<HTMLAnchorElement>} enlaces Enlaces de navegación con `data-ruta`.
 */
export function iniciarRouter(contenedor, enlaces) {
  let generacion = 0;
  let limpiarVistaAnterior = null;
  const mostrarAyuda = iniciarAyuda(contenedor);

  async function pintarRuta() {
    const miGeneracion = ++generacion;
    const { ruta, parametros } = leerRuta();

    for (const enlace of enlaces) {
      if (enlace.dataset.ruta === ruta) enlace.setAttribute('aria-current', 'page');
      else enlace.removeAttribute('aria-current');
    }

    const modulo = await VISTAS[ruta]();
    // Si mientras cargaba el módulo el usuario navegó a otra ruta, no pintamos una
    // vista obsoleta encima de la nueva.
    if (miGeneracion !== generacion) return;
    if (typeof limpiarVistaAnterior === 'function') limpiarVistaAnterior();
    contenedor.replaceChildren();
    // Una vista puede devolver una función de limpieza (p. ej. para cancelar
    // suscripciones a eventos de progreso) que se invoca al salir de ella.
    limpiarVistaAnterior = (await modulo.render(contenedor, parametros)) ?? null;
    if (miGeneracion === generacion) mostrarAyuda(ruta);
  }

  globalThis.addEventListener('hashchange', pintarRuta);
  pintarRuta();
}

/** Navega a otra vista desde código (p. ej. tras terminar el asistente de alta). */
export function navegar(ruta, parametros) {
  const busqueda = parametros ? `?${new URLSearchParams(parametros)}` : '';
  globalThis.location.hash = `#/${ruta}${busqueda}`;
}
