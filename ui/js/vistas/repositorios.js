import { api } from '../api.js';
import { h, pintar, badgeEstado, textoEstado, fechaRelativa, tamanoLegible, ORDEN_ESTADOS, avisar, ocupar } from '../dom.js';

const COLUMNAS = [
  { clave: 'estado', etiqueta: 'Estado' },
  { clave: 'repo', etiqueta: 'Repositorio' },
  { clave: 'visibilidad', etiqueta: 'En GitHub', titulo: 'Visibilidad del repositorio en GitHub. La copia local es siempre privada.' },
  { clave: 'fork', etiqueta: 'Fork' },
  { clave: 'ultima_sync', etiqueta: 'Última sync' },
  { clave: 'tamano', etiqueta: 'Tamaño', num: true },
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
    avisar(`URL de clonado copiada: ${url}`, 'success');
  } catch {
    avisar(`No se pudo copiar automáticamente. URL: ${url}`, 'warning');
  }
}

const AVISO_FORK = 'Es un fork y la cuenta no incluye forks: activa «Incluir forks» en Ajustes.';

function celdaUltimaSync(repo) {
  if (repo.omitido) {
    return h('span', { clase: 'muted', title: repo.omitido === 'fork' ? AVISO_FORK : null }, 'no se clona');
  }
  if (repo.clonado === false) {
    return h('span', null, h('span', { clase: 'spinner', 'aria-hidden': 'true' }), 'clonando…');
  }
  return repo.ultima_sync ? fechaRelativa(repo.ultima_sync) : 'sin datos';
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
          const liberar = ocupar(evento.currentTarget, 'Sincronizando…');
          try {
            await api.sincronizar(login, repo.id);
            avisar(`Repositorio ${repo.id.dueno}/${repo.id.nombre} sincronizado.`, 'success');
            await onCambio();
          } catch (error) {
            avisar(`Fallo al sincronizar: ${error?.mensaje ?? error}`, 'error');
          } finally {
            liberar();
          }
        },
      },
      'Sincronizar',
    ),
    h(
      'button',
      {
        clase: 'btn ghost sm',
        onClick: async () => {
          try {
            await api.abrirGitea(login);
          } catch (error) {
            avisar(`No se pudo abrir Gitea: ${error?.mensaje ?? error}`, 'error');
          }
        },
      },
      'Abrir',
    ),
    h('button', { clase: 'btn ghost sm', onClick: () => copiarUrl(repo.url_clon_local) }, 'Copiar URL'),
  );
}

function filaRepo(login, repo, onCambio) {
  // Existe en GitHub pero no tiene copia local: ni se sincroniza ni se puede clonar de aquí.
  const sinCopia = Boolean(repo.omitido);
  const casilla = h('input', {
    type: 'checkbox',
    checked: repo.incluido,
    disabled: repo.omitido === 'fork',
    title: repo.omitido === 'fork' ? AVISO_FORK : null,
    'aria-label': `Incluir ${repo.id.dueno}/${repo.id.nombre} en la sincronización`,
    onChange: async (evento) => {
      // `currentTarget` deja de existir tras el primer `await`: se guarda antes.
      const casilla = evento.currentTarget;
      const incluir = casilla.checked;
      casilla.disabled = true;
      try {
        await api.excluirRepo(login, repo.id, !incluir);
        if (sinCopia && incluir) {
          avisar(`${repo.id.dueno}/${repo.id.nombre} se clonará en la próxima sincronización.`, 'success');
        }
        await onCambio();
      } catch (error) {
        avisar(`No se pudo cambiar la inclusión: ${error?.mensaje ?? error}`, 'error');
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
      repo.archivado ? h('span', { clase: 'badge muted etiqueta-repo' }, 'Archivado') : null,
    ),
    h('td', null, repo.privado ? 'Privado' : 'Público'),
    h('td', null, repo.es_fork ? 'Sí' : 'No'),
    h('td', null, celdaUltimaSync(repo)),
    h('td', { clase: 'num' }, sinCopia ? '—' : tamanoLegible(repo.tamano_kb)),
    sinCopia
      ? h('td', { clase: 'muted' }, repo.omitido === 'fork' ? 'Fork no incluido' : 'Excluido')
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
        h('h2', null, 'No hay cuentas todavía'),
        h('p', null, 'Añade una cuenta desde Resumen para ver aquí sus repositorios.'),
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
          title: columna.titulo ?? null,
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
        columna.etiqueta,
        flecha ? h('span', { clase: 'flecha' }, flecha) : null,
      ),
    );
  }

  let cuerpoTabla;
  let contenedorTabla;

  function pintarTabla() {
    const filas = repite();
    if (filas.length === 0) {
      pintar(cuerpoTabla, h('tr', null, h('td', { colspan: '8', clase: 'center muted' }, 'Ningún repositorio coincide con el filtro.')));
      return;
    }
    pintar(cuerpoTabla, ...filas.map((repo) => filaRepo(estado.login, repo, cargarRepos)));
  }

  function refrescarCabecera() {
    pintar(
      contenedorTabla.querySelector('thead tr'),
      h('th', { scope: 'col' }, 'Incluido'),
      ...COLUMNAS.map(th),
      h('th', { scope: 'col' }, 'Acciones'),
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
    placeholder: 'Filtrar por dueño/nombre…',
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
    h('option', { value: '' }, 'Todos los estados'),
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
    h('h1', null, 'Repositorios'),
    h(
      'div',
      { clase: 'toolbar mb' },
      h(
        'div',
        { clase: 'controls' },
        h('div', { clase: 'field' }, h('label', { for: 'sel-cuenta' }, 'Cuenta'), selectorCuenta),
        h('div', { clase: 'field' }, h('label', { for: 'sel-filtro-texto' }, 'Buscar'), filtroTexto),
        h('div', { clase: 'field' }, h('label', { for: 'sel-filtro-estado' }, 'Estado'), filtroEstado),
      ),
    ),
    contenedorTabla,
  );

  refrescarCabecera();
  await cargarRepos();
}
