// Diálogo con el usuario y la contraseña del Gitea local de una cuenta. La contraseña se
// pide al abrirlo (sale del llavero del sistema), no se pinta hasta pulsar «Mostrar» y se
// olvida al cerrar.

import { api } from '../api.js';
import { h, avisar } from '../dom.js';

const OCULTA = '••••••••••••';

async function copiar(texto, que) {
  try {
    await navigator.clipboard.writeText(texto);
    avisar(`${que} copiada al portapapeles.`, 'success');
  } catch {
    avisar('No se pudo copiar automáticamente: usa «Mostrar» y cópiala a mano.', 'warning');
  }
}

export async function dialogoCredenciales(login) {
  let credenciales;
  try {
    credenciales = await api.credencialesGitea(login);
  } catch (error) {
    avisar(`No se pudieron leer las credenciales: ${error?.mensaje ?? error}`, 'error');
    return;
  }

  const campoPassword = h('code', { clase: 'pre' }, OCULTA);
  const botonMostrar = h(
    'button',
    {
      type: 'button',
      clase: 'btn ghost sm',
      onClick: () => {
        const visible = campoPassword.textContent !== OCULTA;
        campoPassword.textContent = visible ? OCULTA : credenciales.password;
        botonMostrar.textContent = visible ? 'Mostrar' : 'Ocultar';
      },
    },
    'Mostrar',
  );

  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-credenciales' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-credenciales' }, `Entrar en el Gitea de «${login}»`),
      h(
        'p',
        null,
        'gitmereba creó este usuario administrador al dar de alta la cuenta. La contraseña es aleatoria y está en el llavero de tu sistema.',
      ),
      h(
        'div',
        { clase: 'credencial' },
        h('span', { clase: 'hint' }, 'Usuario'),
        h('code', { clase: 'pre' }, credenciales.usuario),
        h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(credenciales.usuario, 'Usuario') }, 'Copiar'),
        h('span', { clase: 'hint' }, 'Contraseña'),
        campoPassword,
        h(
          'div',
          { clase: 'row' },
          botonMostrar,
          h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(credenciales.password, 'Contraseña') }, 'Copiar'),
        ),
      ),
      h(
        'p',
        { clase: 'aviso-secreto' },
        'Con esta cuenta se puede borrar cualquier repositorio del clon: no la compartas. Cada consulta queda en la auditoría.',
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogo.close() }, 'Cerrar'),
        h(
          'button',
          {
            type: 'button',
            clase: 'btn',
            onClick: async () => {
              try {
                await api.abrirGitea(login);
              } catch (error) {
                avisar(`No se pudo abrir Gitea: ${error?.mensaje ?? error}`, 'error');
              }
            },
          },
          'Abrir Gitea',
        ),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => {
    campoPassword.textContent = OCULTA;
    credenciales = null;
    dialogo.remove();
  });
  dialogo.showModal();
}
