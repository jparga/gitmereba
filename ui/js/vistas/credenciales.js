// Diálogo con el usuario y la contraseña del Gitea local de una cuenta. La contraseña se
// pide al abrirlo (sale del llavero del sistema), no se pinta hasta pulsar «Mostrar» y se
// olvida al cerrar.

import { api } from '../api.js';
import { h, avisar } from '../dom.js';
import { t } from '../i18n.js';

const OCULTA = '••••••••••••';

async function copiar(texto, claveCopiado) {
  try {
    await navigator.clipboard.writeText(texto);
    avisar(t(claveCopiado), 'success');
  } catch {
    avisar(t('credenciales.no_copiada'), 'warning');
  }
}

export async function dialogoCredenciales(login) {
  let credenciales;
  try {
    credenciales = await api.credencialesGitea(login);
  } catch (error) {
    avisar(t('credenciales.error', { mensaje: error?.mensaje ?? error }), 'error');
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
        botonMostrar.textContent = visible ? t('credenciales.mostrar') : t('credenciales.ocultar');
      },
    },
    t('credenciales.mostrar'),
  );

  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-credenciales' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-credenciales' }, t('credenciales.titulo', { login })),
      h(
        'p',
        null,
        t('credenciales.texto'),
      ),
      h(
        'div',
        { clase: 'credencial' },
        h('span', { clase: 'hint' }, t('credenciales.usuario')),
        h('code', { clase: 'pre' }, credenciales.usuario),
        h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(credenciales.usuario, 'credenciales.usuario_copiado') }, t('credenciales.copiar')),
        h('span', { clase: 'hint' }, t('credenciales.password')),
        campoPassword,
        h(
          'div',
          { clase: 'row' },
          botonMostrar,
          h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(credenciales.password, 'credenciales.password_copiada') }, t('credenciales.copiar')),
        ),
      ),
      h(
        'p',
        { clase: 'aviso-secreto' },
        t('credenciales.aviso'),
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogo.close() }, t('contingencia.cerrar')),
        h(
          'button',
          {
            type: 'button',
            clase: 'btn',
            onClick: async () => {
              try {
                await api.abrirGitea(login);
              } catch (error) {
                avisar(t('resumen.abrir_gitea.error', { mensaje: error?.mensaje ?? error }), 'error');
              }
            },
          },
          t('resumen.abrir_gitea'),
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
