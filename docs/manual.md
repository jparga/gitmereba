# Manual de gitmereba

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
| **Ajustes** | Intervalo, alcance, token, acceso desde la red local, baja, actualización de Gitea. |

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
