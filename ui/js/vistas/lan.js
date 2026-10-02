// Panel «Acceso desde la red local» de Ajustes: expone el Gitea de la cuenta a la LAN por
// HTTPS con un nombre `.internal`.

import { api } from '../api.js';
import { h, pintar, avisar, ocupar } from '../dom.js';

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
    avisar(`${que} copiada al portapapeles.`, 'success');
  } catch {
    avisar('No se pudo copiar automáticamente: usa «Mostrar» y cópiala a mano.', 'warning');
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
              avisar('Copiado.', 'success');
            } catch {
              avisar('No se pudo copiar automáticamente.', 'warning');
            }
          },
        },
        'Copiar',
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
      h('h2', { id: 'titulo-lan-desactivar' }, 'Dejar de compartir en la red local'),
      h(
        'p',
        null,
        `El Gitea de «${login}» volverá a escuchar solo en este equipo y se reiniciará. Los demás equipos dejarán de poder clonar o enviar cambios.`,
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogo.close() }, 'Cancelar'),
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
          'Dejar de compartir',
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
      h('h2', null, 'Acceso desde la red local'),
      h('span', { clase: 'badge purple' }, 'Compartido por HTTPS'),
    ),
    h('p', null, 'Dirección para los equipos de la red: ', h('span', { clase: 'mono' }, informe.url_publica)),
    copiable(
      informe.ip_lan
        ? '1. En cada equipo, añade esta línea a /etc/hosts'
        : '1. En cada equipo, añade esta línea a /etc/hosts (sustituye la IP: no se pudo detectar la de este equipo)',
      informe.linea_hosts,
    ),
    copiable('En este mismo equipo, la línea es', `127.0.0.1  ${informe.host}`),
    copiable(
      `2. Copia el certificado (${informe.ruta_certificado}) a cada equipo y haz que git confíe en él solo para esta dirección`,
      informe.comando_git_cliente,
    ),
    copiable('Huella SHA-256 del certificado: compruébala en el otro equipo antes de confiar', informe.huella_sha256),
    copiable('3. Recomendado: limita el puerto a tu red en el cortafuegos de este equipo', informe.regla_cortafuegos_sugerida),
    h(
      'p',
      { clase: 'hint' },
      'Mientras está compartido, Gitea escucha en todas las interfaces de red de este equipo, también en VPN o Wi-Fi ajenas. Desactívalo fuera de tu red.',
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
                avisar('El Gitea de la cuenta vuelve a ser solo local.', 'success');
              } catch (error) {
                avisar(`No se pudo desactivar: ${error?.mensaje ?? error}`, 'error');
              }
              await recargar();
            }).showModal(),
        },
        'Dejar de compartir…',
      ),
    ),
  );
}

