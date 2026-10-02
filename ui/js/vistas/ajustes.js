import { api } from '../api.js';
import { h, pintar, avisar, ocupar } from '../dom.js';
import { crearConmutadorTema } from '../tema.js';
import { navegar } from '../router.js';
import { panelLan } from './lan.js';

function dialogoRotarToken(login) {
  let entrada;
  const mensaje = h('p', { clase: 'hint' });
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-rotar-token' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-rotar-token' }, `Rotar el token de «${login}»`),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'token-nuevo' }, 'Nuevo token de lectura'),
        (entrada = h('input', { id: 'token-nuevo', type: 'password', autocomplete: 'off', required: true })),
        h('p', { clase: 'aviso-secreto' }, 'No se guarda en la interfaz: se envía directamente al llavero.'),
      ),
      mensaje,
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', onClick: () => dialogo.close() }, 'Cancelar'),
        h(
          'button',
          {
            clase: 'btn accent',
            onClick: async (evento) => {
              const token = entrada.value;
              entrada.value = '';
              if (!token) {
                mensaje.textContent = 'Escribe el token nuevo.';
                return;
              }
              const liberar = ocupar(evento.currentTarget, 'Comprobando…');
              try {
                await api.rotarToken(login, token);
                avisar('Token rotado correctamente.', 'success');
                dialogo.close();
              } catch (error) {
                mensaje.textContent = `Error: ${error?.mensaje ?? error}`;
              } finally {
                liberar();
              }
            },
          },
          'Rotar token',
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

function dialogoBaja(login, onBaja) {
  let entradaConfirmacion;
  let botonConfirmar;
  const dialogo = h(
    'dialog',
    { 'aria-labelledby': 'titulo-baja' },
    h(
      'div',
      { clase: 'dialogo-cuerpo' },
      h('h2', { id: 'titulo-baja' }, `Dar de baja «${login}»`),
      h('p', null, 'Esto detiene la sincronización y las unidades systemd de la cuenta. Los repositorios ya clonados no se borran.'),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'confirmar-login' }, `Escribe «${login}» para confirmar`),
        (entradaConfirmacion = h('input', {
          id: 'confirmar-login',
          type: 'text',
          autocomplete: 'off',
          onInput: (evento) => {
            botonConfirmar.disabled = evento.currentTarget.value !== login;
          },
        })),
      ),
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', onClick: () => dialogo.close() }, 'Cancelar'),
        (botonConfirmar = h(
          'button',
          {
            clase: 'btn danger',
            disabled: true,
            onClick: async () => {
              try {
                await api.bajaCuenta(login);
                dialogo.close();
                avisar(`Cuenta «${login}» dada de baja.`, 'success');
                await onBaja();
              } catch (error) {
                avisar(`No se pudo dar de baja la cuenta: ${error?.mensaje ?? error}`, 'error');
              }
            },
          },
          'Dar de baja',
        )),
      ),
    ),
  );
  document.body.append(dialogo);
  dialogo.addEventListener('close', () => dialogo.remove());
  return dialogo;
}

