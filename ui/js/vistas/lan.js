// Panel «Acceso desde la red local» de Ajustes: expone el Gitea de la cuenta a la LAN por
// HTTPS con un nombre `.internal`.

import { api } from '../api.js';
import { h, pintar, avisar, ocupar } from '../dom.js';
import { t } from '../i18n.js';

const NOMBRE_USUARIO_VALIDO = /^[A-Za-z0-9_.-]{1,39}$/;
const PASSWORD_OCULTA = '••••••••••••';

function nombreUsuarioValido(nombre) {
  return (
    NOMBRE_USUARIO_VALIDO.test(nombre) && nombre !== 'gitmereba-admin' && !nombre.startsWith('contingencia-')
  );
}

async function copiar(texto, que) {
  try {
    await navigator.clipboard.writeText(texto);
    avisar(t('lan.copiada', { que }), 'success');
  } catch {
    avisar(t('lan.copia.error'), 'warning');
  }
}

function copiable(etiqueta, texto) {
  return h(
    'div',
    { clase: 'field' },
    h('span', { clase: 'hint' }, etiqueta),
    h(
      'div',
      { clase: 'copiable' },
      h('code', { clase: 'pre' }, texto),
      h(
        'button',
        {
          type: 'button',
          clase: 'btn ghost sm',
          onClick: async () => {
            try {
              await navigator.clipboard.writeText(texto);
              avisar(t('lan.copiado'), 'success');
            } catch {
              avisar(t('lan.copiar.error'), 'warning');
            }
          },
        },
        t('lan.copiar'),
      ),
    ),
  );
}

function dialogoDesactivar(login, alConfirmar) {
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-lan-desactivar' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-lan-desactivar' }, t('lan.desactivar.titulo')),
      h(
        'p',
        null,
        t('lan.desactivar.texto', { login }),
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogo.close() }, t('lan.cancelar')),
        h(
          'button',
          {
            type: 'button',
            clase: 'btn danger',
            onClick: async () => {
              dialogo.close();
              await alConfirmar();
            },
          },
          t('lan.desactivar.confirmar'),
        ),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => dialogo.remove());
  return dialogo;
}

function vistaActiva(contenedor, login, informe, recargar) {
  pintar(
    contenedor,
    h(
      'div',
      { clase: 'row between' },
      h('h2', null, t('lan.titulo')),
      h('span', { clase: 'badge purple' }, t('lan.activa.badge')),
    ),
    h('p', null, t('lan.activa.direccion'), h('span', { clase: 'mono' }, informe.url_publica)),
    copiable(
      informe.ip_lan
        ? t('lan.paso.hosts')
        : t('lan.paso.hosts_sin_ip'),
      informe.linea_hosts,
    ),
    copiable(t('lan.paso.hosts_local'), `127.0.0.1  ${informe.host}`),
    copiable(
      t('lan.paso.certificado', { ruta: informe.ruta_certificado }),
      informe.comando_git_cliente,
    ),
    copiable(t('lan.paso.huella'), informe.huella_sha256),
    copiable(t('lan.paso.cortafuegos'), informe.regla_cortafuegos_sugerida),
    h(
      'p',
      { clase: 'hint' },
      t('lan.activa.aviso'),
    ),
    h(
      'div',
      { clase: 'row mt' },
      h(
        'button',
        {
          type: 'button',
          clase: 'btn ghost',
          onClick: () =>
            dialogoDesactivar(login, async () => {
              try {
                await api.lanDesactivar(login);
                avisar(t('lan.desactivado'), 'success');
              } catch (error) {
                avisar(t('lan.desactivar.error', { mensaje: error?.mensaje ?? error }), 'error');
              }
              await recargar();
            }).showModal(),
        },
        t('lan.dejar_compartir'),
      ),
    ),
  );
}

