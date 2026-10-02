# Changelog

Todos los cambios relevantes de gitmereba. Formato basado en
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); el proyecto sigue
[Semantic Versioning](https://semver.org/lang/es/).

## [Sin publicar]

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

[Sin publicar]: https://github.com/jparga/gitmereba/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/jparga/gitmereba/releases/tag/v0.6.0
