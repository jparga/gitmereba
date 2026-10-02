import { api, suscribirProgresoSync } from '../api.js';
import { h, pintar, badgeEstado, fechaRelativa, tamanoLegible, diasHasta, avisar, ocupar, barraProgreso } from '../dom.js';
import { navegar } from '../router.js';
import { dialogoCredenciales } from './credenciales.js';

// Cada cuántos milisegundos se vuelve a mirar el clonado mientras quede algo pendiente.
const SONDEO_MS = 5000;

function botonAnadirCuenta() {
  return h(
    'button',
    { clase: 'btn accent', onClick: () => navegar('alta') },
    'Añadir cuenta',
  );
}

function vistaVacia() {
  return h(
    'div',
    { clase: 'card pad vacio' },
    h('span', { clase: 'vacio-icono' }, 'g'),
    h('h2', null, 'Todavía no hay ninguna cuenta clonada'),
    h(
      'p',
      null,
      'gitmereba mantiene un clon local y operativo de tus cuentas de GitHub sobre un ',
      'Gitea propio. Añade una cuenta con un token de solo lectura para empezar.',
    ),
    botonAnadirCuenta(),
  );
}

function filaAviso(cuenta, resumen) {
  const dias = diasHasta(resumen.caduca_token);
  if (dias === null || dias > 14) return null;
  const texto =
    dias <= 0
      ? `El token de «${cuenta.login}» ha caducado.`
      : `El token de «${cuenta.login}» caduca en ${dias} día(s).`;
  return h('div', { clase: 'banner show warn' }, h('span', null, texto));
}

function stat(clave, valor, claseValor) {
  return h(
    'div',
    { clase: 'stat' },
    h('div', { clase: 'k' }, clave),
    h('div', { clase: `v tabular ${claseValor ?? ''}` }, String(valor)),
  );
}

/** Repos incluidos de la cuenta y cuántos ha terminado ya de traer Gitea. */
function contarClonado(repos) {
  const incluidos = repos.filter((repo) => repo.incluido);
  return { total: incluidos.length, clonados: incluidos.filter((repo) => repo.clonado !== false).length };
}

/** Texto (y, si procede, `hechos`/`total`) de la barra mientras dura la pasada de
 * `sincronizar`, según el último evento `sync://progreso` recibido para esta cuenta
 * (`null` si aún no ha llegado ninguno: la fase «listando» inicial). */
function textoProgresoSync(progresoSync) {
  if (!progresoSync) return { texto: 'Sincronizando con GitHub…' };
  const { fase, hechos, total } = progresoSync;
  if (fase === 'listando') return { texto: 'Consultando GitHub…' };
  if (fase === 'aplicando' && total > 0) {
    return { texto: `Aplicando cambios: ${hechos} de ${total}`, hechos, total };
  }
  if (fase === 'verificando') return { texto: 'Verificando la copia…' };
  // «aplicando» sin ninguna acción que aplicar (plan vacío): sigue sin saberse cuánto va
  // a tardar el resto de la pasada.
  return { texto: 'Sincronizando con GitHub…' };
}

