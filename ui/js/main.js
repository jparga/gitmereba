import { api } from './api.js';
import { crearConmutadorTema } from './tema.js';
import { iniciarRouter } from './router.js';

const ETIQUETAS_ESTADO_GITHUB = {
  operativo: 'GitHub operativo',
  degradado: 'GitHub degradado',
  caido: 'GitHub caído',
  desconocido: 'Estado de GitHub desconocido',
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
    texto.textContent = ETIQUETAS_ESTADO_GITHUB[estado] ?? ETIQUETAS_ESTADO_GITHUB.desconocido;
  } catch {
    punto.className = 'dot';
    texto.textContent = 'GitHub: sin datos';
  }
}

function iniciar() {
  document.getElementById('nav-tema-slot').append(crearConmutadorTema());

  const enlaces = document.querySelectorAll('.nav-links a[data-ruta]');
  iniciarRouter(document.getElementById('vista'), enlaces);

  actualizarEstadoGithub();
  setInterval(actualizarEstadoGithub, 60_000);
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', iniciar);
} else {
  iniciar();
}
