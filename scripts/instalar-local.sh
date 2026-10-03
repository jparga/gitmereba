#!/usr/bin/env bash
# Instala gitmereba para el usuario actual, sin root, bajo PREFIJO (por defecto
# $HOME/.local). Sirve tanto ejecutado desde el repositorio (usa
# target/release/gitmereba, compilado antes con «cargo build --release -p gitmereba»
# o con scripts/empaquetar.sh) como desde un .tar.gz de scripts/empaquetar.sh --tar ya
# descomprimido (usa el binario e iconos que trae al lado).
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prefijo="${PREFIJO:-$HOME/.local}"

primero_existente() {
    for candidato in "$@"; do
        if [ -e "$candidato" ]; then
            printf '%s\n' "$candidato"
            return 0
        fi
    done
    return 1
}

binario_origen="$(primero_existente \
    "$script_dir/bin/gitmereba" \
    "$script_dir/../target/release/gitmereba")" \
    || { echo "instalar-local.sh: no encuentro el binario compilado (¿falta «cargo build --release -p gitmereba»?)" >&2; exit 1; }

desktop_origen="$(primero_existente \
    "$script_dir/share/applications/gitmereba.desktop" \
    "$script_dir/../empaquetado/gitmereba.desktop")" \
    || { echo "instalar-local.sh: no encuentro gitmereba.desktop" >&2; exit 1; }

tamanos_icono=(32 48 64 128 256 512)

echo "== Instalando gitmereba en $prefijo =="

mkdir -p "$prefijo/bin" "$prefijo/share/applications"
install -m 0755 "$binario_origen" "$prefijo/bin/gitmereba"

# Exec= con la ruta absoluta: $prefijo/bin puede no estar en el PATH de la sesión
# gráfica (GNOME no siempre lo hereda), así que el lanzador del escritorio no puede
# fiarse de encontrar «gitmereba» en el PATH.
sed "s#^Exec=gitmereba#Exec=$prefijo/bin/gitmereba#" "$desktop_origen" \
    > "$prefijo/share/applications/gitmereba.desktop"
chmod 0644 "$prefijo/share/applications/gitmereba.desktop"

for n in "${tamanos_icono[@]}"; do
    icono_origen="$(primero_existente \
        "$script_dir/share/icons/hicolor/${n}x${n}/apps/gitmereba.png" \
        "$script_dir/../empaquetado/iconos/${n}x${n}/gitmereba.png")" \
        || { echo "instalar-local.sh: no encuentro el icono ${n}x${n}" >&2; exit 1; }
    mkdir -p "$prefijo/share/icons/hicolor/${n}x${n}/apps"
    install -m 0644 "$icono_origen" "$prefijo/share/icons/hicolor/${n}x${n}/apps/gitmereba.png"
done

# Manuales: opcionales (en el paquete tar o en docs/ del repositorio).
for nombre_manual in manual.md manual.en.md; do
    if manual_origen="$(primero_existente \
        "$script_dir/share/doc/gitmereba/$nombre_manual" \
        "$script_dir/../docs/$nombre_manual")"; then
        mkdir -p "$prefijo/share/doc/gitmereba"
        install -m 0644 "$manual_origen" "$prefijo/share/doc/gitmereba/$nombre_manual"
    fi
done

if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d "$prefijo/share/icons/hicolor" ]; then
    gtk-update-icon-cache -q "$prefijo/share/icons/hicolor" 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$prefijo/share/applications" 2>/dev/null || true
fi

echo "Instalado. Búscalo en «Mostrar aplicaciones» como «gitmereba»."

case ":$PATH:" in
    *":$prefijo/bin:"*) ;;
    *)
        echo
        echo "Aviso: $prefijo/bin no está en tu PATH. Para lanzar «gitmereba» desde una" >&2
        echo "terminal añade esto a tu ~/.bashrc (o ~/.zshrc) y abre una nueva sesión:" >&2
        echo "    export PATH=\"$prefijo/bin:\$PATH\"" >&2
        echo "El icono del menú de aplicaciones funciona igualmente: usa la ruta absoluta." >&2
        ;;
esac