function tarjetaCuenta(cuenta, resumen, contenedor, vigilancia) {
  const c = resumen.contadores;
  const zonaProgreso = h('div', { 'aria-live': 'polite' });
  let sincronizando = false;
  let progresoSync = null;

  // Hay dos esperas distintas: la pasada de gitmereba (con avance real, vía
  // `sync://progreso`) y el clonado que hace Gitea después (se sabe: X de N).
  const pintarProgreso = async () => {
    let clonado = { total: 0, clonados: 0 };
    try {
      clonado = contarClonado(await api.listarRepos(cuenta.login));
    } catch {
      // Sin listado no se pinta la barra de clonado; la de la pasada no depende de él.
    }
    const pendiente = clonado.clonados < clonado.total;
    if (pendiente) {
      pintar(zonaProgreso, barraProgreso(`Clonando repositorios: ${clonado.clonados} de ${clonado.total}`, clonado.clonados, clonado.total));
    } else if (sincronizando) {
      const { texto, hechos, total } = textoProgresoSync(progresoSync);
      pintar(zonaProgreso, barraProgreso(texto, hechos, total));
    } else {
      pintar(zonaProgreso);
    }
    return pendiente;
  };

  vigilancia.push(async () => (await pintarProgreso()) || sincronizando);

  const sincronizar = async (boton) => {
    const liberar = ocupar(boton, 'Sincronizando…');
    sincronizando = true;
    progresoSync = null;
    pintar(zonaProgreso, barraProgreso('Sincronizando con GitHub…'));
    // Una tarjeta por cuenta: se filtra por `login` para no pintar el avance de otra.
    const desuscribir = await suscribirProgresoSync((evento) => {
      if (evento.login !== cuenta.login || !sincronizando) return;
      progresoSync = evento;
      const { texto, hechos, total } = textoProgresoSync(progresoSync);
      pintar(zonaProgreso, barraProgreso(texto, hechos, total));
    });
    try {
      await api.sincronizar(cuenta.login);
      avisar(`Sincronización de «${cuenta.login}» completada.`, 'success');
      sincronizando = false;
      await pintarTarjetas(contenedor, vigilancia.reiniciar);
    } catch (error) {
      avisar(`No se pudo sincronizar «${cuenta.login}»: ${error?.mensaje ?? error}`, 'error');
    } finally {
      sincronizando = false;
      progresoSync = null;
      desuscribir();
      liberar();
      pintarProgreso();
    }
  };

  const tarjeta = h(
    'article',
    { clase: 'card pad tarjeta-cuenta' },
    h(
      'div',
      { clase: 'cabecera' },
      h(
        'div',
        null,
        h('h3', null, cuenta.login),
        h('div', { clase: 'carpeta' }, cuenta.carpeta),
      ),
      badgeEstado(resumen.estado_global),
    ),
    filaAviso(cuenta, resumen),
    h(
      'div',
      { clase: 'grid stats' },
      stat('Mirrors', c.total),
      stat('Obsoletos', c.obsoleto, c.obsoleto > 0 ? 'warn' : ''),
      stat('Fallos', c.fallo, c.fallo > 0 ? 'crit' : ''),
      stat('Huérfanos', c.huerfano, c.huerfano > 0 ? 'muted' : ''),
    ),
    h(
      'p',
      { clase: 'hint mt' },
      `${tamanoLegible(resumen.espacio_disco_kb)} en disco · última sincronización ${
        resumen.ultima_sincronizacion ? fechaRelativa(resumen.ultima_sincronizacion) : 'sin datos'
      }`,
    ),
    zonaProgreso,
    h(
      'div',
      { clase: 'row acciones' },
      h('button', { clase: 'btn', onClick: (evento) => sincronizar(evento.currentTarget) }, 'Sincronizar ahora'),
      h(
        'button',
        {
          clase: 'btn ghost',
          onClick: async () => {
            try {
              await api.abrirGitea(cuenta.login);
            } catch (error) {
              avisar(`No se pudo abrir Gitea: ${error?.mensaje ?? error}`, 'error');
            }
          },
        },
        'Abrir Gitea',
      ),
      h('button', { clase: 'btn ghost', onClick: () => dialogoCredenciales(cuenta.login) }, 'Usuario y contraseña'),
    ),
  );
  return tarjeta;
}

async function pintarTarjetas(contenedor, reiniciarVigilancia) {
  const cuentas = await api.listarCuentas();
  if (cuentas.length === 0) {
    pintar(contenedor, vistaVacia());
    return;
  }
  const resumenes = await Promise.all(cuentas.map((cuenta) => api.resumenCuenta(cuenta.login)));
  const vigilancia = reiniciarVigilancia();
  const tarjetas = cuentas.map((cuenta, i) => tarjetaCuenta(cuenta, resumenes[i], contenedor, vigilancia));
  pintar(
    contenedor,
    h(
      'div',
      { clase: 'toolbar mb' },
      h('h1', null, 'Resumen'),
      botonAnadirCuenta(),
    ),
    h('div', { clase: 'grid cols-2' }, tarjetas),
  );
  vigilancia.arrancar();
}

export async function render(contenedor) {
  let temporizador = null;
  let viva = true;

  // Una «vigilancia» por pintado: la lista de comprobaciones de cada tarjeta. Se repite
  // cada SONDEO_MS mientras alguna diga que queda trabajo, y muere al salir de la vista.
  const reiniciarVigilancia = () => {
    clearTimeout(temporizador);
    const comprobaciones = [];
    comprobaciones.reiniciar = reiniciarVigilancia;
    comprobaciones.arrancar = async () => {
      if (!viva) return;
      const pendientes = await Promise.all(comprobaciones.map((comprobar) => comprobar()));
      if (viva && pendientes.some(Boolean)) temporizador = setTimeout(comprobaciones.arrancar, SONDEO_MS);
    };
    return comprobaciones;
  };

  await pintarTarjetas(contenedor, reiniciarVigilancia);
  return () => {
    viva = false;
    clearTimeout(temporizador);
  };
}
