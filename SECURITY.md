# Política de seguridad · Security policy

**Español** · [English](#english)

La seguridad es la prioridad de gitmereba: maneja tokens de GitHub, copias completas de
tus repositorios y un servidor Gitea que puede compartirse en la red local.

## Versiones con soporte

Mientras el proyecto esté por debajo de 1.0, solo la última versión publicada recibe
correcciones de seguridad.

## Cómo informar de una vulnerabilidad

**No abras un issue público.** Usa el aviso privado de GitHub:
[Report a vulnerability](https://github.com/jparga/gitmereba/security/advisories/new).

Incluye, si puedes:

- versión de gitmereba y distribución;
- qué puede hacer un atacante y en qué condiciones (local, LAN, remoto);
- pasos para reproducirlo o una prueba de concepto mínima;
- **nunca** tokens, contraseñas ni datos reales: sustitúyelos por valores inventados.

Compromiso de respuesta (proyecto mantenido por una sola persona):

- acuse de recibo en un máximo de 7 días;
- evaluación inicial en un máximo de 14 días;
- corrección y aviso publicado coordinados contigo; se te reconocerá en el aviso si lo deseas.

## Qué entra en el alcance

- Fuga de secretos (token de GitHub, credenciales de Gitea) a ficheros, logs, mensajes
  de error, argumentos de proceso o URLs.
- Ejecución de órdenes o inyección de argumentos en las llamadas a `git` o `gitea`.
- Acceso no autorizado al Gitea local o al compartido en la LAN.
- Pérdida o alteración silenciosa de repositorios, o de la cadena de auditoría.
- Descarga o instalación de un binario de Gitea sin verificar.

Fuera del alcance: vulnerabilidades de Gitea, GitHub o del llavero del sistema (infórmalas
a sus proyectos), y ataques que requieren control previo de tu sesión de usuario.

---

## English

Security is gitmereba's top priority: it handles GitHub tokens, full copies of your
repositories and a Gitea server that may be shared on your local network.

**Supported versions:** while below 1.0, only the latest release receives security fixes.

**Reporting:** do **not** open a public issue. Use GitHub's private reporting:
[Report a vulnerability](https://github.com/jparga/gitmereba/security/advisories/new).
Include the version, your distribution, impact and preconditions, and reproduction steps.
Never include real tokens, passwords or data.

**Response (single maintainer):** acknowledgement within 7 days, initial assessment within
14 days, fix and advisory coordinated with you, with credit if you want it.

**In scope:** secret leaks (files, logs, errors, process arguments, URLs); command or
argument injection into `git`/`gitea`; unauthorized access to the local or LAN Gitea;
silent loss or tampering of repositories or of the audit chain; installing an unverified
Gitea binary. **Out of scope:** vulnerabilities in Gitea, GitHub or the system keyring
(report them upstream) and attacks that already require control of your user session.
