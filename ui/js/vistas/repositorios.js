import { api } from '../api.js';
import { t } from '../i18n.js';
import { h, pintar, badgeEstado, textoEstado, fechaRelativa, tamanoLegible, ORDEN_ESTADOS, avisar, ocupar } from '../dom.js';

// Claves de texto y no textos: el idioma se fija al arrancar, después de cargar este módulo.
const COLUMNAS = [
  { clave: 'estado', etiqueta: 'repos.col.estado' },
  { clave: 'repo', etiqueta: 'repos.col.repo' },
  { clave: 'visibilidad', etiqueta: 'repos.col.visibilidad', titulo: 'repos.col.visibilidad.ayuda' },
  { clave: 'fork', etiqueta: 'repos.col.fork' },
  { clave: 'ultima_sync', etiqueta: 'repos.col.ultima_sync' },
  { clave: 'tamano', etiqueta: 'repos.col.tamano', num: true },
];

function valorOrden(repo, clave) {
  switch (clave) {
    case 'estado':
      return ORDEN_ESTADOS.indexOf(repo.estado);
    case 'repo':
      return `${repo.id.dueno}/${repo.id.nombre}`.toLowerCase();
    case 'visibilidad':
      return repo.privado ? 1 : 0;
    case 'fork':
      return repo.es_fork ? 1 : 0;
    case 'ultima_sync':
      return repo.ultima_sync ? new Date(repo.ultima_sync).getTime() : -Infinity;
    case 'tamano':
      return repo.tamano_kb;
    default:
      return 0;
  }
}

function ordenar(repos, clave, direccion) {
  const factor = direccion === 'desc' ? -1 : 1;
  return [...repos].sort((a, b) => {
    const va = valorOrden(a, clave);
    const vb = valorOrden(b, clave);
    if (va < vb) return -1 * factor;
    if (va > vb) return 1 * factor;
    return `${a.id.dueno}/${a.id.nombre}`.localeCompare(`${b.id.dueno}/${b.id.nombre}`);
  });
}

async function copiarUrl(url) {
  try {
    await navigator.clipboard.writeText(url);
    avisar(t('repos.url.copiada', { url }), 'success');
  } catch {
    avisar(t('repos.url.no_copiada', { url }), 'warning');
  }
}

const avisoFork = () => t('repos.aviso_fork');

function celdaUltimaSync(repo) {
  if (repo.omitido) {
    return h('span', { clase: 'muted', title: repo.omitido === 'fork' ? avisoFork() : null }, t('repos.no_se_clona'));
  }
  if (repo.clonado === false) {
    return h('span', null, h('span', { clase: 'spinner', 'aria-hidden': 'true' }), t('repos.clonando'));
  }
  return repo.ultima_sync ? fechaRelativa(repo.ultima_sync) : t('repos.sin_datos');
}

function celdaAcciones(login, repo, onCambio) {
  return h(
    'td',
    { clase: 'acciones-fila' },
    h(
      'button',
      {
        clase: 'btn ghost sm',
        onClick: async (evento) => {
          // `currentTarget` deja de existir tras el primer `await`: se guarda antes.
          const liberar = ocupar(evento.currentTarget, t('repos.sincronizando'));
          try {
            await api.sincronizar(login, repo.id);
            avisar(t('repos.sync.hecha', { repo: `${repo.id.dueno}/${repo.id.nombre}` }), 'success');
            await onCambio();
          } catch (error) {
            avisar(t('repos.sync.error', { mensaje: error?.mensaje ?? error }), 'error');
          } finally {
            liberar();
          }
        },
      },
      t('repos.sincronizar'),
    ),
    h(
      'button',
      {
        clase: 'btn ghost sm',
        onClick: async () => {
          try {
            await api.abrirGitea(login);
          } catch (error) {
            avisar(t('repos.abrir.error', { mensaje: error?.mensaje ?? error }), 'error');
          }
        },
      },
      t('repos.abrir'),
    ),
    h('button', { clase: 'btn ghost sm', onClick: () => copiarUrl(repo.url_clon_local) }, t('repos.copiar_url')),
  );
}

function filaRepo(login, repo, onCambio) {
  // Existe en GitHub pero no tiene copia local: ni se sincroniza ni se puede clonar de aquí.
  const sinCopia = Boolean(repo.omitido);
  const casilla = h('input', {
    type: 'checkbox',
    checked: repo.incluido,
    disabled: repo.omitido === 'fork',
    title: repo.omitido === 'fork' ? avisoFork() : null,
    'aria-label': t('repos.incluir.etiqueta', { repo: `${repo.id.dueno}/${repo.id.nombre}` }),
    onChange: async (evento) => {
      // `currentTarget` deja de existir tras el primer `await`: se guarda antes.
      const casilla = evento.currentTarget;
      const incluir = casilla.checked;
      casilla.disabled = true;
      try {
        await api.excluirRepo(login, repo.id, !incluir);
        if (sinCopia && incluir) {
          avisar(t('repos.incluir.clonara', { repo: `${repo.id.dueno}/${repo.id.nombre}` }), 'success');
        }
        await onCambio();
      } catch (error) {
        avisar(t('repos.incluir.error', { mensaje: error?.mensaje ?? error }), 'error');
        casilla.checked = !incluir;
        casilla.disabled = false;
      }
    },
  });

  return h(
    'tr',
    null,
    h('td', null, casilla),
    h('td', null, badgeEstado(repo.estado)),
    h(
      'td',
      { clase: 'mono' },
      `${repo.id.dueno}/${repo.id.nombre}`,
      repo.archivado ? h('span', { clase: 'badge muted etiqueta-repo' }, t('repos.archivado')) : null,
    ),
    h('td', null, repo.privado ? t('repos.privado') : t('repos.publico')),
    h('td', null, repo.es_fork ? t('repos.si') : t('repos.no')),
    h('td', null, celdaUltimaSync(repo)),
    h('td', { clase: 'num' }, sinCopia ? '—' : tamanoLegible(repo.tamano_kb)),
    sinCopia
      ? h('td', { clase: 'muted' }, repo.omitido === 'fork' ? t('repos.fork_no_incluido') : t('repos.excluido'))
      : celdaAcciones(login, repo, onCambio),
  );
}

