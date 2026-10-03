// Conmutador de tema claro/oscuro/automático, persistido en localStorage.
//
// mereba.css ya aplica el oscuro automático vía `prefers-color-scheme`. Para forzar un
// modo, ui/css/tema.css redefine los mismos tokens bajo `:root[data-tema="oscuro"]` y
// `:root[data-tema="claro"]`; aquí solo se pone/quita ese atributo.

import { t } from './i18n.js';

const CLAVE_LOCALSTORAGE = 'gitmereba.tema';
const ORDEN = ['auto', 'claro', 'oscuro'];
// Funciones y no textos: el idioma se fija al arrancar, después de cargar este módulo.
const ETIQUETAS = {
  auto: () => t('tema.auto'),
  claro: () => t('tema.claro'),
  oscuro: () => t('tema.oscuro'),
};

function leerPreferencia() {
  try {
    const valor = localStorage.getItem(CLAVE_LOCALSTORAGE);
    return ORDEN.includes(valor) ? valor : 'auto';
  } catch {
    return 'auto';
  }
}

function guardarPreferencia(valor) {
  try {
    localStorage.setItem(CLAVE_LOCALSTORAGE, valor);
  } catch {
    // Sin almacenamiento disponible (p. ej. ventana privada): el tema no persiste,
    // pero la app sigue funcionando con el valor en memoria de esta sesión.
  }
}

function aplicar(valor) {
  if (valor === 'auto') {
    document.documentElement.removeAttribute('data-tema');
  } else {
    document.documentElement.setAttribute('data-tema', valor);
  }
}

let actual = leerPreferencia();
aplicar(actual);

const botones = new Set();

function actualizarBotones() {
  for (const boton of botones) {
    const etiqueta = ETIQUETAS[actual]();
    boton.textContent = etiqueta;
    boton.setAttribute('aria-label', t('tema.cambiar', { etiqueta }));
  }
}

/** Crea (o registra) el botón conmutador de tema; puede haber varios en la página
 * (barra de navegación y vista de Ajustes) y todos quedan sincronizados. */
export function crearConmutadorTema() {
  const boton = document.createElement('button');
  boton.type = 'button';
  boton.className = 'btn ghost sm';
  boton.addEventListener('click', () => {
    const siguiente = ORDEN[(ORDEN.indexOf(actual) + 1) % ORDEN.length];
    actual = siguiente;
    aplicar(actual);
    guardarPreferencia(actual);
    actualizarBotones();
  });
  botones.add(boton);
  actualizarBotones();
  return boton;
}
