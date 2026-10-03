#!/usr/bin/env bash
# Empaqueta gitmereba para Linux: compila en modo release y construye un .deb con
# dpkg-deb (o, con --tar, un .tar.gz genérico). Sin sudo, sin tocar el sistema real.
#
# Uso:
#   scripts/empaquetar.sh          # construye target/paquetes/gitmereba_<version>_amd64.deb
#   MANTENEDOR='Nombre <correo>' scripts/empaquetar.sh   # contacto real en el paquete
#   scripts/empaquetar.sh --tar    # además genera target/paquetes/gitmereba_<version>_linux_<arch>.tar.gz
set -euo pipefail

hacer_tar=0
for arg in "$@"; do
    case "$arg" in
        --tar) hacer_tar=1 ;;
        *)
            echo "empaquetar.sh: opción desconocida: $arg" >&2
            exit 1
            ;;
    esac
done

raiz="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$raiz"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [ -z "$version" ]; then
    echo "empaquetar.sh: no se pudo leer la versión de Cargo.toml" >&2
    exit 1
fi
arquitectura="$(dpkg --print-architecture)"

echo "== Compilando gitmereba $version ($arquitectura) en modo release =="
cargo build --release -p gitmereba

binario="$raiz/target/release/gitmereba"
if [ ! -x "$binario" ]; then
    echo "empaquetar.sh: no se encuentra $binario tras la compilación" >&2
    exit 1
fi

tamanos_icono=(32 48 64 128 256 512)

directorio_paquetes="$raiz/target/paquetes"
mkdir -p "$directorio_paquetes"

# --- Puesta en escena común a .deb y .tar.gz: árbol usr/... -----------------------
puesta_en_escena() {
    local destino="$1"
    rm -rf "$destino"
    mkdir -p "$destino/usr/bin"
    mkdir -p "$destino/usr/share/applications"
    mkdir -p "$destino/usr/share/doc/gitmereba"

    install -m 0755 "$binario" "$destino/usr/bin/gitmereba"
    if command -v strip >/dev/null 2>&1; then
        strip --strip-unneeded "$destino/usr/bin/gitmereba" 2>/dev/null || true
    fi

    install -m 0644 "$raiz/empaquetado/gitmereba.desktop" \
        "$destino/usr/share/applications/gitmereba.desktop"

    for n in "${tamanos_icono[@]}"; do
        mkdir -p "$destino/usr/share/icons/hicolor/${n}x${n}/apps"
        install -m 0644 "$raiz/empaquetado/iconos/${n}x${n}/gitmereba.png" \
            "$destino/usr/share/icons/hicolor/${n}x${n}/apps/gitmereba.png"
    done

    install -m 0644 "$raiz/docs/manual.md" "$destino/usr/share/doc/gitmereba/manual.md"
    install -m 0644 "$raiz/docs/manual.en.md" "$destino/usr/share/doc/gitmereba/manual.en.md"
    cat > "$destino/usr/share/doc/gitmereba/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: gitmereba
Source: https://github.com/jparga/gitmereba

Files: *
Copyright: 2026 Jacinto Parga (MEREBA)
License: GPL-3.0-or-later
 This program is free software: you can redistribute it and/or modify it under the
 terms of the GNU General Public License as published by the Free Software
 Foundation, either version 3 of the License, or (at your option) any later version.
 .
 On Debian systems, the full text of the GNU General Public License version 3 can be
 found in /usr/share/common-licenses/GPL-3.

Files: usr/share/icons/hicolor/*/apps/gitmereba.png
Copyright: 2026 MEREBA
License: LicenseRef-Mereba-Marca
 El nombre y el logotipo de MEREBA son marcas de MEREBA. Se pueden redistribuir sin
 modificar como parte de gitmereba; ver TRADEMARKS.md en el código fuente.
EOF
}

# --- .deb --------------------------------------------------------------------------
raiz_deb="$directorio_paquetes/deb-stage"
puesta_en_escena "$raiz_deb"
mkdir -p "$raiz_deb/DEBIAN"

tamano_instalado_kb="$(du -sk "$raiz_deb" | cut -f1)"

cat > "$raiz_deb/DEBIAN/control" <<EOF
Package: gitmereba
Version: $version
Architecture: $arquitectura
Installed-Size: $tamano_instalado_kb
Maintainer: ${MANTENEDOR:-Jacinto Parga <jparga@mereba.com>}
Homepage: https://github.com/jparga/gitmereba
Section: vcs
Priority: optional
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, git
Recommends: gnome-keyring | kwalletmanager, ufw
Description: Clon local y operativo de cuentas de GitHub sobre Gitea
 gitmereba mantiene en tu equipo un espejo vivo de tus cuentas de GitHub sobre un
 Gitea nativo: si GitHub deja de responder sigues trabajando contra la copia local
 y, cuando vuelve, devuelves los cambios. Cada cuenta tiene su propio servidor
 Gitea, los tokens viven en el llavero del sistema y la sincronización periódica
 corre sola en segundo plano vía systemd --user.
EOF

for script in postinst postrm; do
    cp "$raiz/empaquetado/$script" "$raiz_deb/DEBIAN/$script"
    chmod 0755 "$raiz_deb/DEBIAN/$script"
done

paquete_deb="$directorio_paquetes/gitmereba_${version}_${arquitectura}.deb"
echo "== Construyendo $paquete_deb =="
dpkg-deb --root-owner-group --build "$raiz_deb" "$paquete_deb"
echo "Generado: $paquete_deb"

# --- .tar.gz opcional, para distribuciones sin dpkg ---------------------------------
if [ "$hacer_tar" -eq 1 ]; then
    raiz_tar="$directorio_paquetes/tar-stage"
    rm -rf "$raiz_tar"
    mkdir -p "$raiz_tar/bin" "$raiz_tar/share/applications" "$raiz_tar/share/doc/gitmereba"
    install -m 0755 "$binario" "$raiz_tar/bin/gitmereba"
    if command -v strip >/dev/null 2>&1; then
        strip --strip-unneeded "$raiz_tar/bin/gitmereba" 2>/dev/null || true
    fi
    install -m 0644 "$raiz/empaquetado/gitmereba.desktop" \
        "$raiz_tar/share/applications/gitmereba.desktop"
    for n in "${tamanos_icono[@]}"; do
        mkdir -p "$raiz_tar/share/icons/hicolor/${n}x${n}/apps"
        install -m 0644 "$raiz/empaquetado/iconos/${n}x${n}/gitmereba.png" \
            "$raiz_tar/share/icons/hicolor/${n}x${n}/apps/gitmereba.png"
    done
    install -m 0644 "$raiz/docs/manual.md" "$raiz_tar/share/doc/gitmereba/manual.md"
    install -m 0644 "$raiz/docs/manual.en.md" "$raiz_tar/share/doc/gitmereba/manual.en.md"
    install -m 0755 "$raiz/scripts/instalar-local.sh" "$raiz_tar/instalar-local.sh"
    install -m 0755 "$raiz/scripts/desinstalar-local.sh" "$raiz_tar/desinstalar-local.sh"

    paquete_tar="$directorio_paquetes/gitmereba_${version}_linux_${arquitectura}.tar.gz"
    echo "== Construyendo $paquete_tar =="
    tar -czf "$paquete_tar" -C "$raiz_tar" .
    echo "Generado: $paquete_tar"
fi