export async function render(contenedor) {
  const cuentas = await api.listarCuentas();
  if (cuentas.length === 0) {
    pintar(
      contenedor,
      h(
        'div',
        { clase: 'card pad vacio' },
        h('h2', null, t('repos.vacio.titulo')),
        h('p', null, t('repos.vacio.texto')),
      ),
    );
    return;
  }

  const estado = {
    login: cuentas[0].login,
    texto: '',
    filtroEstado: '',
    orden: 'estado',
    direccion: 'asc',
    repos: [],
  };

  async function cargarRepos() {
    estado.repos = await api.listarRepos(estado.login);
    pintarTabla();
  }

  function repite() {
    let repos = estado.repos;
    if (estado.filtroEstado) repos = repos.filter((r) => r.estado === estado.filtroEstado);
    if (estado.texto.trim()) {
      const q = estado.texto.trim().toLowerCase();
      repos = repos.filter((r) => `${r.id.dueno}/${r.id.nombre}`.toLowerCase().includes(q));
    }
    return ordenar(repos, estado.orden, estado.direccion);
  }

  function th(columna) {
    const activa = estado.orden === columna.clave;
    const flecha = activa ? (estado.direccion === 'asc' ? '▲' : '▼') : '';
    return h(
      'th',
      { scope: 'col', ...(activa ? { 'aria-sort': estado.direccion === 'asc' ? 'ascending' : 'descending' } : {}), ...(columna.num ? { clase: 'num' } : {}) },
      h(
        'button',
        {
          clase: 'th-orden',
          title: columna.titulo ? t(columna.titulo) : null,
          onClick: () => {
            if (estado.orden === columna.clave) {
              estado.direccion = estado.direccion === 'asc' ? 'desc' : 'asc';
            } else {
              estado.orden = columna.clave;
              estado.direccion = 'asc';
            }
            pintarTabla();
          },
        },
        t(columna.etiqueta),
        flecha ? h('span', { clase: 'flecha' }, flecha) : null,
      ),
    );
  }

  let cuerpoTabla;
  let contenedorTabla;

  function pintarTabla() {
    const filas = repite();
    if (filas.length === 0) {
      pintar(cuerpoTabla, h('tr', null, h('td', { colspan: '8', clase: 'center muted' }, t('repos.sin_coincidencias'))));
      return;
    }
    pintar(cuerpoTabla, ...filas.map((repo) => filaRepo(estado.login, repo, cargarRepos)));
  }

  function refrescarCabecera() {
    pintar(
      contenedorTabla.querySelector('thead tr'),
      h('th', { scope: 'col' }, t('repos.col.incluido')),
      ...COLUMNAS.map(th),
      h('th', { scope: 'col' }, t('repos.col.acciones')),
    );
  }

  const selectorCuenta = h(
    'select',
    {
      id: 'sel-cuenta',
      onChange: async (evento) => {
        estado.login = evento.currentTarget.value;
        await cargarRepos();
      },
    },
    cuentas.map((cuenta) => h('option', { value: cuenta.login }, cuenta.login)),
  );

  const filtroTexto = h('input', {
    id: 'sel-filtro-texto',
    type: 'search',
    placeholder: t('repos.filtro.placeholder'),
    onInput: (evento) => {
      estado.texto = evento.currentTarget.value;
      pintarTabla();
    },
  });

  const filtroEstado = h(
    'select',
    {
      id: 'sel-filtro-estado',
      onChange: (evento) => {
        estado.filtroEstado = evento.currentTarget.value;
        pintarTabla();
      },
    },
    h('option', { value: '' }, t('repos.filtro.todos')),
    ORDEN_ESTADOS.map((e) => h('option', { value: e }, textoEstado(e))),
  );

  cuerpoTabla = h('tbody', null);
  contenedorTabla = h(
    'div',
    { clase: 'table-wrap' },
    h('table', null, h('thead', null, h('tr', null)), cuerpoTabla),
  );

  pintar(
    contenedor,
    h('h1', null, t('repos.titulo')),
    h(
      'div',
      { clase: 'toolbar mb' },
      h(
        'div',
        { clase: 'controls' },
        h('div', { clase: 'field' }, h('label', { for: 'sel-cuenta' }, t('repos.campo.cuenta')), selectorCuenta),
        h('div', { clase: 'field' }, h('label', { for: 'sel-filtro-texto' }, t('repos.campo.buscar')), filtroTexto),
        h('div', { clase: 'field' }, h('label', { for: 'sel-filtro-estado' }, t('repos.campo.estado')), filtroEstado),
      ),
    ),
    contenedorTabla,
  );

  refrescarCabecera();
  await cargarRepos();
}
