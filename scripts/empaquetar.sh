#!/usr/bin/env bash
# Empaqueta gitmereba para Linux: compila en modo release y construye un .deb con
# dpkg-deb (o, con --tar, un .tar.gz genérico). Sin sudo, sin tocar el sistema real.
#
# Uso:
#   scripts/empaquetar.sh          # construye target/paquetes/gitmereba_<version>_amd64.deb
#   MANTENEDOR='Nombre <correo>' scripts/empaquetar.sh   # contacto real en el paquete
#   scripts/empaquetar.sh --tar    # además genera target/paquetes/gitmereba_<version>_linux_<arch>.tar.gz
#   scripts/empaquetar.sh --appimage   # además genera target/paquetes/gitmereba_<version>_x86_64.AppImage
#
# --appimage descarga linuxdeploy, su plugin gtk, appimagetool y el runtime type2 a
# target/herramientas-appimage/
# (versiones y SHA-256 fijados en este script y en empaquetado/appimage/herramientas.sha256;
# se comprueban antes de dar permisos de ejecución). Necesita, además de las dependencias de
# compilación: curl, file, patchelf,
# libglib2.0-bin, libglib2.0-dev-bin, libgdk-pixbuf2.0-bin, libgtk-3-bin y librsvg2-common.
# Conviene construirlo en la distribución más antigua que se quiera soportar (glibc).
set -euo pipefail

hacer_tar=0
hacer_appimage=0
for arg in "$@"; do
    case "$arg" in
        --tar) hacer_tar=1 ;;
        --appimage) hacer_appimage=1 ;;
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

