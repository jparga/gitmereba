// Diccionario inglés de la ventana (ver ui/js/i18n.js).
//
// FORMATO EXIGIDO (app/tests/i18n_ui.rs lo lee con un analizador de líneas, sin Node):
//   - un `export default { … }` con una entrada por línea: `'vista.elemento': 'texto',`
//   - la clave va entre comillas simples, sin comillas dentro, y la línea empieza por ella;
//   - el valor va entre comillas simples en esa misma línea (`\'` para una comilla);
//   - claves planas con prefijo de vista; plurales como `clave_one` y `clave_other`;
//   - `{nombre}` es un hueco que `t('clave', { nombre })` sustituye por texto plano.
// es.js y en.js deben tener exactamente las mismas claves.

export default {
  'nav.saltar': 'Skip to content',
  'nav.marca': 'local GitHub clone',
  'nav.vistas': 'Application views',
  'nav.resumen': 'Summary',
  'nav.repositorios': 'Repositories',
  'nav.contingencia': 'Contingency',
  'nav.actividad': 'Activity',
  'nav.ajustes': 'Settings',
  'pie.privacidad': 'Everything happens on this machine: nothing leaves it except traffic to GitHub and your local Gitea.',
  'github.comprobando': 'Checking GitHub…',
  'github.operativo': 'GitHub operational',
  'github.degradado': 'GitHub degraded',
  'github.caido': 'GitHub down',
  'github.desconocido': 'GitHub status unknown',
  'github.sin_datos': 'GitHub: no data',
  'estado.fallo': 'Failed',
  'estado.obsoleto': 'Outdated',
  'estado.huerfano': 'Orphan',
  'estado.contingencia': 'Contingency',
  'estado.excluido': 'Excluded',
  'estado.ok': 'OK',
  'tema.auto': 'Theme: automatic',
  'tema.claro': 'Theme: light',
  'tema.oscuro': 'Theme: dark',
  'tema.cambiar': '{etiqueta}. Press to change.',
  'ajustes.idioma.etiqueta': 'Interface language',
  'ajustes.idioma.auto': 'Automatic (system)',
  'ajustes.idioma.es': 'Español',
  'ajustes.idioma.en': 'English',
  'ajustes.idioma.error': 'Could not change the language: {mensaje}',
};
