import { api } from '../api.js';
import { h, pintar, fechaAbsoluta } from '../dom.js';
import { t } from '../i18n.js';

// Claves de texto y no textos: el idioma se fija al arrancar, después de cargar este módulo.
const ETIQUETAS_RESULTADO = { ok: 'actividad.resultado.ok', 'con-fallos': 'actividad.resultado.con_fallos', error: 'actividad.resultado.error' };
const CLASES_RESULTADO = { ok: 'ok', 'con-fallos': 'warn', error: 'crit' };

function badgeResultado(resultado) {
  return h('span', { clase: `badge ${CLASES_RESULTADO[resultado] ?? 'muted'}` }, ETIQUETAS_RESULTADO[resultado] ? t(ETIQUETAS_RESULTADO[resultado]) : resultado);
}

async function pintarHistorico(contenedor, login) {
  const filas = await api.historial(login || null, 100);
  if (filas.length === 0) {
    pintar(contenedor, h('p', { clase: 'hint' }, t('actividad.historico.vacio')));
    return;
  }
  pintar(
    contenedor,
    h(
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
            h('th', { scope: 'col' }, t('actividad.col.fecha')),
            h('th', { scope: 'col' }, t('actividad.col.cuenta')),
            h('th', { scope: 'col' }, t('actividad.col.resultado')),
            h('th', { scope: 'col', clase: 'num' }, t('actividad.col.creados')),
            h('th', { scope: 'col', clase: 'num' }, t('actividad.col.huerfanos')),
            h('th', { scope: 'col', clase: 'num' }, t('actividad.col.fallos')),
            h('th', { scope: 'col' }, t('actividad.col.resumen')),
          ),
        ),
        h(
          'tbody',
          null,
          filas.map((s) =>
            h(
              'tr',
              null,
              h('td', null, fechaAbsoluta(s.fin)),
              h('td', { clase: 'mono' }, s.cuenta),
              h('td', null, badgeResultado(s.resultado)),
              h('td', { clase: 'num tabular' }, String(s.creados)),
              h('td', { clase: 'num tabular' }, String(s.huerfanos)),
              h('td', { clase: 'num tabular' }, String(s.fallos)),
              h('td', { clase: 'small' }, s.resumen),
            ),
          ),
        ),
      ),
    ),
  );
}

async function pintarAuditoria(contenedor) {
  const [entradas, verificacion] = await Promise.all([api.auditoria(200), api.verificarAuditoria()]);
  const integra = verificacion?.integra === true;

  const indicador = integra
    ? h('p', { clase: 'integridad ok' }, t('actividad.cadena.integra'))
    : h('p', { clase: 'integridad rota' }, t('actividad.cadena.rota', { id: verificacion?.rota_en_id ?? '?' }));

  pintar(
    contenedor,
    indicador,
    entradas.length === 0
      ? h('p', { clase: 'hint' }, t('actividad.auditoria.vacia'))
      : h(
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
                h('th', { scope: 'col' }, t('actividad.col.momento')),
                h('th', { scope: 'col' }, t('actividad.col.cuenta')),
                h('th', { scope: 'col' }, t('actividad.col.accion')),
                h('th', { scope: 'col' }, t('actividad.col.detalle')),
              ),
            ),
            h(
              'tbody',
              null,
              entradas.map((e) =>
                h(
                  'tr',
                  null,
                  h('td', null, fechaAbsoluta(e.momento)),
                  h('td', { clase: 'mono' }, e.cuenta ?? '—'),
                  h('td', { clase: 'mono' }, e.accion),
                  h('td', { clase: 'small' }, e.detalle),
                ),
              ),
            ),
          ),
        ),
  );
}

export async function render(contenedor) {
  const cuentas = await api.listarCuentas();
  const estado = { pestana: 'historico', login: '' };

  const panelHistorico = h('div', { clase: 'pestanas-panel', role: 'tabpanel', id: 'panel-historico', 'aria-labelledby': 'tab-historico' });
  const panelAuditoria = h('div', { clase: 'pestanas-panel', role: 'tabpanel', id: 'panel-auditoria', 'aria-labelledby': 'tab-auditoria', hidden: true });

  const selectorCuenta = h(
    'select',
    {
      id: 'sel-cuenta-historico',
      onChange: (evento) => {
        estado.login = evento.currentTarget.value;
        pintarHistorico(panelHistorico, estado.login);
      },
    },
    h('option', { value: '' }, t('actividad.todas_cuentas')),
    cuentas.map((cuenta) => h('option', { value: cuenta.login }, cuenta.login)),
  );

  function seleccionarPestana(nombre, botonHistorico, botonAuditoria) {
    estado.pestana = nombre;
    const activaHistorico = nombre === 'historico';
    botonHistorico.classList.toggle('active', activaHistorico);
    botonHistorico.setAttribute('aria-selected', String(activaHistorico));
    botonAuditoria.classList.toggle('active', !activaHistorico);
    botonAuditoria.setAttribute('aria-selected', String(!activaHistorico));
    panelHistorico.hidden = !activaHistorico;
    panelAuditoria.hidden = activaHistorico;
  }

  let botonHistorico;
  let botonAuditoria;
  botonHistorico = h(
    'button',
    {
      id: 'tab-historico',
      role: 'tab',
      type: 'button',
      'aria-selected': 'true',
      'aria-controls': 'panel-historico',
      clase: 'active',
      onClick: () => seleccionarPestana('historico', botonHistorico, botonAuditoria),
    },
    t('actividad.pestana.historico'),
  );
  botonAuditoria = h(
    'button',
    {
      id: 'tab-auditoria',
      role: 'tab',
      type: 'button',
      'aria-selected': 'false',
      'aria-controls': 'panel-auditoria',
      onClick: () => seleccionarPestana('auditoria', botonHistorico, botonAuditoria),
    },
    t('actividad.pestana.auditoria'),
  );

  pintar(
    contenedor,
    h('h1', null, t('actividad.titulo')),
    h('div', { clase: 'segmented', role: 'tablist', 'aria-label': t('actividad.titulo') }, botonHistorico, botonAuditoria),
    h('div', { clase: 'field mt mb' }, h('label', { for: 'sel-cuenta-historico' }, t('actividad.filtrar_cuenta')), selectorCuenta),
    panelHistorico,
    panelAuditoria,
  );

  await Promise.all([pintarHistorico(panelHistorico, ''), pintarAuditoria(panelAuditoria)]);
}