function vistaInactiva(contenedor, login, hostSugerido, recargar) {
  const campoHost = h('input', { id: 'campo-host-lan', type: 'text', value: hostSugerido, autocomplete: 'off', spellcheck: false });
  const mensaje = h('p', { clase: 'hint' });
  pintar(
    contenedor,
    h('div', { clase: 'row between' }, h('h2', null, t('lan.titulo')), h('span', { clase: 'badge' }, t('lan.inactiva.badge'))),
    h('p', null, t('lan.inactiva.texto')),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'campo-host-lan' }, t('lan.host.etiqueta')),
      campoHost,
      h('p', { clase: 'hint' }, t('lan.host.ayuda')),
    ),
    mensaje,
    h(
      'div',
      { clase: 'row mt' },
      h(
        'button',
        {
          type: 'button',
          clase: 'btn',
          onClick: async (evento) => {
            const boton = evento.currentTarget;
            boton.disabled = true;
            mensaje.textContent = t('lan.activando');
            try {
              await api.lanActivar(login, campoHost.value.trim() || null);
              avisar(t('lan.activado'), 'success');
              await recargar();
            } catch (error) {
              mensaje.textContent = t('lan.error', { mensaje: error?.mensaje ?? error });
              boton.disabled = false;
            }
          },
        },
        t('lan.compartir'),
      ),
    ),
  );
}

function dialogoEliminarUsuario(login, nombre, alConfirmar) {
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-lan-usuario-eliminar' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-lan-usuario-eliminar' }, t('lan.usuario.eliminar.titulo', { nombre })),
      h(
        'p',
        null,
        t('lan.usuario.eliminar.texto', { nombre, login }),
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogo.close() }, t('lan.cancelar')),
        h(
          'button',
          {
            type: 'button',
            clase: 'btn danger',
            onClick: async () => {
              dialogo.close();
              await alConfirmar();
            },
          },
          t('lan.eliminar'),
        ),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => dialogo.remove());
  return dialogo;
}

/** Diálogo que muestra, una sola vez, el usuario y la contraseña recién creados. La
 * contraseña llega ya generada por Gitea; aquí solo se enseña y se olvida al cerrar
 * (no queda en el DOM ni en ninguna variable), igual que ui/js/vistas/credenciales.js. */
function dialogoUsuarioCreado(nombre, password) {
  let secreto = password;
  const campoPassword = h('code', { clase: 'pre' }, PASSWORD_OCULTA);
  const botonMostrar = h(
    'button',
    {
      type: 'button',
      clase: 'btn ghost sm',
      onClick: () => {
        const visible = campoPassword.textContent !== PASSWORD_OCULTA;
        campoPassword.textContent = visible ? PASSWORD_OCULTA : secreto;
        botonMostrar.textContent = visible ? t('lan.mostrar') : t('lan.ocultar');
      },
    },
    t('lan.mostrar'),
  );

  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-lan-usuario-creado' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-lan-usuario-creado' }, t('lan.creado.titulo', { nombre })),
      h(
        'div',
        { clase: 'credencial' },
        h('span', { clase: 'hint' }, t('lan.usuario')),
        h('code', { clase: 'pre' }, nombre),
        h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(nombre, t('lan.usuario')) }, t('lan.copiar')),
        h('span', { clase: 'hint' }, t('lan.password')),
        campoPassword,
        h(
          'div',
          { clase: 'row' },
          botonMostrar,
          h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(secreto, t('lan.password')) }, t('lan.copiar')),
        ),
      ),
      h(
        'p',
        { clase: 'aviso-secreto' },
        t('lan.creado.aviso'),
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn', onClick: () => dialogo.close() }, t('lan.cerrar')),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => {
    campoPassword.textContent = PASSWORD_OCULTA;
    secreto = null;
    dialogo.remove();
  });
  dialogo.showModal();
  return dialogo;
}

function filaUsuario(login, nombre, recargar) {
  return h(
    'tr',
    null,
    h('td', null, nombre),
    h(
      'td',
      { clase: 'acciones-fila' },
      h(
        'button',
        {
          type: 'button',
          clase: 'btn ghost sm',
          onClick: () =>
            dialogoEliminarUsuario(login, nombre, async () => {
              try {
                await api.lanUsuarioEliminar(login, nombre);
                avisar(t('lan.usuario.eliminado', { nombre }), 'success');
              } catch (error) {
                avisar(t('lan.usuario.eliminar.error', { mensaje: error?.mensaje ?? error }), 'error');
              }
              await recargar();
            }).showModal(),
        },
        t('lan.eliminar'),
      ),
    ),
  );
}

