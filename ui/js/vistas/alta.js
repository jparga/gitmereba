import { api, suscribirProgresoAlta } from '../api.js';
import { h, pintar, tamanoLegible, avisar } from '../dom.js';
import { navegar } from '../router.js';

// Nunca se guarda un token en una variable de módulo ni de estado: cada paso lo lee
// directamente del <input> en el momento de invocar el comando correspondiente, y lo
// borra del DOM justo después, tanto si el comando tiene éxito como si falla. Por eso el
// asistente vuelve a pedir el token al pasar de la previsualización a la creación: es el
// precio de no retenerlo en memoria entre pasos (así lo fija el contrato con la
// interfaz en sus notas de seguridad).

function pasosAsistente(actual) {
  const nombres = ['Cuenta y token', 'Previsualización', 'Progreso'];
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
    h('summary', null, 'Cómo crear un token de solo lectura'),
    h(
      'div',
      null,
      h(
        'p',
        null,
        'En GitHub: Settings → Developer settings → Personal access tokens → Fine-grained tokens → ',
        'Generate new token. Limita el repositorio o la organización que quieras clonar y, en ',
        '«Repository permissions», concede solo lectura a ',
        h('strong', null, 'Contents'),
        ' y ',
        h('strong', null, 'Metadata'),
        '. También se admite un token clásico con el permiso ',
        h('strong', null, 'repo'),
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
      mensaje.textContent = 'Rellena usuario, token y carpeta de destino.';
      return;
    }
    boton.disabled = true;
    mensaje.textContent = 'Comprobando…';
    try {
      const previa = await api.validarAlta(usuario, token, carpeta);
      avanzar({ usuario, carpeta, previa });
    } catch (error) {
      mensaje.textContent = `No se pudo validar: ${error?.mensaje ?? error}`;
    } finally {
      // Se borra tanto en éxito como en error: en éxito, el paso 2 vuelve a pedirlo
      // antes de crear la cuenta; nunca queda retenido en el DOM más de lo necesario.
      entradaToken.value = '';
      boton.disabled = false;
    }
  };

  pintar(
    contenedor,
    h('h2', null, 'Usuario de GitHub y token de lectura'),
    ayudaToken(),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-usuario' }, 'Usuario de GitHub'),
      (entradaUsuario = h('input', { id: 'alta-usuario', type: 'text', autocomplete: 'off', required: true })),
    ),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-token' }, 'Token de lectura (PAT)'),
      (entradaToken = h('input', { id: 'alta-token', type: 'password', autocomplete: 'off', required: true })),
      h('p', { clase: 'aviso-secreto' }, 'No se guarda en la interfaz: se borra en cuanto se usa.'),
    ),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-carpeta' }, 'Carpeta de destino'),
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
          'Elegir…',
        ),
      ),
    ),
    mensaje,
    h(
      'div',
      { clase: 'row mt' },
      h('button', { clase: 'btn', onClick: (evento) => comprobar(evento.currentTarget) }, 'Comprobar'),
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
    repo.privado ? h('span', { clase: 'badge muted' }, 'Privado') : null,
    repo.es_fork ? h('span', { clase: 'badge purple' }, 'Fork') : null,
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

  const caduca = previa.identidad.caduca ? new Date(previa.identidad.caduca).toLocaleDateString('es-ES') : 'sin caducidad informada';

  let entradaToken;
  const mensaje = h('p', { clase: 'hint' });

  pintar(
    contenedor,
    h('h2', null, 'Previsualización'),
    h('p', null, `Identidad validada: ${previa.identidad.login}. Token caduca: ${caduca}.`),
    h('p', { clase: 'hint' }, `${previa.repos.length} repositorio(s) descubiertos, ${tamanoLegible(previa.total_tamano_kb)} en total.`),
    grupos,
    h('h3', { clase: 'mt' }, 'Confirmar y crear'),
    h('p', null, 'Vuelve a escribir el token de lectura para crear la cuenta.'),
    h(
      'div',
      { clase: 'field' },
      h('label', { for: 'alta-token-confirmar' }, 'Token de lectura'),
      (entradaToken = h('input', { id: 'alta-token-confirmar', type: 'password', autocomplete: 'off', required: true })),
      h('p', { clase: 'aviso-secreto' }, 'No se guarda: se borra en cuanto se envía.'),
    ),
    mensaje,
    h(
      'div',
      { clase: 'row mt' },
      h('button', { clase: 'btn ghost', onClick: retroceder }, 'Atrás'),
      h(
        'button',
        {
          clase: 'btn accent',
          onClick: async (evento) => {
            const token = entradaToken.value;
            entradaToken.value = '';
            if (!token) {
              mensaje.textContent = 'Escribe el token para continuar.';
              return;
            }
            if (seleccionados.size === 0) {
              mensaje.textContent = 'Selecciona al menos un repositorio.';
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
        'Crear cuenta',
      ),
    ),
  );
}

const ETIQUETAS_PROGRESO_ESTADO = { 'en-curso': 'En curso…', ok: 'Hecho', error: 'Error' };
const ICONOS_PROGRESO_ESTADO = { 'en-curso': '…', ok: '✓', error: '✕' };

async function pasoTres(contenedor, datosDos) {
  const lista = h('ul', { clase: 'pasos-progreso' });
  const filas = new Map();
  const final = h('p', { clase: 'hint mt' }, 'Creando la cuenta…');

  pintar(contenedor, h('h2', null, 'Creando la cuenta'), lista, final);

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
      final.textContent = 'Cuenta creada. Ya puedes verla en Resumen.';
      final.after(h('button', { clase: 'btn accent', onClick: () => navegar('resumen') }, 'Ir a Resumen'));
    }
  });

  // `datosDos.token` solo vive en esta variable local el tiempo de esta llamada: no se
  // asigna a ningún campo de estado del asistente ni queda en un <input> (el paso 2 ya
  // lo borró del DOM en cuanto lo leyó).
  const { token, ...resto } = datosDos;
  try {
    await api.crearCuenta({ ...resto, token });
  } catch (error) {
    final.textContent = `Error al crear la cuenta: ${error?.mensaje ?? error}`;
    avisar(`No se pudo completar el alta: ${error?.mensaje ?? error}`, 'error');
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
    pintar(cabecera, h('h1', null, 'Añadir cuenta'), pasosAsistente(paso));
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