function vistaInactiva(contenedor, login, hostSugerido, recargar) {
  const campoHost = h('input', { id: 'campo-host-lan', type: 'text', value: hostSugerido, autocomplete: 'off', spellcheck: false });
  const mensaje = h('p', { clase: 'hint' });
  pintar(
    contenedor,
    h('div', { clase: 'row between' }, h('h2', null, 'Acceso desde la red local'), h('span', { clase: 'badge' }, 'Solo este equipo')),
    h(
      'p',
      null,
      'Permite que otros equipos de tu red clonen y envíen cambios a este Gitea. Siempre por HTTPS, con un certificado propio de esta cuenta; el registro sigue cerrado y los repositorios, privados.',
    ),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'campo-host-lan' }, 'Nombre en la red (debe terminar en .internal)'),
      campoHost,
      h('p', { clase: 'hint' }, 'Cada equipo lo resuelve con una línea en su /etc/hosts; no hace falta DNS.'),
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
            mensaje.textContent = 'Generando el certificado y reiniciando Gitea…';
            try {
              await api.lanActivar(login, campoHost.value.trim() || null);
              avisar('Compartido en la red local por HTTPS.', 'success');
              await recargar();
            } catch (error) {
              mensaje.textContent = `Error: ${error?.mensaje ?? error}`;
              boton.disabled = false;
            }
          },
        },
        'Compartir en la red local',
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
      h('h2', { id: 'titulo-lan-usuario-eliminar' }, `Eliminar el usuario «${nombre}»`),
      h(
        'p',
        null,
        `«${nombre}» dejará de poder entrar en el Gitea de «${login}». Si vuelve a necesitar acceso, habrá que crearlo de nuevo con una contraseña distinta.`,
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogo.close() }, 'Cancelar'),
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
          'Eliminar',
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
        botonMostrar.textContent = visible ? 'Mostrar' : 'Ocultar';
      },
    },
    'Mostrar',
  );

  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-lan-usuario-creado' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-lan-usuario-creado' }, `Usuario «${nombre}» creado`),
      h(
        'div',
        { clase: 'credencial' },
        h('span', { clase: 'hint' }, 'Usuario'),
        h('code', { clase: 'pre' }, nombre),
        h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(nombre, 'Usuario') }, 'Copiar'),
        h('span', { clase: 'hint' }, 'Contraseña'),
        campoPassword,
        h(
          'div',
          { clase: 'row' },
          botonMostrar,
          h('button', { type: 'button', clase: 'btn ghost sm', onClick: () => copiar(secreto, 'Contraseña') }, 'Copiar'),
        ),
      ),
      h(
        'p',
        { clase: 'aviso-secreto' },
        'Esta contraseña no se volverá a mostrar: cópiala ahora y pásasela a la persona por un canal seguro (no por correo ni chat sin cifrar).',
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { type: 'button', clase: 'btn', onClick: () => dialogo.close() }, 'Cerrar'),
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
                avisar(`Usuario «${nombre}» eliminado.`, 'success');
              } catch (error) {
                avisar(`No se pudo eliminar: ${error?.mensaje ?? error}`, 'error');
              }
              await recargar();
            }).showModal(),
        },
        'Eliminar',
      ),
    ),
  );
}

function listaUsuarios(login, usuarios, recargar) {
  if (usuarios.length === 0) {
    return h('p', { clase: 'hint' }, 'Todavía no hay usuarios: solo el administrador puede entrar.');
  }
  return h(
    'div',
    { clase: 'table-wrap' },
    h(
      'table',
      null,
      h('thead', null, h('tr', null, h('th', { scope: 'col' }, 'Usuario'), h('th', { scope: 'col' }, 'Acciones'))),
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
    h('h2', null, 'Usuarios de la LAN'),
    h(
      'p',
      null,
      'Cada usuario que crees aquí puede leer todos los repositorios clonados y escribir solo en los de contingencia: es un usuario «restringido» de Gitea, que no ve nada más. Puede cambiar su propia contraseña desde la web de Gitea; si la olvida, elimínalo y créalo de nuevo. gitmereba no guarda estas contraseñas.',
    ),
    activo
      ? null
      : h(
          'div',
          { clase: 'banner show warn' },
          h('span', null, 'Los usuarios existen, pero sin el acceso LAN activado nadie de fuera llega a este Gitea.'),
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
            mensaje.textContent =
              'El nombre debe tener entre 1 y 39 caracteres (letras, números, «-», «_» o «.») y no puede ser «gitmereba-admin» ni empezar por «contingencia-».';
            return;
          }
          const liberar = ocupar(botonCrear, 'Creando…');
          try {
            const creado = await api.lanUsuarioCrear(login, nombre);
            mensaje.textContent = '';
            campoNombre.value = '';
            dialogoUsuarioCreado(creado.nombre, creado.password);
            await recargar();
          } catch (error) {
            mensaje.textContent = `Error: ${error?.mensaje ?? error}`;
          } finally {
            liberar();
          }
        },
      },
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'campo-lan-usuario-nuevo' }, 'Nombre de usuario'),
        h(
          'div',
          { clase: 'inline-form' },
          campoNombre,
          (botonCrear = h('button', { type: 'submit', clase: 'btn' }, 'Crear usuario')),
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
          h('h2', null, 'Usuarios de la LAN'),
          h('p', { clase: 'hint' }, `No disponible: ${error?.mensaje ?? error}`),
        );
      }
    } catch (error) {
      pintar(tarjeta, h('h2', null, 'Acceso desde la red local'), h('p', { clase: 'hint' }, `No disponible: ${error?.mensaje ?? error}`));
    }
  }
  recargar();
  return [tarjeta, tarjetaUsuarios];
}
