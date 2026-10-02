// Datos de ejemplo para previsualizar la interfaz sin Tauri (fuera de la ventana de la
// app, p. ej. abriendo ui/index.html en un navegador o sirviéndola con un servidor
// estático). api.js usa este módulo solo cuando `window.__TAURI__` no existe.
//
// Las formas imitan lo que documenta el contrato con la interfaz. No hay red real: los
// `http://127.0.0.1:<puerto>` son ilustrativos (Gitea siempre escucha en local).

const ahora = Date.now();
const haceMin = (m) => new Date(ahora - m * 60_000).toISOString();
const haceHoras = (h) => new Date(ahora - h * 3_600_000).toISOString();
const haceDias = (d) => new Date(ahora - d * 86_400_000).toISOString();
const enDias = (d) => new Date(ahora + d * 86_400_000).toISOString();

function idRepo(dueno, nombre) {
  return { dueno, nombre };
}

function repo(dueno, nombre, resto) {
  return {
    id: idRepo(dueno, nombre),
    privado: false,
    es_fork: false,
    archivado: false,
    tamano_kb: 4096,
    estado: 'ok',
    incluido: true,
    ultima_sync: haceMin(5),
    url_clon_local: `http://127.0.0.1:${resto?.puerto ?? 3900}/${dueno}/${nombre}.git`,
    detalle: null,
    clonado: true,
    ...resto,
  };
}

const CUENTAS = {
  jparga: {
    login: 'jparga',
    carpeta: '/home/jparga/gitmereba/jparga',
    puerto: 3900,
    intervalo_minutos: 30,
    alcance: {
      incluir_forks: false,
      organizaciones: ['mereba-oss'],
      excluidos: [idRepo('jparga', 'viejo-privado')],
    },
  },
  'mereba-ci': {
    login: 'mereba-ci',
    carpeta: '/home/jparga/gitmereba/mereba-ci',
    puerto: 3901,
    intervalo_minutos: 60,
    alcance: { incluir_forks: true, organizaciones: [], excluidos: [] },
  },
};

const REPOS = {
  jparga: [
    // Fork que existe en GitHub pero no se clona: la cuenta no incluye forks.
    repo('jparga', 'yatl', {
      es_fork: true,
      estado: 'excluido',
      incluido: false,
      tamano_kb: 0,
      ultima_sync: null,
      omitido: 'fork',
    }),
    repo('jparga', 'gitmereba', { tamano_kb: 8192, ultima_sync: haceMin(4) }),
    repo('jparga', 'facturas-cli', { tamano_kb: 2048, ultima_sync: haceHoras(2) }),
    repo('jparga', 'viejo-privado', {
      privado: true,
      estado: 'excluido',
      incluido: false,
      ultima_sync: haceDias(40),
      detalle: 'Excluido por el usuario.',
    }),
    repo('jparga', 'legacy-tool', {
      estado: 'obsoleto',
      ultima_sync: haceDias(9),
      detalle: 'Sin sincronizar desde hace más del intervalo esperado.',
    }),
    repo('jparga', 'roto-ci', {
      estado: 'fallo',
      ultima_sync: haceHoras(6),
      detalle: 'Tiempo de espera agotado al sincronizar con GitHub.',
    }),
    repo('jparga', 'borrado-en-github', {
      estado: 'huerfano',
      ultima_sync: haceDias(15),
      detalle: 'Ya no existe en GitHub; se conserva en local.',
    }),
    repo('mereba-oss', 'panel', { tamano_kb: 1536, ultima_sync: haceMin(20), puerto: 3900 }),
    repo('jparga', 'incidente-2024', {
      estado: 'contingencia',
      tamano_kb: 12288,
      ultima_sync: haceDias(2),
      detalle: 'Convertido en repo con escritura durante una contingencia activa.',
    }),
  ],
  'mereba-ci': [
    // Existen en GitHub pero no se clonan: fork fuera del alcance y repo desmarcado en el alta.
    // (En 'mereba-ci' los forks sí están incluidos, así que aquí solo cabe un excluido.)
    repo('mereba-ci', 'pruebas-viejas', {
      estado: 'excluido',
      incluido: false,
      archivado: true,
      tamano_kb: 0,
      ultima_sync: null,
      omitido: 'excluido',
      puerto: 3901,
    }),
    // Recién dado de alta: Gitea aún lo está trayendo (termina a los pocos sondeos).
    repo('mereba-ci', 'pipeline-base', { tamano_kb: 0, ultima_sync: null, puerto: 3901, clonado: false }),
    repo('mereba-ci', 'pipeline-fork', {
      es_fork: true,
      tamano_kb: 900,
      ultima_sync: haceHoras(1),
      puerto: 3901,
    }),
    repo('mereba-ci', 'runner-imagenes', {
      estado: 'obsoleto',
      tamano_kb: 20480,
      ultima_sync: haceDias(6),
      puerto: 3901,
      detalle: 'Sin sincronizar desde hace más del intervalo esperado.',
    }),
    repo('mereba-ci', 'sandbox', {
      estado: 'fallo',
      tamano_kb: 512,
      ultima_sync: haceHoras(10),
      puerto: 3901,
      detalle: 'Error de autenticación al listar el repositorio de origen.',
    }),
  ],
};

