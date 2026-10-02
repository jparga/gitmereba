# Compilar e instalar desde el código

Lo normal es instalar el paquete `.deb` de la página de
[versiones](https://github.com/jparga/gitmereba/releases). Esta guía es para compilarlo tú.

## 1. Requisitos

Linux con escritorio (probado en Ubuntu 24.04), `git` y las bibliotecas de desarrollo de
Tauri 2:

```bash
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev libsoup-3.0-dev libgtk-3-dev librsvg2-dev libdbus-1-dev
```

La versión de Rust la fija `rust-toolchain.toml`. Con [rustup](https://rustup.rs) se
instala sola en la primera compilación.

## 2. Compilar

```bash
cargo build --release -p gitmereba      # deja el binario en target/release/gitmereba
```

## 3. Generar e instalar el paquete

```bash
scripts/empaquetar.sh          # compila y construye target/paquetes/gitmereba_<versión>_amd64.deb
scripts/empaquetar.sh --tar    # además, un .tar.gz genérico para distribuciones sin dpkg

sudo apt install ./target/paquetes/gitmereba_<versión>_amd64.deb
```

El paquete instala `/usr/bin/gitmereba`, el lanzador de escritorio y los iconos.
`apt` instala también las dependencias (`libwebkit2gtk-4.1-0`, `libgtk-3-0`, `git`).
Para desinstalarlo: `sudo apt remove gitmereba`.

## 4. Instalar sin permisos de administrador

```bash
scripts/instalar-local.sh                    # bajo $HOME/.local
PREFIJO=/otra/ruta scripts/instalar-local.sh
scripts/desinstalar-local.sh                 # con el mismo PREFIJO
```

Funciona desde el repositorio (después de compilar) o desde un `.tar.gz` descomprimido.
Desinstalar borra solo los ficheros que se instalaron, **nunca los datos de las
cuentas**. Para eso está **Ajustes → Dar de baja** dentro de la aplicación.

## 5. Fijarlo al dock de GNOME

Abre **Mostrar aplicaciones**, busca «gitmereba» y, con el botón derecho sobre el icono,
elige **Añadir a favoritos**.

## 6. Si diste de alta cuentas con otro ejecutable

El temporizador de cada cuenta apunta al ejecutable con el que se creó. Al abrir la
ventana, gitmereba lo comprueba y, si hace falta, lo corrige solo. Basta con abrir una
vez la versión instalada. Para comprobarlo:
`systemctl --user cat gitmereba-sync-<login>.service`.

## 7. Verificar el código

```bash
scripts/verificar.sh    # formato, clippy, pruebas, licencias y avisos de seguridad
```

Necesita `cargo-nextest`, `cargo-deny` y `cargo-audit` (`cargo install --locked <nombre>`).
Más detalles en [CONTRIBUTING.md](../CONTRIBUTING.md).