async function panelCuenta(contenedor, login, recargarCuentas) {
  const datos = await api.ajustesLeer(login);
  const cuenta = datos.cuenta;

  const campoIntervalo = h('input', {
    id: 'campo-intervalo',
    type: 'number',
    min: '5',
    step: '5',
    value: String(cuenta.intervalo_minutos),
  });
  const campoForks = h('input', { id: 'campo-forks', type: 'checkbox', checked: cuenta.alcance.incluir_forks });
  const campoOrganizaciones = h('input', {
    id: 'campo-organizaciones',
    type: 'text',
    value: cuenta.alcance.organizaciones.join(', '),
  });
  const campoCarpeta = h('input', { id: 'campo-carpeta', type: 'text', value: cuenta.carpeta, readonly: true });

  const mensajeGuardado = h('p', { clase: 'hint' });

  pintar(
    contenedor,
    h(
      'form',
      {
        clase: 'card pad',
        onSubmit: async (evento) => {
          evento.preventDefault();
          try {
            await api.ajustesGuardar(login, {
              intervalo_minutos: Number(campoIntervalo.value),
              carpeta: campoCarpeta.value,
              alcance: {
                incluir_forks: campoForks.checked,
                organizaciones: campoOrganizaciones.value
                  .split(',')
                  .map((s) => s.trim())
                  .filter(Boolean),
                excluidos: cuenta.alcance.excluidos,
              },
            });
            mensajeGuardado.textContent = 'Guardado.';
            avisar('Ajustes guardados.', 'success');
          } catch (error) {
            mensajeGuardado.textContent = `Error: ${error?.mensaje ?? error}`;
          }
        },
      },
      h('h2', null, `Cuenta: ${login}`),
      h('div', { clase: 'field' }, h('label', { for: 'campo-intervalo' }, 'Intervalo de sincronización (minutos)'), campoIntervalo),
      h('div', { clase: 'field' }, h('label', null, campoForks, ' Incluir forks')),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'campo-organizaciones' }, 'Organizaciones a incluir (separadas por comas)'),
        campoOrganizaciones,
      ),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'campo-carpeta' }, 'Carpeta de la cuenta'),
        h(
          'div',
          { clase: 'inline-form' },
          campoCarpeta,
          h(
            'button',
            {
              type: 'button',
              clase: 'btn ghost',
              onClick: async () => {
                const elegido = await api.elegirCarpeta();
                if (elegido?.carpeta) campoCarpeta.value = elegido.carpeta;
              },
            },
            'Elegir…',
          ),
        ),
        h('p', { clase: 'hint' }, 'Cambiar la carpeta mueve todo el contenido de la cuenta al guardar.'),
      ),
      mensajeGuardado,
      h(
        'div',
        { clase: 'row mt' },
        h('button', { type: 'submit', clase: 'btn' }, 'Guardar'),
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogoRotarToken(login).showModal() }, 'Rotar token'),
      ),
    ),
    panelLan(login),
    h(
      'div',
      { clase: 'zona-peligro mt' },
      h('h3', null, 'Dar de baja la cuenta'),
      h('p', null, 'Detiene la sincronización de forma permanente. No se puede deshacer desde la interfaz.'),
      h(
        'button',
        {
          clase: 'btn danger',
          onClick: () => dialogoBaja(login, recargarCuentas).showModal(),
        },
        'Dar de baja…',
      ),
    ),
  );
}

async function panelGlobal(contenedor) {
  const datos = await api.ajustesLeer(null);
  pintar(
    contenedor,
    h(
      'div',
      { clase: 'card pad' },
      h('h2', null, 'General'),
      h(
        'div',
        { clase: 'row between' },
        h('p', { clase: 'hint' }, `Gitea instalado: versión ${datos?.version_gitea ?? 'desconocida'}`),
        h(
          'button',
          {
            clase: 'btn ghost',
            onClick: async (evento) => {
              const liberar = ocupar(evento.currentTarget, 'Descargando y verificando…');
              try {
                const respuesta = await api.actualizarGitea();
                avisar(`Gitea actualizado a la versión ${respuesta.version}.`, 'success');
              } catch (error) {
                avisar(`No se pudo actualizar: ${error?.mensaje ?? error}`, 'error');
              } finally {
                liberar();
              }
            },
          },
          'Actualizar (verificado)',
        ),
      ),
      h('div', { clase: 'row between mt' }, h('span', { clase: 'hint' }, 'Tema de la interfaz'), crearConmutadorTema()),
    ),
  );
}

export async function render(contenedor) {
  const cuentas = await api.listarCuentas();

  const zonaCuenta = h('div', null);
  const zonaGlobal = h('div', { clase: 'mt' });

  async function recargar() {
    await render(contenedor);
  }

  if (cuentas.length === 0) {
    pintar(
      zonaCuenta,
      h(
        'div',
        { clase: 'card pad vacio' },
        h('h2', null, 'No hay cuentas todavía'),
        h('p', null, 'Añade una cuenta para configurar su intervalo, alcance y token.'),
        h('button', { clase: 'btn accent', onClick: () => navegar('alta') }, 'Añadir cuenta'),
      ),
    );
  } else {
    const estado = { login: cuentas[0].login };
    const contenedorForm = h('div');
    const selectorCuenta = h(
      'select',
      {
        id: 'sel-cuenta-ajustes',
        onChange: (evento) => {
          estado.login = evento.currentTarget.value;
          panelCuenta(contenedorForm, estado.login, recargar);
        },
      },
      cuentas.map((cuenta) => h('option', { value: cuenta.login }, cuenta.login)),
    );
    pintar(
      zonaCuenta,
      h('div', { clase: 'field mb' }, h('label', { for: 'sel-cuenta-ajustes' }, 'Cuenta'), selectorCuenta),
      contenedorForm,
    );
    await panelCuenta(contenedorForm, estado.login, recargar);
  }

  pintar(contenedor, h('h1', null, 'Ajustes'), zonaCuenta, zonaGlobal);
  await panelGlobal(zonaGlobal);
}