function listaUsuarios(login, usuarios, recargar) {
  if (usuarios.length === 0) {
    return h('p', { clase: 'hint' }, t('lan.usuarios.vacio'));
  }
  return h(
    'div',
    { clase: 'table-wrap' },
    h(
      'table',
      null,
      h('thead', null, h('tr', null, h('th', { scope: 'col' }, t('lan.col.usuario')), h('th', { scope: 'col' }, t('lan.col.acciones')))),
      h(
        'tbody',
        null,
        usuarios.map((usuario) => filaUsuario(login, usuario.nombre, recargar)),
      ),
    ),
  );
}

function vistaUsuarios(contenedor, login, usuarios, activo, recargar) {
  let botonCrear;
  const campoNombre = h('input', {
    id: 'campo-lan-usuario-nuevo',
    type: 'text',
    autocomplete: 'off',
    spellcheck: false,
    maxlength: '39',
  });
  const mensaje = h('p', { clase: 'hint' });

  pintar(
    contenedor,
    h('h2', null, t('lan.usuarios.titulo')),
    h('p', null, t('lan.usuarios.texto')),
    activo
      ? null
      : h(
          'div',
          { clase: 'banner show warn' },
          h('span', null, t('lan.usuarios.aviso_inactivo')),
        ),
    listaUsuarios(login, usuarios, recargar),
    h(
      'form',
      {
        clase: 'row mt',
        onSubmit: async (evento) => {
          evento.preventDefault();
          const nombre = campoNombre.value.trim();
          if (!nombreUsuarioValido(nombre)) {
            mensaje.textContent = t('lan.usuario.nombre_invalido');
            return;
          }
          const liberar = ocupar(botonCrear, t('lan.creando'));
          try {
            const creado = await api.lanUsuarioCrear(login, nombre);
            mensaje.textContent = '';
            campoNombre.value = '';
            dialogoUsuarioCreado(creado.nombre, creado.password);
            await recargar();
          } catch (error) {
            mensaje.textContent = t('lan.error', { mensaje: error?.mensaje ?? error });
          } finally {
            liberar();
          }
        },
      },
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'campo-lan-usuario-nuevo' }, t('lan.usuario.etiqueta')),
        h(
          'div',
          { clase: 'inline-form' },
          campoNombre,
          (botonCrear = h('button', { type: 'submit', clase: 'btn' }, t('lan.crear'))),
        ),
      ),
      mensaje,
    ),
  );
}

/** Devuelve la tarjeta del panel y la de sus usuarios de LAN; ambas se rellenan solas y
 * se repintan tras cada cambio. */
export function panelLan(login) {
  const tarjeta = h('section', { clase: 'card pad mt panel-medio' });
  const tarjetaUsuarios = h('section', { clase: 'card pad mt panel-medio' });

  async function recargar() {
    try {
      const estado = await api.lanEstado(login);
      if (estado.activo && estado.informe) vistaActiva(tarjeta, login, estado.informe, recargar);
      else vistaInactiva(tarjeta, login, estado.host_sugerido, recargar);
      try {
        const usuarios = await api.lanUsuariosListar(login);
        vistaUsuarios(tarjetaUsuarios, login, usuarios, estado.activo, recargar);
      } catch (error) {
        pintar(
          tarjetaUsuarios,
          h('h2', null, t('lan.usuarios.titulo')),
          h('p', { clase: 'hint' }, t('lan.no_disponible', { mensaje: error?.mensaje ?? error })),
        );
      }
    } catch (error) {
      pintar(
        tarjeta,
        h('h2', null, t('lan.titulo')),
        h('p', { clase: 'hint' }, t('lan.no_disponible', { mensaje: error?.mensaje ?? error })),
      );
    }
  }
  recargar();
  return [tarjeta, tarjetaUsuarios];
}