const CADUCA_TOKEN = { jparga: enDias(9), 'mereba-ci': null };

const HISTORIAL = {
  jparga: [
    {
      id: 501,
      cuenta: 'jparga',
      inicio: haceMin(6),
      fin: haceMin(4),
      resultado: 'ok',
      creados: 0,
      huerfanos: 0,
      fallos: 0,
      resumen: 'Sincronización de «jparga»: 0 alta(s), 0 pausado(s), 0 reanudado(s), 0 ajuste(s) de intervalo, 1 omitido(s).',
      detalle_json: null,
    },
    {
      id: 500,
      cuenta: 'jparga',
      inicio: haceHoras(6),
      fin: haceHoras(6),
      resultado: 'con-fallos',
      creados: 0,
      huerfanos: 1,
      fallos: 1,
      resumen: 'Sincronización de «jparga»: 0 alta(s), 1 pausado(s), 0 reanudado(s), 0 ajuste(s) de intervalo, 1 omitido(s), 1 fallo(s).',
      detalle_json: null,
    },
    {
      id: 480,
      cuenta: 'jparga',
      inicio: haceDias(2),
      fin: haceDias(2),
      resultado: 'ok',
      creados: 1,
      huerfanos: 0,
      fallos: 0,
      resumen: 'Sincronización de «jparga»: 1 alta(s), 0 pausado(s), 0 reanudado(s), 0 ajuste(s) de intervalo, 0 omitido(s).',
      detalle_json: null,
    },
  ],
  'mereba-ci': [
    {
      id: 495,
      cuenta: 'mereba-ci',
      inicio: haceMin(35),
      fin: haceMin(30),
      resultado: 'con-fallos',
      creados: 0,
      huerfanos: 0,
      fallos: 1,
      resumen: 'Sincronización de «mereba-ci»: 0 alta(s), 0 pausado(s), 0 reanudado(s), 0 ajuste(s) de intervalo, 0 omitido(s), 1 fallo(s).',
      detalle_json: null,
    },
  ],
};

// Cadena de auditoría de ejemplo (hashes ilustrativos, no calculados de verdad: en la
// app real los genera `almacen::auditar`).
const AUDITORIA = [
  {
    id: 1,
    momento: haceDias(40),
    cuenta: 'jparga',
    accion: 'cuenta.alta',
    detalle: 'Alta de la cuenta jparga en /home/jparga/gitmereba/jparga (puerto 3900).',
    hash_anterior: '0'.repeat(64),
    hash: 'a1'.repeat(32),
  },
  {
    id: 2,
    momento: haceDias(38),
    cuenta: 'jparga',
    accion: 'repo.excluir',
    detalle: 'Excluye jparga/viejo-privado.',
    hash_anterior: 'a1'.repeat(32),
    hash: 'b2'.repeat(32),
  },
  {
    id: 3,
    momento: haceDias(20),
    cuenta: 'mereba-ci',
    accion: 'cuenta.alta',
    detalle: 'Alta de la cuenta mereba-ci en /home/jparga/gitmereba/mereba-ci (puerto 3901).',
    hash_anterior: 'b2'.repeat(32),
    hash: 'c3'.repeat(32),
  },
  {
    id: 4,
    momento: haceDias(2),
    cuenta: 'jparga',
    accion: 'contingencia.activar',
    detalle: 'Activa contingencia en jparga/incidente-2024.',
    hash_anterior: 'c3'.repeat(32),
    hash: 'd4'.repeat(32),
  },
  {
    id: 5,
    momento: haceHoras(6),
    cuenta: 'jparga',
    accion: 'sincronizacion.con-fallos',
    detalle: '1 huérfano, 1 fallo (roto-ci: tiempo de espera agotado).',
    hash_anterior: 'd4'.repeat(32),
    hash: 'e5'.repeat(32),
  },
];

