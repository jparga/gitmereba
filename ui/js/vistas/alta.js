import { api, suscribirProgresoAlta } from '../api.js';
import { h, pintar, tamanoLegible, avisar } from '../dom.js';
import { navegar } from '../router.js';
import { t, formatoSoloFecha } from '../i18n.js';

// Nunca se guarda un token en una variable de módulo ni de estado: cada paso lo lee
// directamente del <input> en el momento de invocar el comando correspondiente, y lo
// borra del DOM justo después, tanto si el comando tiene éxito como si falla. Por eso el
// asistente vuelve a pedir el token al pasar de la previsualización a la creación: es el
// precio de no retenerlo en memoria entre pasos (así lo fija el contrato con la
// interfaz en sus notas de seguridad).

function pasosAsistente(actual) {
  const nombres = [t('alta.paso.cuenta'), t('alta.paso.previa'), t('alta.paso.progreso')];
  return h(
    'ol',
    { clase: 'asistente-pasos' },
    nombres.map((nombre, i) => {
      const numero = i + 1;
      const clase = numero === actual ? 'activo' : numero < actual ? 'hecho' : '';
      return h('li', { clase }, h('span', { clase: 'numero' }, String(numero)), nombre);
    }),
  );
}

function ayudaToken() {
  return h(
    'details',
    { clase: 'disclosure mb' },
    h('summary', null, t('alta.ayuda.titulo')),
    h(
      'div',
      null,
      h(
        'p',
        null,
        t('alta.ayuda.pasos'),
        h('strong', null, t('alta.ayuda.permiso_contents')),
        t('alta.ayuda.y'),
        h('strong', null, t('alta.ayuda.permiso_metadata')),
        t('alta.ayuda.clasico'),
        h('strong', null, t('alta.ayuda.permiso_repo')),
        '.',
      ),
    ),
  );
}

async function pasoUno(contenedor, avanzar) {
  let entradaUsuario;
  let entradaToken;
  let entradaCarpeta;
  const mensaje = h('p', { clase: 'hint' });

  const comprobar = async (boton) => {
    const usuario = entradaUsuario.value.trim();
    const token = entradaToken.value;
    const carpeta = entradaCarpeta.value.trim();
    if (!usuario || !token || !carpeta) {
      mensaje.textContent = t('alta.falta_datos');
      return;
    }
    boton.disabled = true;
    mensaje.textContent = t('alta.comprobando');
    try {
      const previa = await api.validarAlta(usuario, token, carpeta);
      avanzar({ usuario, carpeta, previa });
    } catch (error) {
      mensaje.textContent = t('alta.validar.error', { mensaje: error?.mensaje ?? error });
    } finally {
      // Se borra tanto en éxito como en error: en éxito, el paso 2 vuelve a pedirlo
      // antes de crear la cuenta; nunca queda retenido en el DOM más de lo necesario.
      entradaToken.value = '';
      boton.disabled = false;
    }
  };

  pintar(
    contenedor,
    h('h2', null, t('alta.uno.titulo')),
    ayudaToken(),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-usuario' }, t('alta.usuario')),
      (entradaUsuario = h('input', { id: 'alta-usuario', type: 'text', autocomplete: 'off', required: true })),
    ),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-token' }, t('alta.token')),
      (entradaToken = h('input', { id: 'alta-token', type: 'password', autocomplete: 'off', required: true })),
      h('p', { clase: 'aviso-secreto' }, t('alta.token.aviso')),
    ),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-carpeta' }, t('alta.carpeta')),
      h(
        'div',
        { clase: 'inline-form' },
        (entradaCarpeta = h('input', { id: 'alta-carpeta', type: 'text', readonly: true, required: true })),
        h(
          'button',
          {
            type: 'button',
            clase: 'btn ghost',
            onClick: async () => {
              const elegida = await api.elegirCarpeta();
              if (elegida?.carpeta) entradaCarpeta.value = elegida.carpeta;
            },
          },
          t('alta.elegir'),
        ),
      ),
    ),
    mensaje,
    h(
      'div',
      { clase: 'row mt' },
      h('button', { clase: 'btn', onClick: (evento) => comprobar(evento.currentTarget) }, t('alta.comprobar')),
    ),
  );
}

function filaRepoPreview(repo, seleccionados) {
  const id = `previa-${repo.dueno}-${repo.nombre}`;
  const marcado = !repo.es_fork;
  if (marcado) seleccionados.add(`${repo.dueno}/${repo.nombre}`);
  return h(
    'li',
    { clase: 'row' },
    h('input', {
      id,
      type: 'checkbox',
      checked: marcado,
      onChange: (evento) => {
        const clave = `${repo.dueno}/${repo.nombre}`;
        if (evento.currentTarget.checked) seleccionados.add(clave);
        else seleccionados.delete(clave);
      },
    }),
    h('label', { for: id, clase: 'mono' }, `${repo.dueno}/${repo.nombre}`),
    repo.privado ? h('span', { clase: 'badge muted' }, t('repos.privado')) : null,
    repo.es_fork ? h('span', { clase: 'badge purple' }, t('repos.col.fork')) : null,
    h('span', { clase: 'hint' }, tamanoLegible(repo.tamano_kb)),
  );
}

