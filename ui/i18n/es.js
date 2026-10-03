// Diccionario español de la ventana (ver ui/js/i18n.js).
//
// FORMATO EXIGIDO (app/tests/i18n_ui.rs lo lee con un analizador de líneas, sin Node):
//   - un `export default { … }` con una entrada por línea: `'vista.elemento': 'texto',`
//   - la clave va entre comillas simples, sin comillas dentro, y la línea empieza por ella;
//   - el valor va entre comillas simples en esa misma línea (`\'` para una comilla);
//   - claves planas con prefijo de vista; plurales como `clave_one` y `clave_other`;
//   - `{nombre}` es un hueco que `t('clave', { nombre })` sustituye por texto plano.
// es.js y en.js deben tener exactamente las mismas claves.

export default {
  'nav.saltar': 'Saltar al contenido',
  'nav.marca': 'clon local de GitHub',
  'nav.vistas': 'Vistas de la aplicación',
  'nav.resumen': 'Resumen',
  'nav.repositorios': 'Repositorios',
  'nav.contingencia': 'Contingencia',
  'nav.actividad': 'Actividad',
  'nav.ajustes': 'Ajustes',
  'pie.privacidad': 'Todo ocurre en este equipo: nada sale a la red salvo hacia GitHub y tu Gitea local.',
  'github.comprobando': 'Comprobando GitHub…',
  'github.operativo': 'GitHub operativo',
  'github.degradado': 'GitHub degradado',
  'github.caido': 'GitHub caído',
  'github.desconocido': 'Estado de GitHub desconocido',
  'github.sin_datos': 'GitHub: sin datos',
  'estado.fallo': 'Fallo',
  'estado.obsoleto': 'Obsoleto',
  'estado.huerfano': 'Huérfano',
  'estado.contingencia': 'Contingencia',
  'estado.excluido': 'Excluido',
  'estado.ok': 'Correcto',
  'tema.auto': 'Tema: automático',
  'tema.claro': 'Tema: claro',
  'tema.oscuro': 'Tema: oscuro',
  'tema.cambiar': '{etiqueta}. Pulsa para cambiar.',
  'ajustes.idioma.etiqueta': 'Idioma de la interfaz',
  'ajustes.idioma.auto': 'Automático (sistema)',
  'ajustes.idioma.es': 'Español',
  'ajustes.idioma.en': 'English',
  'ajustes.idioma.error': 'No se pudo cambiar el idioma: {mensaje}',
};