function contadoresDe(login) {
  const repos = REPOS[login] ?? [];
  const contadores = { total: repos.length, ok: 0, obsoleto: 0, fallo: 0, huerfano: 0, contingencia: 0, excluido: 0 };
  for (const r of repos) contadores[r.estado] += 1;
  return contadores;
}

function estadoGlobalDe(login) {
  const contadores = contadoresDe(login);
  for (const estado of ['fallo', 'obsoleto', 'huerfano', 'contingencia']) {
    if (contadores[estado] > 0) return estado;
  }
  return 'ok';
}

async function esperar(ms) {
  return new Promise((resuelve) => setTimeout(resuelve, ms));
}

// Acceso desde la LAN por cuenta (login → host `.internal`), apagado por defecto.
const LAN = {};
const hostSugerido = (login) => `${login.toLowerCase().replace(/[^a-z0-9-]/g, '-')}.gitmereba.internal`;
const informeLan = (login) => {
  const host = LAN[login];
  const puerto = CUENTAS[login].puerto;
  const certificado = `${CUENTAS[login].carpeta}/gitea/tls/gitmereba-${login}.crt`;
  return {
    url_publica: `https://${host}:${puerto}`,
    host,
    ip_lan: '192.168.1.40',
    linea_hosts: `192.168.1.40  ${host}`,
    huella_sha256: '3A:7F:12:C4:9B:E0:55:D1:8E:2A:64:F7:0C:B3:91:4D:6E:A8:27:F0:1B:C5:73:9D:E2:48:06:BF:5A:94:D7:31',
    ruta_certificado: certificado,
    comando_git_cliente: `git config --global http."https://${host}:${puerto}/".sslCAInfo ~/gitmereba-${login}.crt`,
    regla_cortafuegos_sugerida: `sudo ufw allow from 192.168.1.0/24 to any port ${puerto} proto tcp`,
  };
};

// Usuarios de Gitea para el acceso desde la LAN, por cuenta (no incluyen el
// administrador). `jparga` trae un par de ejemplo; el resto empieza sin ninguno.
const USUARIOS_LAN = {
  jparga: ['ana', 'luis'],
  'mereba-ci': [],
};

const NOMBRE_USUARIO_LAN_VALIDO = /^[A-Za-z0-9_.-]{1,39}$/;

function nombreUsuarioLanValido(nombre) {
  return (
    typeof nombre === 'string' &&
    NOMBRE_USUARIO_LAN_VALIDO.test(nombre) &&
    nombre !== 'gitmereba-admin' &&
    !nombre.startsWith('contingencia-')
  );
}

// Contraseña de ejemplo de 32 caracteres, imitando lo que genera Gitea de verdad.
function passwordLanDeEjemplo() {
  const alfabeto = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789';
  let password = '';
  for (let i = 0; i < 32; i += 1) {
    password += alfabeto[Math.floor(Math.random() * alfabeto.length)];
  }
  return password;
}

// Simula el clonado de Gitea: tras unas cuantas consultas, el repo pendiente termina.
let consultasDeClonado = 0;
function clonarUnPocoMas(repos) {
  consultasDeClonado += 1;
  if (consultasDeClonado > 6) {
    for (const r of repos) {
      if (r.clonado === false) Object.assign(r, { clonado: true, tamano_kb: 3072, ultima_sync: new Date().toISOString() });
    }
  }
  return repos;
}

const PASOS_ALTA = [
  'Validar el token contra GitHub',
  'Descubrir repositorios',
  'Guardar el token en el llavero',
  'Crear la carpeta y el puerto, escribir app.ini',
  'Asegurar el binario de Gitea',
  'Provisión desatendida (base de datos, administrador, token de API)',
  'Instalar y arrancar las unidades systemd',
  'Crear organizaciones y mirrors',
];

const suscriptoresProgreso = new Set();

export function suscribirProgreso(callback) {
  suscriptoresProgreso.add(callback);
  return Promise.resolve(() => suscriptoresProgreso.delete(callback));
}

