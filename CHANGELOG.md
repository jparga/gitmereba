# Changelog

Todos los cambios relevantes de gitmereba. Formato basado en
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); el proyecto sigue
[Semantic Versioning](https://semver.org/lang/es/).

## [Sin publicar]

## [0.9.0] - 2026-10-25

### Añadido

- **AppImage.** Cada versión incluye un `.AppImage` (x86_64) además del `.deb` y el `.tar.gz`:
  lleva GTK y WebKitGTK dentro y se ejecuta sin instalar. `scripts/empaquetar.sh --appimage` lo
  construye con `linuxdeploy`, su plugin gtk, `appimagetool` y el runtime type2, en versiones
  concretas con SHA-256 fijado. Los temporizadores de systemd apuntan al propio fichero
  `.AppImage` y no al montaje temporal, y extraen el AppImage en un directorio privado (0700) en
  vez del `/tmp` compartido. Dentro de un AppImage la ventana no se abre si no puede fijar su
  directorio de trabajo con seguridad: WebKitGTK localiza sus procesos auxiliares por una ruta
  relativa a él.

### Seguridad

- La navegación de la ventana solo acepta rutas propias: `#/constructor` o `#/__proto__` ya no
  resuelven a funciones heredadas de `Object`.

### Cambiado

- Dependencias al día: Tauri 2.12.2, `tauri-plugin-dialog` 2.8.3, `serde_json`, `sha2`, `zeroize`
  y `notify-rust`.

## [0.8.1] - 2026-10-25

### Seguridad

- Actualizadas dependencias con avisos de seguridad: `rustls` 0.23.46 (exposición de secretos TLS
  en `Debug`, degradación del intercambio de claves, datos enviados tras un rechazo de ECH y
  descompresión de certificados sin límite), `rustls-webpki` 0.103.16 (intermedios desconfiados
  aceptados), `ctutils` 0.4.5 (comparaciones en tiempo no constante), `tokio` 1.54.1
  (`AsyncFd::new` incumplía la seguridad de E/S) y `webpki-root-certs` 1.0.11 (raíces que daban
  por buenos intermedios emSign desconfiados).

## [0.8.0] - 2026-10-04

### Añadido

- **Versión en el pie y pantalla «Acerca de».** El pie de la ventana muestra la versión y lleva a
  «Acerca de», con la licencia, enlaces al repositorio, a las notas de la versión, a la política de
  seguridad y a las marcas; la versión de Gitea incluida y la que usa cada cuenta (marca las
  desfasadas); y las rutas, el ejecutable, el idioma y el sistema. «Copiar para informe» lo copia
  todo en texto plano para una incidencia, sin secretos y con las rutas personales como `~`.

### Corregido

- `doctor` escribe el resumen de capturas con plural real («1 captura», «89 capturas»).

## [0.7.1] - 2026-10-03

### Corregido

- Los paquetes `.deb` y `.tar.gz` incluyen el manual en inglés (`manual.en.md`); el `.tar.gz` incluye también el español,
  y `instalar-local.sh` los instala en `~/.local/share/doc/gitmereba/`.

## [0.7.0] - 2026-10-03

### Añadido

- **Interfaz en inglés y español.** La ventana, la CLI (ayudas y salida), `doctor`, los avisos de
  escritorio, los mensajes de error y la ayuda integrada están en los dos idiomas. El idioma sale
  del sistema (`LC_ALL`, `LC_MESSAGES`, `LANG`; sin ellos, o con `C`/`POSIX`, inglés) y se puede
  fijar en Ajustes. La preferencia se guarda en `$XDG_CONFIG_HOME/gitmereba/preferencias.toml`, y
  `doctor` muestra qué idioma se usa y de dónde sale.
- Manual de usuario en inglés (`docs/manual.en.md`) y capturas en inglés en el README.
- La entrada de escritorio tiene nombre y descripción en inglés y en español.
- Nota: los subcomandos y opciones de la CLI siguen en español en los dos idiomas. El histórico de
  sincronización, el registro de auditoría y los logs se guardan en español.

## [0.6.0] - 2026-10-02

Primera versión pública, bajo la licencia GPL-3.0-or-later.

### Qué incluye

- **Clon local por cuenta.** Un Gitea nativo (sin Docker) por cuenta de GitHub, descargado
  y verificado con SHA-256 y firma GPG, con configuración endurecida y servicio
  `systemd --user`.
- **Sincronización periódica** con la ventana cerrada: altas, exclusiones y huérfanos que
  nunca se borran, reintento del clonado inicial fallido, verificación de SHAs y
  `git fsck`, y avisos de escritorio ante fallos o un token a punto de caducar.
- **Snapshots** de ramas y etiquetas antes de cada pasada, que detectan la historia
  reescrita en GitHub y permiten recuperarla.
- **Contingencia.** Si GitHub cae, puedes trabajar contra una copia con escritura y después
  reconciliar sin forzar nada: si hay divergencia, se detiene.
- **Acceso desde la LAN**, opcional, por HTTPS con certificado propio y nombre `.internal`,
  con usuarios restringidos para otras personas.
- **Ventana Tauri 2** (Resumen, Repositorios, Contingencia, Actividad, Ajustes) con ayuda
  contextual, y **CLI completa** (`cuenta`, `sync`, `status`, `doctor`).
- **Seguridad.** Los secretos solo están en el llavero del sistema y nunca en ficheros,
  logs, errores, argumentos ni URLs. No se usa shell. La auditoría es de solo añadir y
  encadenada con SHA-256. `unsafe` está prohibido, y `cargo deny` y `cargo audit` se
  ejecutan en la CI.
- **Paquete `.deb`** y `.tar.gz`, con `SHA256SUMS` y atestación de procedencia.

[Sin publicar]: https://github.com/jparga/gitmereba/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jparga/gitmereba/compare/v0.8.1...v0.9.0
[0.8.1]: https://github.com/jparga/gitmereba/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/jparga/gitmereba/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/jparga/gitmereba/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/jparga/gitmereba/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/jparga/gitmereba/releases/tag/v0.6.0
