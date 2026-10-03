import { api } from '../api.js';
import { h, pintar, avisar } from '../dom.js';
import { t } from '../i18n.js';

function dialogoConfirmarActivacion(repo, onConfirmar) {
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-confirmar-activar' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-confirmar-activar' }, t('contingencia.activar')),
      h('p', null, t('contingencia.activar.texto', { repo: `${repo.id.dueno}/${repo.id.nombre}` })),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', type: 'button', onClick: () => dialogo.close() }, t('contingencia.cancelar')),
        h(
          'button',
          {
            clase: 'btn danger',
            onClick: async () => {
              dialogo.close();
              await onConfirmar();
            },
          },
          t('contingencia.activar'),
        ),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => dialogo.remove());
  return dialogo;
}

function dialogoReconciliar(repo, login, onHecho) {
  let entrada;
  const resultado = h('p', { clase: 'hint' });
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-reconciliar' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-reconciliar' }, t('contingencia.reconciliar')),
      h('p', null, t('contingencia.reconciliar.texto', { repo: `${repo.id.dueno}/${repo.id.nombre}` })),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'token-reconciliar' }, t('contingencia.token.etiqueta')),
        (entrada = h('input', {
          id: 'token-reconciliar',
          type: 'password',
          autocomplete: 'off',
          required: true,
        })),
        h('p', { clase: 'aviso-secreto' }, t('contingencia.token.aviso')),
      ),
      resultado,
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', onClick: () => dialogo.close() }, t('contingencia.cerrar')),
        h(
          'button',
          {
            clase: 'btn accent',
            onClick: async (evento) => {
              const boton = evento.currentTarget;
              const token = entrada.value;
              entrada.value = '';
              if (!token) {
                resultado.textContent = t('contingencia.token.falta');
                return;
              }
              boton.disabled = true;
              try {
                const respuesta = await api.reconciliar(login, repo.id, token);
                resultado.textContent =
                  respuesta.resultado === 'ok'
                    ? t('contingencia.reconciliar.exito', { mensaje: respuesta.mensaje })
                    : t('contingencia.reconciliar.divergencia', { mensaje: respuesta.mensaje });
                if (respuesta.resultado === 'ok') await onHecho();
              } catch (error) {
                resultado.textContent = t('contingencia.reconciliar.error', { mensaje: error?.mensaje ?? error });
              } finally {
                boton.disabled = false;
              }
            },
          },
          t('contingencia.reconciliar.boton'),
        ),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => {
    entrada.value = '';
    dialogo.remove();
  });
  return dialogo;
}

function filaCandidata(login, repo, recargar) {
  return h(
    'li',
    { clase: 'card pad mb' },
    h(
      'div',
      { clase: 'toolbar' },
      h('span', { clase: 'mono' }, `${repo.id.dueno}/${repo.id.nombre}`),
      h(
        'button',
        {
          clase: 'btn ghost sm',
          onClick: () => {
            const dialogo = dialogoConfirmarActivacion(repo, async () => {
              try {
                await api.activarContingencia(login, repo.id);
                avisar(t('contingencia.activada', { repo: `${repo.id.dueno}/${repo.id.nombre}` }), 'success');
                await recargar();
              } catch (error) {
                avisar(t('contingencia.activar.error', { mensaje: error?.mensaje ?? error }), 'error');
              }
            });
            dialogo.showModal();
          },
        },
        t('contingencia.activar'),
      ),
    ),
  );
}

function filaActiva(login, entrada, recargar) {
  return h(
    'li',
    { clase: 'card pad mb' },
    h('div', { clase: 'row between' }, h('span', { clase: 'mono' }, `${entrada.id.dueno}/${entrada.id.nombre}`), h('span', { clase: 'badge purple' }, t('contingencia.en_contingencia'))),
    h(
      'div',
      { clase: 'copiable mt' },
      h('code', { clase: 'pre' }, entrada.comando),
      h(
        'button',
        {
          clase: 'btn ghost sm',
          onClick: async () => {
            try {
              await navigator.clipboard.writeText(entrada.comando);
              avisar(t('contingencia.comando.copiado'), 'success');
            } catch {
              avisar(t('contingencia.comando.no_copiado'), 'warning');
            }
          },
        },
        t('contingencia.copiar'),
      ),
    ),
    h('p', { clase: 'hint mt' }, t('contingencia.commits_de_mas')),
    h(
      'div',
      { clase: 'commits-de-mas' },
      entrada.commits_de_mas.map((c) => h('span', { clase: 'badge info' }, `${c.rama} +${c.commits}`)),
    ),
    h(
      'button',
      {
        clase: 'btn mt',
        onClick: () => dialogoReconciliar({ id: entrada.id }, login, recargar).showModal(),
      },
      t('contingencia.reconciliar'),
    ),
  );
}

export async function render(contenedor) {
  const cuentas = await api.listarCuentas();
  if (cuentas.length === 0) {
    pintar(contenedor, h('div', { clase: 'card pad vacio' }, h('h2', null, t('repos.vacio.titulo')), h('p', null, t('contingencia.vacio.texto'))));
    return;
  }

  const estado = { login: cuentas[0].login };
  const zonaCandidatas = h('ul', { clase: 'section' });
  const zonaActivas = h('ul', { clase: 'section' });

  async function recargar() {
    const [repos, activas] = await Promise.all([api.listarRepos(estado.login), api.estadoContingencia(estado.login)]);
    const idsActivos = new Set(activas.map((a) => `${a.id.dueno}/${a.id.nombre}`));
    const candidatas = repos.filter((r) => r.estado !== 'contingencia' && r.estado !== 'excluido' && !idsActivos.has(`${r.id.dueno}/${r.id.nombre}`));

    pintar(
      zonaCandidatas,
      candidatas.length
        ? candidatas.map((r) => filaCandidata(estado.login, r, recargar))
        : h('p', { clase: 'hint' }, t('contingencia.sin_candidatas')),
    );
    pintar(
      zonaActivas,
      activas.length ? activas.map((a) => filaActiva(estado.login, a, recargar)) : h('p', { clase: 'hint' }, t('contingencia.sin_activas')),
    );
  }

  const selectorCuenta = h(
    'select',
    {
      id: 'sel-cuenta-contingencia',
      onChange: async (evento) => {
        estado.login = evento.currentTarget.value;
        await recargar();
      },
    },
    cuentas.map((cuenta) => h('option', { value: cuenta.login }, cuenta.login)),
  );

  pintar(
    contenedor,
    h('h1', null, t('nav.contingencia')),
    h(
      'p',
      { clase: 'lead' },
      t('contingencia.lead'),
    ),
    h('div', { clase: 'field mb' }, h('label', { for: 'sel-cuenta-contingencia' }, t('repos.campo.cuenta')), selectorCuenta),
    h('h2', null, t('contingencia.disponibles')),
    zonaCandidatas,
    h('h2', null, t('contingencia.en_contingencia')),
    zonaActivas,
  );

  await recargar();
}