async function emitirProgresoAlta() {
  for (let paso = 0; paso < PASOS_ALTA.length; paso += 1) {
    for (const cb of suscriptoresProgreso) cb({ paso, total: PASOS_ALTA.length, texto: PASOS_ALTA[paso], estado: 'en-curso' });
    await esperar(280);
    for (const cb of suscriptoresProgreso) cb({ paso, total: PASOS_ALTA.length, texto: PASOS_ALTA[paso], estado: 'ok' });
  }
}

// Progreso de `sincronizar` (cuenta entera): mismo mecanismo de suscriptores que
// `suscribirProgreso`/`emitirProgresoAlta`, pero con su propio canal (`sync://progreso`
// es un evento distinto de `alta://progreso`).
const suscriptoresProgresoSync = new Set();

export function suscribirProgresoSync(callback) {
  suscriptoresProgresoSync.add(callback);
  return Promise.resolve(() => suscriptoresProgresoSync.delete(callback));
}

// Cuántas acciones simuladas se «aplican» en una pasada de ejemplo.
const ACCIONES_SYNC_DE_EJEMPLO = 3;

async function emitirProgresoSync(login) {
  const emitir = (fase, hechos, total) => {
    for (const cb of suscriptoresProgresoSync) cb({ login, fase, hechos, total });
  };
  emitir('listando', 0, 0);
  await esperar(300);
  emitir('aplicando', 0, ACCIONES_SYNC_DE_EJEMPLO);
  for (let hechos = 1; hechos <= ACCIONES_SYNC_DE_EJEMPLO; hechos += 1) {
    await esperar(250);
    emitir('aplicando', hechos, ACCIONES_SYNC_DE_EJEMPLO);
  }
  emitir('verificando', 0, 0);
}

