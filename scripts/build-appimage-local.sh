#!/usr/bin/env bash
# Monta uma AppImage do Sitrus localmente, sem appimage-builder e sem apt.
#
# Diferença para o AppImage do CI (packaging/AppImageBuilder.yml):
#   - o do CI é construído em ubuntu-22.04 e embute WebKitGTK/soup/GStreamer,
#     então roda em qualquer distro e inclui o login por webview;
#   - este aqui empacota o binário já compilado nesta máquina e usa as
#     bibliotecas do sistema (glibc, GTK3, X11/Wayland, ALSA). Roda nesta
#     distro; não é portátil para outra.
#
# Uso: scripts/build-appimage-local.sh [saida.AppImage]
# Requisitos: cargo build --release já feito, curl e AppImage tooling.
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output="${1:-$repo_dir/Sitrus-x86_64.AppImage}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo_dir/Cargo.toml" | head -1)"
appdir="$work/AppDir"

[ -x "$repo_dir/target/release/sitrus" ] || {
  echo "binário ausente: rode 'cargo build --release' antes" >&2
  exit 1
}

mkdir -p "$appdir/usr/bin" \
  "$appdir/usr/lib/sitrus/engine" \
  "$appdir/usr/share/applications" \
  "$appdir/usr/share/metainfo" \
  "$appdir/usr/share/icons/hicolor/scalable/apps"

cp "$repo_dir/target/release/sitrus" "$appdir/usr/bin/sitrus"
cp "$repo_dir/packaging/sitrus.desktop" "$appdir/usr/share/applications/"
cp "$repo_dir/packaging/sitrus.appdata.xml" "$appdir/usr/share/metainfo/"
cp "$repo_dir/packaging/sitrus.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/"
cp "$appdir/usr/share/applications/sitrus.desktop" "$appdir/sitrus.desktop"
cp "$appdir/usr/share/icons/hicolor/scalable/apps/sitrus.svg" "$appdir/sitrus.svg"

# Fontes do sidecar (JS), sem node_modules: dentro da AppImage o mount é
# somente leitura, então o bootstrap do Electron não roda ali — o engine só
# funciona com `engine/node_modules` presente (checkout do repo ou pacote RPM).
cp "$repo_dir/engine/package.json" "$repo_dir/engine/sidecar.js" \
  "$repo_dir/engine/main.js" "$repo_dir/engine/preload.js" \
  "$appdir/usr/lib/sitrus/engine/" 2>/dev/null || true
cp -r "$repo_dir/engine/lib" "$repo_dir/engine/mock" "$appdir/usr/lib/sitrus/engine/"

cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/sitrus" "$@"
APPRUN
chmod 755 "$appdir/AppRun"
chmod -R a+rX "$appdir"

# appimagetool: usa o do PATH ou baixa o build contínuo para o cache.
tool="$(command -v appimagetool || true)"
if [ -z "$tool" ]; then
  cache="${XDG_CACHE_HOME:-$HOME/.cache}/sitrus"
  mkdir -p "$cache"
  tool="$cache/appimagetool"
  [ -x "$tool" ] || curl -fsSL -o "$tool" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$tool"
fi

ARCH=x86_64 VERSION="$version" "$tool" --no-appstream "$appdir" "$output"
ls -lh "$output"
