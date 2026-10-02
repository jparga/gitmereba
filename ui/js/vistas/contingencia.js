import { api } from '../api.js';
import { h, pintar, avisar } from '../dom.js';

function dialogoConfirmarActivacion(repo, onConfirmar) {
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-confirmar-activar' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-confirmar-activar' }, 'Activar contingencia'),
      h(
        'p',
        null,
        `«${repo.id.dueno}/${repo.id.nombre}» pasará a ser un repositorio con escritura `,
        'en local. El mirror original no se toca y queda en pausa hasta que reconcilies.',
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', type: 'button', onClick: () => dialogo.close() }, 'Cancelar'),
        h(
          'button',
          {
            clase: 'btn danger',
            onClick: async () => {
              dialogo.close();
              await onConfirmar();
            },
          },
          'Activar contingencia',
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
      h('h2', { id: 'titulo-reconciliar' }, 'Reconciliar con GitHub'),
      h('p', null, `Se harán push de los commits locales de «${repo.id.dueno}/${repo.id.nombre}» hacia GitHub, sin --force.`),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'token-reconciliar' }, 'Token de escritura'),
        (entrada = h('input', {
          id: 'token-reconciliar',
          type: 'password',
          autocomplete: 'off',
          required: true,
        })),
        h('p', { clase: 'aviso-secreto' }, 'Este token no se guarda: solo se usa para este push.'),
      ),
      resultado,
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', onClick: () => dialogo.close() }, 'Cerrar'),
        h(
          'button',
          {
            clase: 'btn accent',
            onClick: async (evento) => {
              const token = entrada.value;
              entrada.value = '';
              if (!token) {
                resultado.textContent = 'Escribe un token de escritura.';
                return;
              }
              evento.currentTarget.disabled = true;
              try {
                const respuesta = await api.reconciliar(login, repo.id, token);
                resultado.textContent =
                  respuesta.resultado === 'ok'
                    ? `Éxito: ${respuesta.mensaje}`
                    : `Divergencia detectada, no se ha forzado nada: ${respuesta.mensaje}`;
                if (respuesta.resultado === 'ok') await onHecho();
              } catch (error) {
                resultado.textContent = `Error: ${error?.mensaje ?? error}`;
              } finally {
                evento.currentTarget.disabled = false;
              }
            },
          },
          'Reconciliar',
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
                avisar(`Contingencia activada en ${repo.id.dueno}/${repo.id.nombre}.`, 'success');
                await recargar();
              } catch (error) {
                avisar(`No se pudo activar la contingencia: ${error?.mensaje ?? error}`, 'error');
              }
            });
            dialogo.showModal();
          },
        },
        'Activar contingencia',
      ),
    ),
  );
}

function filaActiva(login, entrada, recargar) {
  return h(
    'li',
    { clase: 'card pad mb' },
    h('div', { clase: 'row between' }, h('span', { clase: 'mono' }, `${entrada.id.dueno}/${entrada.id.nombre}`), h('span', { clase: 'badge purple' }, 'En contingencia')),
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
              avisar('Comando copiado.', 'success');
            } catch {
              avisar('No se pudo copiar automáticamente.', 'warning');
            }
          },
        },
        'Copiar',
      ),
    ),
    h('p', { clase: 'hint mt' }, 'Commits de más por rama:'),
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
      'Reconciliar con GitHub',
    ),
  );
}

export async function render(contenedor) {
  const cuentas = await api.listarCuentas();
  if (cuentas.length === 0) {
    pintar(contenedor, h('div', { clase: 'card pad vacio' }, h('h2', null, 'No hay cuentas todavía'), h('p', null, 'Añade una cuenta desde Resumen.')));
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
        : h('p', { clase: 'hint' }, 'No hay repositorios disponibles para activar.'),
    );
    pintar(
      zonaActivas,
      activas.length ? activas.map((a) => filaActiva(estado.login, a, recargar)) : h('p', { clase: 'hint' }, 'Ningún repositorio está en contingencia ahora mismo.'),
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
    h('h1', null, 'Contingencia'),
    h(
      'p',
      { clase: 'lead' },
      'La contingencia crea una copia de trabajo con escritura de un repositorio en tu Gitea local, para que puedas seguir trabajando aunque GitHub no esté disponible. El mirror original se conserva intacto y en pausa. Cuando GitHub vuelva, reconcilia para enviar tus commits; nunca se fuerza nada.',
    ),
    h('div', { clase: 'field mb' }, h('label', { for: 'sel-cuenta-contingencia' }, 'Cuenta'), selectorCuenta),
    h('h2', null, 'Disponibles para activar'),
    zonaCandidatas,
    h('h2', null, 'En contingencia'),
    zonaActivas,
  );

  await recargar();
}