/** Simula `invoke(comando, args)` con los mismos nombres que el contrato con la interfaz. */
export async function invocar(comando, args = {}) {
  await esperar(120);
  switch (comando) {
    case 'listar_cuentas':
      return Object.values(CUENTAS);

    case 'resumen_cuenta': {
      const cuenta = CUENTAS[args.login];
      if (!cuenta) throw { codigo: 'cuenta_no_encontrada', mensaje: `No existe la cuenta «${args.login}».` };
      const repos = REPOS[args.login] ?? [];
      return {
        cuenta,
        estado_global: estadoGlobalDe(args.login),
        contadores: contadoresDe(args.login),
        espacio_disco_kb: repos.reduce((suma, r) => suma + r.tamano_kb, 0),
        ultima_sincronizacion: (HISTORIAL[args.login] ?? [])[0]?.fin ?? null,
        caduca_token: CADUCA_TOKEN[args.login] ?? null,
      };
    }

    case 'listar_repos':
      if (!CUENTAS[args.login]) throw { codigo: 'cuenta_no_encontrada', mensaje: `No existe la cuenta «${args.login}».` };
      return clonarUnPocoMas(REPOS[args.login] ?? []);

    case 'excluir_repo': {
      const repos = REPOS[args.login] ?? [];
      const fila = repos.find((r) => r.id.dueno === args.id.dueno && r.id.nombre === args.id.nombre);
      if (fila) {
        fila.incluido = !args.excluido;
        fila.estado = args.excluido ? 'excluido' : 'ok';
      }
      return { ok: true };
    }

    case 'sincronizar':
      // Solo la sincronización de cuenta entera (sin `id`) tiene fases que informar: la
      // de un repo suelto (`id` presente) sigue siendo indeterminada.
      if (!args.id) emitirProgresoSync(args.login);
      await esperar(2500);
      return {
        cuenta: CUENTAS[args.login],
        inicio: new Date(Date.now() - 1500).toISOString(),
        fin: new Date().toISOString(),
        resultado: 'ok',
        resumen: `Sincronización de «${args.login}»: 0 alta(s), 0 pausado(s), 0 reanudado(s), 0 ajuste(s) de intervalo, 0 omitido(s).`,
      };

    case 'estado_github':
      return { estado: 'degradado', consultado: new Date().toISOString() };

    case 'elegir_carpeta':
      return { carpeta: '/home/jparga/gitmereba/nueva-cuenta' };

    case 'validar_alta':
      if (!args.token) throw { codigo: 'token_vacio', mensaje: 'Falta el token de lectura.' };
      return {
        identidad: { login: 'nueva-cuenta', scopes: [], caduca: enDias(90) },
        repos: [
          { dueno: 'nueva-cuenta', nombre: 'sitio-web', privado: false, es_fork: false, tamano_kb: 2048 },
          { dueno: 'nueva-cuenta', nombre: 'api-interna', privado: true, es_fork: false, tamano_kb: 5120 },
          { dueno: 'nueva-cuenta', nombre: 'fork-ajeno', privado: false, es_fork: true, tamano_kb: 256 },
        ],
        organizaciones: ['mereba-oss'],
        total_tamano_kb: 7424,
      };

    case 'crear_cuenta':
      emitirProgresoAlta();
      return { ok: true };

    case 'historial':
      return args.login ? (HISTORIAL[args.login] ?? []) : Object.values(HISTORIAL).flat();

    case 'auditoria':
      return AUDITORIA.filter((entrada) => !args.desde_id || entrada.id > args.desde_id).slice(0, args.limite ?? 50);

    case 'verificar_auditoria':
      return { integra: true };

    case 'estado_contingencia':
      return (REPOS[args.login] ?? [])
        .filter((r) => r.estado === 'contingencia')
        .map((r) => ({
          id: r.id,
          comando: `git remote add mereba http://127.0.0.1:${CUENTAS[args.login].puerto}/contingencia-${r.id.dueno}/${r.id.nombre}.git`,
          commits_de_mas: [
            { rama: 'main', commits: 3 },
            { rama: 'feature/parche-urgente', commits: 1 },
          ],
        }));

    case 'activar_contingencia':
      return { ok: true };

    case 'reconciliar':
      if (!args.token) throw { codigo: 'token_vacio', mensaje: 'Falta el token de escritura.' };
      return { resultado: 'ok', mensaje: 'Reconciliado: GitHub ya contiene todos los commits locales.' };

    case 'ajustes_leer':
      return { cuenta: CUENTAS[args.login], version_gitea: '1.27.3', snapshots: [] };

    case 'ajustes_guardar':
      Object.assign(CUENTAS[args.login], args.ajustes);
      return { ok: true };

    case 'rotar_token':
      if (!args.token) throw { codigo: 'token_vacio', mensaje: 'Falta el token nuevo.' };
      return { ok: true };

    case 'baja_cuenta':
      delete CUENTAS[args.login];
      delete REPOS[args.login];
      return { ok: true };

    case 'abrir_gitea':
      return { ok: true };

    case 'credenciales_gitea':
      return { usuario: 'gitmereba-admin', password: 'ejemplo-Xk3v9QpL2mZt7RwB' };

    case 'actualizar_gitea':
      return { ok: true, version: '1.27.3' };

    case 'lan_estado':
      return { activo: Boolean(LAN[args.login]), host_sugerido: hostSugerido(args.login), informe: LAN[args.login] ? informeLan(args.login) : null };

    case 'lan_activar': {
      const host = args.host ?? hostSugerido(args.login);
      if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*\.internal$/.test(host)) {
        throw { codigo: 'host_invalido', mensaje: 'El nombre debe ser un nombre de host en minúsculas que termine en «.internal».' };
      }
      LAN[args.login] = host;
      return informeLan(args.login);
    }

    case 'lan_desactivar':
      delete LAN[args.login];
      return { ok: true };

    case 'lan_usuarios_listar':
      return [...(USUARIOS_LAN[args.login] ?? [])].sort((a, b) => a.localeCompare(b, 'es')).map((nombre) => ({ nombre }));

    case 'lan_usuario_crear': {
      if (!nombreUsuarioLanValido(args.nombre)) {
        throw {
          codigo: 'usuario_no_valido',
          mensaje: `«${args.nombre}» no es un nombre de usuario válido para Gitea.`,
        };
      }
      const usuarios = USUARIOS_LAN[args.login] ?? (USUARIOS_LAN[args.login] = []);
      if (usuarios.includes(args.nombre)) {
        throw { codigo: 'usuario_ya_existe', mensaje: `Ya existe un usuario «${args.nombre}» en este Gitea.` };
      }
      usuarios.push(args.nombre);
      return { nombre: args.nombre, password: passwordLanDeEjemplo() };
    }

    case 'lan_usuario_eliminar': {
      const usuarios = USUARIOS_LAN[args.login] ?? [];
      const indice = usuarios.indexOf(args.nombre);
      if (indice >= 0) usuarios.splice(indice, 1);
      return null;
    }

    default:
      throw { codigo: 'comando_desconocido', mensaje: `Comando no reconocido: «${comando}».` };
  }
}