async function pasoDos(contenedor, datosUno, avanzar, retroceder) {
  const { previa } = datosUno;
  const seleccionados = new Set();
  const porDueno = new Map();
  for (const repo of previa.repos) {
    if (!porDueno.has(repo.dueno)) porDueno.set(repo.dueno, []);
    porDueno.get(repo.dueno).push(repo);
  }

  const grupos = [...porDueno.entries()].map(([dueno, repos]) =>
    h('div', { clase: 'card pad mb' }, h('h3', null, dueno), h('ul', { clase: 'section' }, repos.map((r) => filaRepoPreview(r, seleccionados)))),
  );

  const caduca = previa.identidad.caduca ? formatoSoloFecha(new Date(previa.identidad.caduca)) : t('alta.previa.sin_caducidad');

  let entradaToken;
  const mensaje = h('p', { clase: 'hint' });

  pintar(
    contenedor,
    h('h2', null, t('alta.paso.previa')),
    h('p', null, t('alta.previa.identidad', { login: previa.identidad.login, caduca })),
    h('p', { clase: 'hint' }, t('alta.previa.descubiertos', { n: previa.repos.length, tamano: tamanoLegible(previa.total_tamano_kb) })),
    grupos,
    h('h3', { clase: 'mt' }, t('alta.confirmar.titulo')),
    h('p', null, t('alta.confirmar.texto')),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-token-confirmar' }, t('alta.confirmar.token')),
      (entradaToken = h('input', { id: 'alta-token-confirmar', type: 'password', autocomplete: 'off', required: true })),
      h('p', { clase: 'aviso-secreto' }, t('alta.confirmar.aviso')),
    ),
    mensaje,
    h(
      'div',
      { clase: 'row mt' },
      h('button', { clase: 'btn ghost', onClick: retroceder }, t('alta.atras')),
      h(
        'button',
        {
          clase: 'btn accent',
          onClick: async (evento) => {
            const token = entradaToken.value;
            entradaToken.value = '';
            if (!token) {
              mensaje.textContent = t('alta.confirmar.falta_token');
              return;
            }
            if (seleccionados.size === 0) {
              mensaje.textContent = t('alta.confirmar.falta_repos');
              return;
            }
            evento.currentTarget.disabled = true;
            avanzar({
              usuario: datosUno.usuario,
              carpeta: datosUno.carpeta,
              token,
              repos: [...seleccionados],
              organizaciones: previa.organizaciones,
            });
          },
        },
        t('alta.crear'),
      ),
    ),
  );
}

const ICONOS_PROGRESO_ESTADO = { 'en-curso': '…', ok: '✓', error: '✕' };

async function pasoTres(contenedor, datosDos) {
  const lista = h('ul', { clase: 'pasos-progreso' });
  const filas = new Map();
  const final = h('p', { clase: 'hint mt' }, t('alta.progreso.creando'));

  pintar(contenedor, h('h2', null, t('alta.progreso.titulo')), lista, final);

  const desuscribir = await suscribirProgresoAlta((evento) => {
    let fila = filas.get(evento.paso);
    if (!fila) {
      fila = h(
        'li',
        null,
        h('span', { clase: 'estado-icono' }, ICONOS_PROGRESO_ESTADO[evento.estado] ?? ''),
        h('span', null, evento.texto),
      );
      filas.set(evento.paso, fila);
      lista.append(fila);
    }
    fila.className = evento.estado;
    fila.querySelector('.estado-icono').textContent = ICONOS_PROGRESO_ESTADO[evento.estado] ?? '';
    if (evento.paso === evento.total - 1 && evento.estado === 'ok') {
      final.textContent = t('alta.progreso.creada');
      final.after(h('button', { clase: 'btn accent', onClick: () => navegar('resumen') }, t('alta.progreso.ir_resumen')));
    }
  });

  // `datosDos.token` solo vive en esta variable local el tiempo de esta llamada: no se
  // asigna a ningún campo de estado del asistente ni queda en un <input> (el paso 2 ya
  // lo borró del DOM en cuanto lo leyó).
  const { token, ...resto } = datosDos;
  try {
    await api.crearCuenta({ ...resto, token });
  } catch (error) {
    final.textContent = t('alta.progreso.error', { mensaje: error?.mensaje ?? error });
    avisar(t('alta.error', { mensaje: error?.mensaje ?? error }), 'error');
  }
  return desuscribir;
}

export async function render(contenedor) {
  let datosUno = null;
  let desuscribirProgreso = null;

  const marco = h('div');
  const cabecera = h('div');
  pintar(contenedor, cabecera, marco);

  function repintarCabecera(paso) {
    pintar(cabecera, h('h1', null, t('resumen.anadir_cuenta')), pasosAsistente(paso));
  }

  async function mostrarPasoUno() {
    repintarCabecera(1);
    await pasoUno(marco, async (datos) => {
      datosUno = datos;
      await mostrarPasoDos();
    });
  }

  async function mostrarPasoDos() {
    repintarCabecera(2);
    await pasoDos(marco, datosUno, mostrarPasoTres, mostrarPasoUno);
  }

  async function mostrarPasoTres(datosDos) {
    repintarCabecera(3);
    desuscribirProgreso = await pasoTres(marco, datosDos);
  }

  await mostrarPasoUno();

  return () => {
    if (typeof desuscribirProgreso === 'function') desuscribirProgreso();
  };
}
