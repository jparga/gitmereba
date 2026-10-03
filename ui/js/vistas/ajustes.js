import { api } from '../api.js';
import { h, pintar, avisar, ocupar } from '../dom.js';
import { crearConmutadorTema } from '../tema.js';
import { navegar } from '../router.js';
import { t } from '../i18n.js';
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
      h('h2', { id: 'titulo-rotar-token' }, t('ajustes.rotar.titulo', { login })),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'token-nuevo' }, t('ajustes.rotar.etiqueta')),
        (entrada = h('input', { id: 'token-nuevo', type: 'password', autocomplete: 'off', required: true })),
        h('p', { clase: 'aviso-secreto' }, t('ajustes.rotar.aviso')),
      ),
      mensaje,
      h(
        'div',
        { clase: 'dialogo-botones' },
        h('button', { clase: 'btn ghost', onClick: () => dialogo.close() }, t('ajustes.cancelar')),
        h(
          'button',
          {
            clase: 'btn accent',
            onClick: async (evento) => {
              const token = entrada.value;
              entrada.value = '';
              if (!token) {
                mensaje.textContent = t('ajustes.rotar.falta');
                return;
              }
              const liberar = ocupar(evento.currentTarget, t('ajustes.comprobando'));
              try {
                await api.rotarToken(login, token);
                avisar(t('ajustes.rotar.exito'), 'success');
                dialogo.close();
              } catch (error) {
                mensaje.textContent = t('ajustes.error', { mensaje: error?.mensaje ?? error });
              } finally {
                liberar();
              }
            },
          },
          t('ajustes.rotar.boton'),
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
      h('h2', { id: 'titulo-baja' }, t('ajustes.baja.titulo', { login })),
      h('p', null, t('ajustes.baja.texto')),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'confirmar-login' }, t('ajustes.baja.confirmar', { login })),
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
        h('button', { clase: 'btn ghost', onClick: () => dialogo.close() }, t('ajustes.cancelar')),
        (botonConfirmar = h(
          'button',
          {
            clase: 'btn danger',
            disabled: true,
            onClick: async () => {
              try {
                await api.bajaCuenta(login);
                dialogo.close();
                avisar(t('ajustes.baja.hecha', { login }), 'success');
                await onBaja();
              } catch (error) {
                avisar(t('ajustes.baja.error', { mensaje: error?.mensaje ?? error }), 'error');
              }
            },
          },
          t('ajustes.baja.boton'),
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
            mensajeGuardado.textContent = t('ajustes.guardado');
            avisar(t('ajustes.guardados'), 'success');
          } catch (error) {
            mensajeGuardado.textContent = t('ajustes.error', { mensaje: error?.mensaje ?? error });
          }
        },
      },
      h('h2', null, t('ajustes.cuenta.titulo', { login })),
      h('div', { clase: 'field' }, h('label', { for: 'campo-intervalo' }, t('ajustes.intervalo')), campoIntervalo),
      h('div', { clase: 'field' }, h('label', null, campoForks, ` ${t('ajustes.forks')}`)),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'campo-organizaciones' }, t('ajustes.organizaciones')),
        campoOrganizaciones,
      ),
      h(
        'div',
        { clase: 'field' },
        h('label', { for: 'campo-carpeta' }, t('ajustes.carpeta')),
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
            t('ajustes.elegir'),
          ),
        ),
        h('p', { clase: 'hint' }, t('ajustes.carpeta.ayuda')),
      ),
      mensajeGuardado,
      h(
        'div',
        { clase: 'row mt' },
        h('button', { type: 'submit', clase: 'btn' }, t('ajustes.guardar')),
        h('button', { type: 'button', clase: 'btn ghost', onClick: () => dialogoRotarToken(login).showModal() }, t('ajustes.rotar.boton')),
      ),
    ),
    panelLan(login),
    h(
      'div',
      { clase: 'zona-peligro mt' },
      h('h3', null, t('ajustes.baja.zona_titulo')),
      h('p', null, t('ajustes.baja.zona_texto')),
      h(
        'button',
        {
          clase: 'btn danger',
          onClick: () => dialogoBaja(login, recargarCuentas).showModal(),
        },
        t('ajustes.baja.abrir'),
      ),
    ),
  );
}

/** Fila con el selector de idioma: guarda la preferencia y recarga la ventana. */
async function filaIdioma() {
  let guardada = 'auto';
  try {
    guardada = (await api.idioma()).preferencia;
  } catch {
    // Sin dato del backend: se muestra «Automático».
  }
  const selector = h(
    'select',
    {
      id: 'sel-idioma',
      onChange: async () => {
        try {
          await api.fijarIdioma(selector.value);
          globalThis.location.reload();
        } catch (error) {
          selector.value = guardada;
          avisar(t('ajustes.idioma.error', { mensaje: error?.mensaje ?? error }), 'error');
        }
      },
    },
    h('option', { value: 'auto' }, t('ajustes.idioma.auto')),
    h('option', { value: 'es' }, t('ajustes.idioma.es')),
    h('option', { value: 'en' }, t('ajustes.idioma.en')),
  );
  selector.value = guardada;
  return h(
    'div',
    { clase: 'row between mt' },
    h('label', { clase: 'hint', for: 'sel-idioma' }, t('ajustes.idioma.etiqueta')),
    selector,
  );
}

async function panelGlobal(contenedor) {
  const datos = await api.ajustesLeer(null);
  const idioma = await filaIdioma();
  pintar(
    contenedor,
    h(
      'div',
      { clase: 'card pad' },
      h('h2', null, t('ajustes.general')),
      h(
        'div',
        { clase: 'row between' },
        h('p', { clase: 'hint' }, t('ajustes.gitea.version', { version: datos?.version_gitea ?? t('ajustes.gitea.desconocida') })),
        h(
          'button',
          {
            clase: 'btn ghost',
            onClick: async (evento) => {
              const liberar = ocupar(evento.currentTarget, t('ajustes.gitea.actualizando'));
              try {
                const respuesta = await api.actualizarGitea();
                avisar(t('ajustes.gitea.actualizado', { version: respuesta.version }), 'success');
              } catch (error) {
                avisar(t('ajustes.gitea.error', { mensaje: error?.mensaje ?? error }), 'error');
              } finally {
                liberar();
              }
            },
          },
          t('ajustes.gitea.actualizar'),
        ),
      ),
      h('div', { clase: 'row between mt' }, h('span', { clase: 'hint' }, t('ajustes.tema')), crearConmutadorTema()),
      idioma,
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
        h('h2', null, t('ajustes.vacio.titulo')),
        h('p', null, t('ajustes.vacio.texto')),
        h('button', { clase: 'btn accent', onClick: () => navegar('alta') }, t('ajustes.vacio.boton')),
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
      h('div', { clase: 'field mb' }, h('label', { for: 'sel-cuenta-ajustes' }, t('ajustes.cuenta')), selectorCuenta),
      contenedorForm,
    );
    await panelCuenta(contenedorForm, estado.login, recargar);
  }

  pintar(contenedor, h('h1', null, t('ajustes.titulo')), zonaCuenta, zonaGlobal);
  await panelGlobal(zonaGlobal);
}
