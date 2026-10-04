import { api } from './api.js';
import { crearConmutadorTema } from './tema.js';
import { iniciarRouter } from './router.js';
import { iniciarI18n, t } from './i18n.js';

// Funciones y no textos: el idioma se fija al arrancar, después de cargar este módulo.
const ETIQUETAS_ESTADO_GITHUB = {
  operativo: () => t('github.operativo'),
  degradado: () => t('github.degradado'),
  caido: () => t('github.caido'),
  desconocido: () => t('github.desconocido'),
};

const CLASES_ESTADO_GITHUB = {
  operativo: 'live',
  degradado: 'warn',
  caido: 'crit',
  desconocido: '',
};

async function actualizarEstadoGithub() {
  const punto = document.getElementById('estado-github-punto');
  const texto = document.getElementById('estado-github-texto');
  try {
    const { estado } = await api.estadoGithub();
    punto.className = `dot ${CLASES_ESTADO_GITHUB[estado] ?? ''}`;
    texto.textContent = (ETIQUETAS_ESTADO_GITHUB[estado] ?? ETIQUETAS_ESTADO_GITHUB.desconocido)();
  } catch {
    punto.className = 'dot';
    texto.textContent = t('github.sin_datos');
  }
}

/** Idioma resuelto por el backend; si no responde, inglés (el valor por defecto de la app). */
async function resolverIdioma() {
  try {
    const { idioma } = await api.idioma();
    return idioma;
  } catch {
    return 'en';
  }
}

/** Pone «gitmereba X.Y.Z ·» delante del enlace «Acerca de» del pie. Si el backend no
 * responde, el enlace se queda solo. */
async function pintarVersionPie() {
  try {
    const { version } = await api.acercaDe();
    document.getElementById('pie-version').textContent = t('pie.version', { version });
  } catch {
    // Sin versión: el enlace sigue llevando a «Acerca de», que muestra el error.
  }
}

async function iniciar() {
  iniciarI18n(await resolverIdioma());

  document.getElementById('nav-tema-slot').append(crearConmutadorTema());

  const enlaces = document.querySelectorAll('.nav-links a[data-ruta]');
  iniciarRouter(document.getElementById('vista'), enlaces);

  pintarVersionPie();
  actualizarEstadoGithub();
  setInterval(actualizarEstadoGithub, 60_000);
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', iniciar);
} else {
  iniciar();
}