# --- AppImage opcional ---------------------------------------------------------------
if [ "$hacer_appimage" -eq 1 ]; then
    arquitectura_appimage="$(uname -m)"
    if [ "$arquitectura_appimage" != "x86_64" ]; then
        echo "empaquetar.sh: el AppImage solo está soportado en x86_64 (aquí: $arquitectura_appimage)" >&2
        exit 1
    fi

    # Versiones concretas (nada de «continuous» ni «latest»). Los SHA-256 están en
    # empaquetado/appimage/herramientas.sha256, con estos mismos nombres de fichero.
    # - linuxdeploy 1-alpha-20251107-1 (github.com/linuxdeploy/linuxdeploy)
    # - plugin gtk: commit 3b67a1d de linuxdeploy-plugin-gtk (un script, fijado por SHA)
    # - appimagetool 1.9.1 (github.com/AppImage/appimagetool)
    herramientas="$raiz/target/herramientas-appimage"
    fichero_sumas="$raiz/empaquetado/appimage/herramientas.sha256"
    nombre_linuxdeploy="linuxdeploy-1-alpha-20251107-1-x86_64.AppImage"
    url_linuxdeploy="https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage"
    nombre_plugin_gtk="linuxdeploy-plugin-gtk-3b67a1d.sh"
    url_plugin_gtk="https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/3b67a1d1c1b0c8268f57f2bce40fe2d33d409cea/linuxdeploy-plugin-gtk.sh"
    nombre_appimagetool="appimagetool-1.9.1-x86_64.AppImage"
    url_appimagetool="https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage"
    # - runtime type2 (el ejecutable que abre el AppImage), release 20251108 de
    #   AppImage/type2-runtime. Sin --runtime-file, appimagetool lo descargaría de la
    #   release «continuous» en cada construcción; así queda fijado y verificado.
    nombre_runtime="runtime-type2-20251108-x86_64"
    url_runtime="https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64"

    mkdir -p "$herramientas"

    # Comprueba el SHA-256 de una herramienta contra el fichero versionado en el repo.
    # Si no coincide (o falta), la borra y aborta. No da permisos ni ejecuta nada.
    verificar_herramienta() {
        local nombre="$1"
        local linea
        linea="$(grep -F "  $nombre" "$fichero_sumas" || true)"
        if [ -z "$linea" ]; then
            rm -f "$herramientas/$nombre"
            echo "empaquetar.sh: no hay SHA-256 fijado para $nombre en $fichero_sumas" >&2
            exit 1
        fi
        if ! (cd "$herramientas" && printf '%s\n' "$linea" | sha256sum -c --status -); then
            rm -f "$herramientas/$nombre"
            echo "empaquetar.sh: el SHA-256 de $nombre no coincide con el fijado en el repo;" >&2
            echo "  se ha borrado la descarga y se aborta (¿descarga manipulada o corrupta?)" >&2
            exit 1
        fi
    }

    # Descarga (si no está ya en la caché) y verifica. El fichero llega sin permisos de
    # ejecución; el chmod va después, una vez comprobado el hash.
    obtener_herramienta() {
        local nombre="$1" url="$2"
        if [ ! -f "$herramientas/$nombre" ]; then
            echo "== Descargando $nombre =="
            rm -f "$herramientas/$nombre.parcial"
            curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
                --output "$herramientas/$nombre.parcial" "$url"
            mv "$herramientas/$nombre.parcial" "$herramientas/$nombre"
        fi
        verificar_herramienta "$nombre"
        chmod 0755 "$herramientas/$nombre"
    }

    obtener_herramienta "$nombre_linuxdeploy" "$url_linuxdeploy"
    obtener_herramienta "$nombre_plugin_gtk" "$url_plugin_gtk"
    obtener_herramienta "$nombre_appimagetool" "$url_appimagetool"
    obtener_herramienta "$nombre_runtime" "$url_runtime"

    # linuxdeploy busca sus plugins en el PATH con el nombre linuxdeploy-plugin-<x>.sh.
    mkdir -p "$herramientas/bin"
    ln -sf "../$nombre_plugin_gtk" "$herramientas/bin/linuxdeploy-plugin-gtk.sh"

    appdir="$directorio_paquetes/appimage-stage"
    puesta_en_escena "$appdir"

    # Procesos auxiliares de WebKitGTK (WebKitWebProcess, WebKitNetworkProcess e injected
    # bundle): libwebkit2gtk los busca en una ruta absoluta fijada al compilar
    # (/usr/lib/<multiarch>/webkit2gtk-4.1) y no admite sustituirla por variable de entorno.
    # Igual que el bundler AppImage de Tauri, se copian al AppDir con la misma ruta y se
    # parchea la cadena en libwebkit2gtk cambiando "/usr" por "././" (mismo largo): queda
    # una ruta relativa a $APPDIR/usr, que la ventana fija como directorio de trabajo.
    origen_webkit=""
    for base in "/usr/lib/$(uname -m)-linux-gnu" /usr/lib64 /usr/lib /usr/libexec; do
        if [ -x "$base/webkit2gtk-4.1/WebKitWebProcess" ]; then
            origen_webkit="$base/webkit2gtk-4.1"
            break
        fi
    done
    if [ -z "$origen_webkit" ]; then
        echo "empaquetar.sh: no se encuentra webkit2gtk-4.1 (¿falta libwebkit2gtk-4.1-dev?)" >&2
        exit 1
    fi
    # La ventana reconoce el AppImage por esta ruta exacta (crates/core/src/entorno.rs).
    if [ "$origen_webkit" != "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1" ]; then
        echo "empaquetar.sh: ruta de WebKit inesperada: $origen_webkit (se esperaba /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1)" >&2
        exit 1
    fi
    destino_webkit="$appdir$origen_webkit"
    mkdir -p "$destino_webkit/injected-bundle"
    for auxiliar in WebKitWebProcess WebKitNetworkProcess; do
        install -m 0755 "$origen_webkit/$auxiliar" "$destino_webkit/$auxiliar"
    done
    install -m 0644 "$origen_webkit/injected-bundle/libwebkit2gtkinjectedbundle.so" \
        "$destino_webkit/injected-bundle/libwebkit2gtkinjectedbundle.so"

    mkdir -p "$appdir/apprun-hooks"
    # Gancho que AppRun carga antes de lanzar gitmereba: el injected bundle sí admite
    # variable de entorno (los procesos auxiliares van por la ruta relativa parcheada).
    printf 'export WEBKIT_INJECTED_BUNDLE_PATH="$APPDIR%s/injected-bundle"\n' "$origen_webkit" \
        > "$appdir/apprun-hooks/webkit.sh"

    echo "== Desplegando bibliotecas en el AppDir (linuxdeploy) =="
    # git NO se incluye: se usa el del sistema. gitea tampoco: la app lo descarga.
    PATH="$herramientas/bin:$PATH" \
        ARCH=x86_64 \
        APPIMAGE_EXTRACT_AND_RUN=1 \
        DEPLOY_GTK_VERSION=3 \
        NO_STRIP=1 \
        "$herramientas/$nombre_linuxdeploy" \
        --appdir "$appdir" \
        --executable "$appdir/usr/bin/gitmereba" \
        --executable "$destino_webkit/WebKitWebProcess" \
        --executable "$destino_webkit/WebKitNetworkProcess" \
        --library "$destino_webkit/injected-bundle/libwebkit2gtkinjectedbundle.so" \
        --desktop-file "$appdir/usr/share/applications/gitmereba.desktop" \
        --icon-file "$appdir/usr/share/icons/hicolor/512x512/apps/gitmereba.png" \
        --plugin gtk

    # linuxdeploy deja a los auxiliares de WebKit un RUNPATH de $ORIGIN, pero sus
    # bibliotecas están en usr/lib: se corrige a una ruta relativa a cada fichero (así no
    # hace falta LD_LIBRARY_PATH, que heredaría el git del sistema que lanza la app).
    if ! command -v patchelf >/dev/null 2>&1; then
        echo "empaquetar.sh: falta patchelf (sudo apt install patchelf)" >&2
        exit 1
    fi
    for auxiliar in \
        "$destino_webkit/WebKitWebProcess" \
        "$destino_webkit/WebKitNetworkProcess" \
        "$destino_webkit/injected-bundle/libwebkit2gtkinjectedbundle.so"; do
        relativa="$(realpath --relative-to="$(dirname "$auxiliar")" "$appdir/usr/lib")"
        patchelf --set-rpath "\$ORIGIN/$relativa" "$auxiliar"
    done

    # Parche de la ruta compilada en libwebkit2gtk (tras linuxdeploy, que la reescribe).
    biblioteca_webkit="$(find "$appdir/usr/lib" -maxdepth 1 -name 'libwebkit2gtk-4.1.so.0' | head -1)"
    if [ -z "$biblioteca_webkit" ]; then
        echo "empaquetar.sh: linuxdeploy no desplegó libwebkit2gtk-4.1" >&2
        exit 1
    fi
    ruta_relativa="././${origen_webkit#/usr}"
    LC_ALL=C sed -i "s|$origen_webkit|$ruta_relativa|g" "$biblioteca_webkit"
    if ! grep -aqF "$ruta_relativa" "$biblioteca_webkit" || grep -aqF "$origen_webkit" "$biblioteca_webkit"; then
        echo "empaquetar.sh: no se pudo parchear la ruta de WebKit en $biblioteca_webkit" >&2
        exit 1
    fi

    paquete_appimage="$directorio_paquetes/gitmereba_${version}_x86_64.AppImage"
    echo "== Construyendo $paquete_appimage =="
    rm -f "$paquete_appimage"
    ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 \
        "$herramientas/$nombre_appimagetool" --no-appstream \
        --runtime-file "$herramientas/$nombre_runtime" "$appdir" "$paquete_appimage"
    echo "Generado: $paquete_appimage"
fi
