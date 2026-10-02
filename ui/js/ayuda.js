// Ayuda contextual: un desplegable «¿Cómo funciona esta pantalla?» bajo el título de cada
// vista. Los textos viven aquí, juntos, para que sea fácil mantenerlos al día con
// docs/manual.md. F1 lo abre o lo cierra.

import { h } from './dom.js';

const AYUDA = {
  resumen: {
    intro: 'De un vistazo: cómo está cada cuenta clonada y si GitHub funciona.',
    puntos: [
      'Cada tarjeta es una cuenta de GitHub. El color indica su peor repositorio: verde, todo al día; ámbar, algo lleva tiempo sin sincronizar; rojo, algo falla.',
      'Los contadores separan los repositorios al día, los que necesitan atención, los que ya no existen en GitHub (se conservan aquí) y los que están en contingencia.',
      '«Sincronizar ahora» fuerza una pasada sin esperar al temporizador. Mientras dura verás una barra; después, «Clonando repositorios: X de N» indica cuántos ha terminado de traer Gitea.',
      '«Abrir Gitea» abre en el navegador el servidor local de esa cuenta. Te pedirá usuario y contraseña: están en «Usuario y contraseña».',
      'Si el token de GitHub va a caducar, aparece un aviso: renuévalo en Ajustes → Rotar token.',
      'El indicador de arriba a la derecha muestra el estado de GitHub. Si está caído, ve a Contingencia.',
    ],
  },
  repositorios: {
    intro: 'La lista completa de repositorios de una cuenta y su estado.',
    puntos: [
      'Filtra por estado o busca por nombre. Los problemas aparecen primero.',
      '«En GitHub» indica si el repositorio es público o privado allí. Tu copia local es siempre privada: en Gitea nadie ve nada sin iniciar sesión.',
      'Los que pone «no se clona» existen en GitHub pero no tienen copia: márcalos para incluirlos. Si son forks, activa antes «Incluir forks» en Ajustes.',
      'Desmarca «Incluido» para dejar de clonar un repositorio: su copia se queda en pausa, no se borra.',
      '«Sincronizar» actualiza solo ese repositorio; si su primer clonado se cortó («no ha sincronizado nunca»), lo vuelve a clonar. «Abrir» lo muestra en el Gitea local.',
      '«Huérfano» significa que ya no está en GitHub. gitmereba nunca borra un huérfano: decide tú qué hacer con él.',
      'La dirección de clonado es la de tu copia local; sirve aunque GitHub no responda.',
    ],
  },
  contingencia: {
    intro: 'Para seguir trabajando cuando GitHub no responde y devolver después los cambios.',
    puntos: [
      '«Activar» crea una copia con escritura del repositorio en tu Gitea local y pausa su sincronización. El original no se toca.',
      'Copia el comando «git remote add mereba …» en tu carpeta de trabajo y envía tus cambios con «git push mereba».',
      'Cuando GitHub vuelva, pulsa «Reconciliar» e introduce un token con permiso de escritura. Solo se usa para ese envío y no se guarda.',
      'Nunca se fuerza nada: si GitHub recibió cambios distintos mientras tanto, la reconciliación se detiene y te avisa para que lo resuelvas tú con git.',
      'Tras reconciliar, el repositorio vuelve a sincronizarse con normalidad.',
    ],
  },
  actividad: {
    intro: 'Qué ha hecho gitmereba y cuándo.',
    puntos: [
      '«Sincronizaciones» lista cada pasada: cuántos repositorios se crearon, cuántos fallaron y un resumen.',
      '«Auditoría» registra cada acción importante (altas, bajas, exclusiones, contingencias, cambios de ajustes). Solo se añade: no se puede editar ni borrar.',
      'Cada entrada va encadenada a la anterior con una huella. «Cadena íntegra» confirma que nadie ha alterado el registro.',
    ],
  },
  ajustes: {
    intro: 'Configuración de cada cuenta y de la aplicación.',
    puntos: [
      'Intervalo: cada cuánto se sincroniza la cuenta (mínimo 10 minutos). Funciona aunque esta ventana esté cerrada.',
      'Forks y organizaciones: qué se clona además de los repositorios propios. Tras activar «Incluir forks», marca en Repositorios los que quieras y sincroniza.',
      '«Rotar token» sustituye el token de GitHub. Se guarda en el llavero del sistema, nunca en ficheros.',
      '«Acceso desde la red local» comparte el Gitea de la cuenta con otros equipos de tu red, siempre por HTTPS. Está apagado por defecto; sigue los tres pasos que muestra al activarlo.',
      '«Usuarios de la LAN» crea personas que entran con su propio usuario en vez del administrador: leen todos los mirrors y solo pueden escribir en los repositorios de contingencia. La contraseña la genera Gitea y se muestra una única vez al crear el usuario: cópiala entonces, porque gitmereba no la guarda. Si alguien la pierde, elimina su usuario y créalo de nuevo.',
      '«Dar de baja» detiene la sincronización, pero no borra los repositorios ya clonados.',
      '«Actualizar» descarga la versión de Gitea fijada por gitmereba y comprueba su firma antes de instalarla.',
    ],
  },
  alta: {
    intro: 'Tres pasos para clonar una cuenta de GitHub.',
    puntos: [
      'Usuario: el nombre de la cuenta de GitHub que quieres clonar.',
      'Token: créalo en GitHub → Settings → Developer settings → Fine-grained tokens, con acceso de solo lectura a «Contents» y «Metadata». Se guarda en el llavero del sistema.',
      'Carpeta: debe estar vacía o no existir. Ahí vivirá todo lo de la cuenta (repositorios, Gitea y copias de seguridad); puedes moverla o respaldarla entera.',
      'Antes de crear nada verás qué repositorios se van a clonar y cuánto ocupan; desmarca los que no quieras.',
      'Si algo falla a mitad, repite el alta: continúa donde se quedó.',
    ],
  },
};

function crearAyuda(ruta) {
  const datos = AYUDA[ruta];
  if (!datos) return null;
  return h(
    'details',
    { clase: 'disclosure ayuda', 'data-ayuda': ruta },
    h('summary', null, '¿Cómo funciona esta pantalla?'),
    h(
      'div',
      null,
      h('p', null, datos.intro),
      h('ul', null, datos.puntos.map((punto) => h('li', null, punto))),
      h('p', { clase: 'hint' }, 'Pulsa F1 para abrir o cerrar esta ayuda. El manual completo está en docs/manual.md.'),
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
