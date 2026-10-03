#!/usr/bin/env bash
# Desinstala lo que instalar-local.sh puso bajo PREFIJO (por defecto $HOME/.local).
# Borra exactamente esos ficheros: nunca toca ~/.local/share/gitmereba ni las carpetas
# de las cuentas, ni las unidades systemd --user: si ya no vas a usar gitmereba, da antes
# de baja las cuentas (`gitmereba cuenta rm <login>`), que es lo que las detiene y borra.
set -euo pipefail

prefijo="${PREFIJO:-$HOME/.local}"
tamanos_icono=(32 48 64 128 256 512)

echo "== Desinstalando gitmereba de $prefijo =="

rm -f "$prefijo/bin/gitmereba"
rm -f "$prefijo/share/applications/gitmereba.desktop"
for n in "${tamanos_icono[@]}"; do
    rm -f "$prefijo/share/icons/hicolor/${n}x${n}/apps/gitmereba.png"
done
rm -f "$prefijo/share/doc/gitmereba/manual.md" "$prefijo/share/doc/gitmereba/manual.en.md"
rmdir "$prefijo/share/doc/gitmereba" 2>/dev/null || true

if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d "$prefijo/share/icons/hicolor" ]; then
    gtk-update-icon-cache -q "$prefijo/share/icons/hicolor" 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$prefijo/share/applications" 2>/dev/null || true
fi

echo "Desinstalado. Los datos de tus cuentas (en ~/.local/share/gitmereba y en sus"
echo "carpetas) no se han tocado."
