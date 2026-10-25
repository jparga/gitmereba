# Manual de gitmereba

**Español** · [English](manual.en.md)

gitmereba mantiene en tu equipo una copia viva de tus cuentas de GitHub. Si GitHub cae,
sigues trabajando contra tu copia y, cuando vuelve, devuelves los cambios.

Todo ocurre en tu equipo: cada cuenta tiene su propio servidor Gitea local, los tokens se
guardan en el llavero del sistema y nada se borra sin que lo decidas tú.

## 1. Requisitos

- Linux con escritorio (probado en Ubuntu), `git` y `gpgv` instalados.
- Un llavero del sistema activo (GNOME Keyring o KWallet).
- Un token de GitHub **de solo lectura**: GitHub → *Settings* → *Developer settings* →
  *Fine-grained tokens* → permisos de repositorio **Contents: Read** y **Metadata: Read**.

## 2. Instalación

Descarga el `.deb` de la página de
[versiones](https://github.com/jparga/gitmereba/releases) e instálalo con
`sudo apt install ./gitmereba_<versión>_amd64.deb`. Para compilarlo tú, instalar sin
permisos de administrador o fijar gitmereba al dock de Ubuntu, consulta
[`compilar.md`](compilar.md).

## 3. Primeros pasos

Abre gitmereba (`gitmereba` sin argumentos) y pulsa **Añadir cuenta**:

1. **Usuario** de GitHub, **token** y **carpeta** de destino (vacía o inexistente).
2. Revisa la lista de repositorios que se van a clonar y desmarca los que no quieras.
3. Pulsa **Crear cuenta**. gitmereba descarga y verifica Gitea, lo configura y crea las copias.

Si algo falla a mitad, repite el alta: continúa donde se quedó. Puedes añadir tantas cuentas
como quieras; cada una es independiente.

A partir de ahí la cuenta se sincroniza sola cada 30 minutos, **aunque la ventana esté
cerrada**, y recibirás una notificación de escritorio si algo falla.

## 4. Las pantallas

Cada pantalla tiene su propia ayuda: **¿Cómo funciona esta pantalla?** bajo el título, o
la tecla **F1**.

| Pantalla | Para qué sirve |
|---|---|
| **Resumen** | Estado de cada cuenta y de GitHub. Sincronizar ahora. Abrir Gitea. |
| **Repositorios** | Lista con estado por repositorio. Incluir o excluir. Dirección de clonado local. |
| **Contingencia** | Trabajar sin GitHub y devolver los cambios después. |
| **Actividad** | Historial de sincronizaciones y registro de auditoría. |
| **Ajustes** | Intervalo, alcance, carpeta de la cuenta, token, acceso desde la red local, baja, actualización de Gitea. |
| **Acerca de** | Desde el pie de la ventana. Versión, licencia y enlaces; versión de Gitea de cada cuenta; rutas y entorno. «Copiar para informe». |

### Entrar en Gitea

**Abrir Gitea** abre en el navegador el servidor local de la cuenta y pide usuario y
contraseña. Están en **Resumen → Usuario y contraseña**: el usuario es `gitmereba-admin` y la
contraseña es aleatoria, guardada en el llavero de tu sistema. Es una cuenta de
administración: no la compartas.

### Progreso

Al pulsar **Sincronizar ahora** verás una barra mientras gitmereba habla con GitHub. Si hay
repositorios nuevos, después aparece **Clonando repositorios: X de N** hasta que Gitea termina
de traerlos; los grandes pueden tardar varios minutos.

### Públicos, privados y forks

La columna **En GitHub** de Repositorios dice si el repositorio es público o privado *en
GitHub*. Tu copia local es **siempre privada**: en Gitea no se ve nada sin iniciar sesión,
aunque el original sea público.

Los forks no se clonan salvo que lo pidas. Aparecen en la lista como **no se clona**. Para
traerlos: Ajustes → **Incluir forks** → Guardar; después marca en Repositorios los que
quieras y pulsa **Sincronizar ahora** en Resumen.

### Estados de un repositorio

| Estado | Significado | Qué hacer |
|---|---|---|
| **Al día** | La copia coincide con GitHub | Nada |
| **Obsoleto** | Lleva demasiado sin sincronizar | «Sincronizar»; si persiste, mira Actividad |
| **Fallo** | La copia falla o no coincide con GitHub | Lee el detalle del repositorio |
| **Huérfano** | Ya no existe en GitHub | Se conserva en pausa; decide tú |
| **Contingencia** | Tiene una copia con escritura activa | Reconciliar cuando GitHub vuelva |
| **Excluido** | Lo has desmarcado | Vuelve a incluirlo cuando quieras |

## 5. Si GitHub cae: contingencia

1. En **Contingencia**, pulsa **Activar** en el repositorio que necesitas.
2. Copia el comando que aparece y ejecútalo en tu carpeta de trabajo:
   ```bash
   git remote add mereba <dirección que muestra la app>
   git push mereba main        # trabaja y envía con normalidad
   ```
3. Cuando GitHub vuelva, pulsa **Reconciliar** e introduce un token **con escritura**
   (*Contents: Read and write*). Se usa solo para ese envío y no se guarda.

gitmereba **nunca fuerza** un envío. Si GitHub recibió otros cambios mientras tanto, se
detiene y te lo dice: intégralos tú con `git pull`/`git merge` y vuelve a reconciliar.

## 6. Recuperar historia borrada

Antes de cada sincronización gitmereba guarda una captura de las ramas y etiquetas. Si en
GitHub alguien reescribe la historia (un *force-push*) o borra una rama, recibes un aviso y la
versión anterior sigue guardada en `<carpeta>/snapshots/`. Se conservan las 10 últimas
capturas y todas las de los últimos 30 días.

## 7. Compartir con otros equipos de tu red

Apagado por defecto. En **Ajustes → Acceso desde la red local** pulsa **Compartir en la red
local** (o `gitmereba cuenta lan --login <usuario> --activar`). La app te da tres cosas:

1. **La línea para `/etc/hosts`** de cada equipo, por ejemplo
   `192.168.1.40  jparga.gitmereba.internal`. En el propio equipo anfitrión es
   `127.0.0.1  jparga.gitmereba.internal`.
2. **El certificado** y el comando para que `git` confíe en él solo para esa dirección.
   Copia el fichero `.crt` al otro equipo y comprueba que su **huella SHA-256** coincide con
   la que muestra la app.
3. **La regla de cortafuegos** recomendada para limitar el puerto a tu red.

La dirección será `https://<usuario>.gitmereba.internal:<puerto>`. Siempre es HTTPS.

> Mientras está compartido, Gitea escucha en todas las redes a las que esté conectado el
> equipo. Desactívalo si te llevas el portátil a una red que no es la tuya.

### Usuarios para otras personas

Con la LAN activa, cada persona debería entrar con su propio usuario, no con
`gitmereba-admin`. En **Ajustes → Usuarios de la LAN** (debajo de «Acceso desde la red
local») tienes la lista y dos acciones:

- **Crear usuario**: escribe un nombre y confirma. Aparece un diálogo con el **usuario y la
  contraseña** que ha generado Gitea (32 caracteres) — **se muestra una sola vez**. Está
  oculta por defecto; **Mostrar** la revela y **Copiar** la lleva al portapapeles. Si cierras
  el diálogo sin apuntarla, no hay forma de recuperarla: solo queda eliminar el usuario y
  crear uno nuevo.
- **Eliminar** (con confirmación): borra el usuario de Gitea. Los commits que ya hubiera
  enviado no se tocan.

Desde la terminal es lo mismo con `gitmereba cuenta usuario` (ver §8).

Cada persona creada así es un usuario **restringido** de Gitea: solo ve lo que gitmereba le
da, sin necesitar la cuenta de administración:

- **Lectura** sobre todos los mirrors de las organizaciones normales.
- **Escritura** solo sobre los repositorios de contingencia (`contingencia-*`) — para poder
  enviar cambios cuando GitHub está caído (§5). Un `git push` a un mirror normal se rechaza:
  «mirror repository is read-only».

No hace falta activar nada más: tras cada sincronización y al activar una contingencia,
gitmereba reconcilia sola los permisos, así que una organización u organización de
contingencia nueva recibe su equipo y a todas las personas de la LAN. La contraseña **no la
guarda gitmereba** (ni en el llavero, ni en ficheros, ni en el registro); la propia persona
puede cambiarla luego desde la web de Gitea.

**Cómo clona la otra persona**: añade su línea de `/etc/hosts`, confía en el certificado con
el comando `git config` del panel LAN (ver arriba) y clona con su usuario y contraseña:

```bash
git clone https://<host>.internal:<puerto>/<dueño>/<repo>.git
```

Limitación conocida: un usuario restringido puede crear repositorios propios en ese Gitea
(ocupan disco local; no se sincronizan con GitHub ni con ningún otro sitio).

## 8. Línea de órdenes

```bash
gitmereba                         # abre la ventana
gitmereba cuenta add --login <usuario> --carpeta <ruta>   # pide el token sin mostrarlo
gitmereba cuenta list
gitmereba cuenta rm <usuario>                              # no borra los repositorios
gitmereba cuenta mover <usuario> <carpeta> [--si]          # mueve la carpeta de la cuenta
gitmereba cuenta lan --login <usuario> --activar | --desactivar | --estado
gitmereba cuenta usuario --login <usuario> --crear <nombre> | --eliminar <nombre> | --listar
gitmereba sync <usuario> | --todas [--simulacro]
gitmereba sync <usuario> --repo <dueño/nombre>              # reintenta un clonado que falló
gitmereba status [--json]
gitmereba doctor                  # comprueba el sistema y cada cuenta
```

## 9. Seguridad, en corto

- Los tokens viven en el llavero del sistema; nunca en ficheros, registros ni argumentos.
- Gitea se descarga de su web oficial y se comprueban su huella y su firma antes de usarlo.
- Las copias son siempre privadas, el registro de usuarios está cerrado y, salvo que lo
  compartas tú, Gitea solo escucha en tu equipo.
- Nada se borra solo: ni huérfanos, ni carpetas al dar de baja una cuenta.
- La auditoría es de solo añadir y está encadenada: **Actividad** te dice si está íntegra.

## 10. Si algo va mal

| Síntoma | Prueba |
|---|---|
| No sé qué falla | `gitmereba doctor`: dice qué comprobación falla y cómo arreglarla |
| Voy a abrir una incidencia | **Acerca de → Copiar para informe** y pégalo en ella: no lleva tokens ni contraseñas |
| «Token no válido o caducado» | **Ajustes → Rotar token** |
| «El llavero está bloqueado» | Desbloquea el llavero del sistema (inicia sesión en el escritorio) |
| Un repositorio en **Fallo**: «no ha sincronizado nunca» | Se cortó su primer clonado. Pulsa **Sincronizar** en su fila: se descarta la copia vacía y se vuelve a clonar |
| No se sincroniza con la ventana cerrada | `systemctl --user list-timers \| grep gitmereba` |
| Gitea no abre | `systemctl --user status gitmereba-gitea-<usuario>` |
| Otro equipo no conecta | Revisa su `/etc/hosts`, el cortafuegos del anfitrión y que use `https://` y el puerto |
| Alguien olvidó su contraseña de la LAN | No se puede recuperar: **Eliminar** ese usuario y **Crear usuario** de nuevo con el mismo nombre |

## 11. Copia de seguridad y desinstalación

Toda la cuenta está en su carpeta: copiarla entera (con la cuenta parada) es una copia de
seguridad completa. Para dejar de usar una cuenta, **Ajustes → Dar de baja**; la carpeta se
queda donde está hasta que la borres tú.

### Mover la carpeta de una cuenta

En **Ajustes → Carpeta de la cuenta** pulsa **Elegir…**, escoge otra y **Guardar**; o, desde
la terminal, `gitmereba cuenta mover <usuario> <carpeta>`. Pide confirmación, porque **la
carpeta antigua se borra**. La carpeta nueva debe ser una ruta absoluta que no exista o esté
vacía, tuya, y no puede estar dentro de la carpeta de otra cuenta ni de la propia (ni al
revés).

- Se para Gitea (la cuenta no está disponible mientras dura) y, si hay una sincronización en
  marcha, el movimiento se rechaza: espera a que termine.
- En el mismo disco es un simple cambio de nombre, instantáneo.
- A otro disco se copia todo mostrando el progreso, se **verifica** la copia (mismos ficheros,
  tamaños y permisos, mismas sumas SHA-256 de la base de datos y de la configuración, y
  `git fsck` de cada repositorio y de las copias de seguridad) y solo entonces se borra la
  antigua.
- Después se actualizan la configuración, las unidades de systemd y los repositorios de Gitea,
  y Gitea arranca en la carpeta nueva. Si algo falla antes de terminar, la cuenta queda como
  estaba (en la carpeta antigua, funcionando) y se te dice el motivo.
- Si ya está todo en la carpeta nueva pero no se puede borrar la antigua, se te avisa de qué
  ruta queda por borrar a mano.
- No se mueve a un destino con espacios al principio o al final de un componente ni a un punto
  de montaje (crea una subcarpeta dentro de ese disco y elígela).
- Mientras dura, ninguna otra operación (sincronización, contingencia, acceso LAN, baja) puede
  tocar esa cuenta, ni desde otra ventana ni desde el temporizador.

### Si un movimiento se interrumpe

Mientras se mueve una carpeta, gitmereba deja en la carpeta de datos
(`~/.local/share/gitmereba/`) un fichero `mover-en-curso-<usuario>.toml` con el origen, el
destino y el modo, y la ventana no se deja cerrar. Se borra solo al terminar o al deshacerlo
todo. Si el equipo se apaga o el proceso muere a mitad, el fichero se queda,
`gitmereba doctor` lo marca como fallo y gitmereba se niega a empezar otro movimiento de esa
cuenta. No se recupera solo: tienes que decidir qué carpeta es la buena.

1. Con Gitea parado (`systemctl --user stop gitmereba-gitea-<usuario>`), mira cuál de las dos
   carpetas del fichero contiene `gitea/data/gitea.db`. Si ya no existe el origen, los datos
   están en el destino; si no, siguen en el origen.
2. Si la buena es la **antigua**: borra la carpeta nueva y la `<destino>.gitmereba-parcial` si
   existe. La cuenta sigue apuntando a la carpeta antigua.
3. Si la buena es la **nueva**: edita `carpeta` en su `gitmereba.toml` y en `cuentas.toml`
   (`~/.local/share/gitmereba/`), y abre **Ajustes** una vez para que se reescriban las
   unidades de systemd.
4. Borra el fichero `mover-en-curso-<usuario>.toml` y arranca de nuevo con
   `systemctl --user restart gitmereba-gitea-<usuario>`. Comprueba con `gitmereba doctor`.

## 12. Idioma

gitmereba está disponible en español e inglés: la ventana, la ayuda integrada, la ayuda y los
mensajes de la línea de órdenes y las notificaciones.

- **De dónde sale el idioma.** Por defecto sigue al sistema, con la primera de `LC_ALL`,
  `LC_MESSAGES` y `LANG` que esté definida. Si no hay ninguna, o su valor es `C` o `POSIX`,
  gitmereba usa inglés.
- **Elegirlo tú.** En **Ajustes → Idioma de la interfaz** elige *Automático (sistema)*,
  *Español* o *English*. La elección se guarda en `$XDG_CONFIG_HOME/gitmereba/preferencias.toml`
  (por defecto `~/.config/gitmereba/preferencias.toml`) y manda sobre el sistema.
- **Comprobarlo.** `gitmereba doctor` muestra el idioma en uso y de dónde sale (el fichero de
  preferencias, una variable de entorno o el valor por defecto).
- **Qué sigue en español.** Los subcomandos y las opciones de la línea de órdenes
  (`gitmereba cuenta add`, `--activar`). El histórico de sincronización, el registro de
  auditoría y los logs también se guardan en español, sea cual sea el idioma de la ventana.
