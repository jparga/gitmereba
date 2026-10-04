// «Acerca de»: versión y enlaces del proyecto, versión de Gitea (la incluida y la de cada
// cuenta) y entorno. «Copiar para informe» copia todo en texto plano para una incidencia;
// no lleva secretos y las rutas ya llegan con `~` en lugar del directorio personal.

import { api } from '../api.js';
import { h, pintar, avisar } from '../dom.js';
import { t } from '../i18n.js';

// Claves y no textos: el idioma se fija al arrancar, después de cargar este módulo.
const ETIQUETAS_VERSION = {
  al_dia: 'acerca.gitea.al_dia',
  desfasada: 'acerca.gitea.desfasada',
  desconocida: 'acerca.gitea.desconocida',
  no_disponible: 'acerca.gitea.no_disponible',
};
const CLASES_VERSION = { al_dia: 'ok', desfasada: 'warn', desconocida: 'muted', no_disponible: 'muted' };

const ENLACES = [
  ['repositorio', 'acerca.enlace.repositorio'],
  ['release', 'acerca.enlace.release'],
  ['seguridad', 'acerca.enlace.seguridad'],
  ['marcas', 'acerca.enlace.marcas'],
];

/** Texto de la versión de una cuenta: la versión y su estado, o «…» mientras se consulta. */
function textoVersion(resultado) {
  if (!resultado) return t('acerca.gitea.consultando');
  const estado = t(ETIQUETAS_VERSION[resultado.estado] ?? ETIQUETAS_VERSION.desconocida);
  return resultado.version ? `${resultado.version} (${estado})` : estado;
}

function celdaVersion(resultado) {
  if (!resultado) return h('span', { clase: 'hint' }, t('acerca.gitea.consultando'));
  const etiqueta = t(ETIQUETAS_VERSION[resultado.estado] ?? ETIQUETAS_VERSION.desconocida);
  return h(
    'span',
    { clase: 'row' },
    resultado.version ? h('span', { clase: 'mono' }, resultado.version) : null,
    h('span', { clase: `badge ${CLASES_VERSION[resultado.estado] ?? 'muted'}` }, etiqueta),
  );
}

/** Pares [etiqueta, valor] del bloque de entorno, en el orden en que se muestran y copian. */
function datosEntorno(datos) {
  const desconocido = t('acerca.desconocido');
  return [
    [t('acerca.entorno.datos'), datos.rutas.datos],
    [t('acerca.entorno.config'), datos.rutas.config],
    [t('acerca.entorno.systemd'), datos.rutas.systemd],
    [t('acerca.entorno.ejecutable'), datos.ejecutable ?? desconocido],
    [t('acerca.entorno.idioma'), datos.idioma],
    [t('acerca.entorno.sistema'), datos.sistema ?? desconocido],
  ];
}

function listaDefinicion(pares) {
  return h(
    'dl',
    { clase: 'acerca-datos' },
    pares.flatMap(([clave, valor]) => [h('dt', null, clave), h('dd', { clase: 'mono' }, valor)]),
  );
}

/** Informe en texto plano con lo que muestra la pantalla en este momento. */
function componerInforme(datos, versiones) {
  const lineas = [
    `${t('acerca.nombre')} ${datos.version} (${datos.licencia})`,
    `${t('acerca.gitea.incluida')}: ${datos.version_gitea_incluida}`,
  ];
  for (const cuenta of datos.cuentas) {
    lineas.push(
      `${t('acerca.gitea.cuenta_linea', { login: cuenta.login, carpeta: cuenta.carpeta })}: ${textoVersion(versiones.get(cuenta.login))}`,
    );
  }
  for (const [clave, valor] of datosEntorno(datos)) lineas.push(`${clave}: ${valor}`);
  return lineas.join('\n');
}

async function copiarInforme(datos, versiones) {
  try {
    await navigator.clipboard.writeText(componerInforme(datos, versiones));
    avisar(t('acerca.copiado'), 'success');
  } catch {
    avisar(t('acerca.no_copiado'), 'warning');
  }
}

async function abrirEnlace(destino) {
  try {
    await api.abrirEnlace(destino);
  } catch (error) {
    avisar(t('acerca.enlace.error', { mensaje: error?.mensaje ?? error }), 'error');
  }
}

export async function render(contenedor) {
  const datos = await api.acercaDe();
  const versiones = new Map();

  const tarjetaVersion = h(
    'div',
    { clase: 'card pad section' },
    h('h2', null, t('acerca.version.titulo')),
    h('p', { clase: 'acerca-version' }, h('span', { clase: 'wordmark' }, t('acerca.nombre')), ' ', h('span', { clase: 'mono' }, datos.version)),
    h('p', null, t('acerca.licencia', { licencia: datos.licencia })),
    h('p', { clase: 'hint' }, t('acerca.marcas')),
    h(
      'div',
      { clase: 'row' },
      ENLACES.map(([destino, clave]) =>
        h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => abrirEnlace(destino) }, t(clave)),
      ),
    ),
  );

  const celdas = new Map();
  const tablaCuentas = datos.cuentas.length
    ? h(
        'div',
        { clase: 'table-wrap' },
        h(
          'table',
          null,
          h(
            'thead',
            null,
            h(
              'tr',
              null,
              h('th', { scope: 'col' }, t('acerca.gitea.col.cuenta')),
              h('th', { scope: 'col' }, t('acerca.gitea.col.carpeta')),
              h('th', { scope: 'col' }, t('acerca.gitea.col.version')),
            ),
          ),
          h(
            'tbody',
            null,
            datos.cuentas.map((cuenta) => {
              const celda = h('td', null, celdaVersion(null));
              celdas.set(cuenta.login, celda);
              return h('tr', null, h('td', { clase: 'mono' }, cuenta.login), h('td', { clase: 'mono small' }, cuenta.carpeta), celda);
            }),
          ),
        ),
      )
    : h('p', { clase: 'hint' }, t('acerca.sin_cuentas'));

  const tarjetaGitea = h(
    'div',
    { clase: 'card pad section' },
    h('h2', null, t('acerca.gitea.titulo')),
    h('p', null, `${t('acerca.gitea.incluida')}: `, h('span', { clase: 'mono' }, datos.version_gitea_incluida)),
    tablaCuentas,
  );

  const tarjetaEntorno = h('div', { clase: 'card pad section' }, h('h2', null, t('acerca.entorno.titulo')), listaDefinicion(datosEntorno(datos)));

  pintar(
    contenedor,
    h('h1', null, t('acerca.titulo')),
    h('p', { clase: 'lead' }, t('acerca.lead')),
    tarjetaVersion,
    tarjetaGitea,
    tarjetaEntorno,
    h(
      'div',
      { clase: 'row' },
      h('button', { type: 'button', clase: 'btn', onClick: () => copiarInforme(datos, versiones) }, t('acerca.copiar')),
    ),
  );

  // Cada cuenta se consulta por separado: una instancia parada no retrasa a las demás.
  for (const cuenta of datos.cuentas) {
    api
      .versionGiteaCuenta(cuenta.login)
      .catch(() => ({ version: null, estado: 'no_disponible' }))
      .then((resultado) => {
        versiones.set(cuenta.login, resultado);
        celdas.get(cuenta.login)?.replaceChildren(celdaVersion(resultado));
      });
  }
}
