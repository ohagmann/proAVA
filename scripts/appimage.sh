#!/usr/bin/env bash
# AppImage für Linux aus dem fertigen Linux-Ordner von scripts/release.sh:
#   dist/<version>/proava-<version>-x86_64.AppImage
#
# Eine Datei, ohne Einrichten lauffähig (ausführbar machen, starten). Inhalt: proava,
# proava-mcp, proava-web, Symbole, Handbuch, Lizenzen und install_linux.sh. Läuft auf Systemen
# mit derselben oder einer neueren glibc als das Build-System (Ubuntu 24.04 / Linux Mint 22:
# glibc 2.39). Das Browserfenster nutzt das WebKitGTK 4.1 des Systems; fehlt es, öffnet proAVA
# die Seiten im System-Browser.
#
#   scripts/appimage.sh [LINUX-ORDNER [ZIEL]]
#
# Werkzeuge (einmal herunterladen, nicht im Repo), Ordner APPIMAGE_TOOLS (Vorgabe
# ~/.local/opt/appimage):
#   appimagetool-x86_64.AppImage  https://github.com/AppImage/appimagetool/releases
#   runtime-x86_64                https://github.com/AppImage/type2-runtime/releases
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
LINUX="${1:-$ROOT/dist/$VERSION/linux}"
ZIEL="${2:-$(dirname "$LINUX")/proava-$VERSION-x86_64.AppImage}"
TOOLS="${APPIMAGE_TOOLS:-$HOME/.local/opt/appimage}"
TOOL="$TOOLS/appimagetool-x86_64.AppImage"
RUNTIME="$TOOLS/runtime-x86_64"

fehlt() {
    echo "FEHLER: $*" >&2
    exit 1
}
[ -x "$LINUX/proava" ] || fehlt "$LINUX/proava fehlt – erst scripts/release.sh --nur-linux."
[ -x "$TOOL" ] || fehlt "$TOOL fehlt (siehe Kopf von scripts/appimage.sh)."
[ -f "$RUNTIME" ] || fehlt "$RUNTIME fehlt (siehe Kopf von scripts/appimage.sh)."

ARBEIT=$(mktemp -d "${TMPDIR:-/tmp}/proava-appimage.XXXXXX")
trap 'rm -rf "$ARBEIT"' EXIT
APPDIR="$ARBEIT/proAVA.AppDir"
SIZES=(16 24 32 48 64 128 256 512)

mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/proava/icons" "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/mime/packages" "$APPDIR/usr/share/doc/proava"
for bin in proava proava-mcp proava-web; do
    if [ -x "$LINUX/$bin" ]; then
        install -m 755 "$LINUX/$bin" "$APPDIR/usr/bin/$bin"
    fi
done
install -m 755 assets/linux/AppRun "$APPDIR/AppRun"
# Startmenü und Dateitypen aus dem AppImage heraus („Ins Startmenü eintragen“, --integrieren):
# install_linux.sh mit allem, was es braucht, wie im entpackten Archiv.
install -m 755 scripts/install_linux.sh "$APPDIR/usr/share/proava/install_linux.sh"
install -m 644 assets/linux/proava.desktop assets/linux/proava-mime.xml LICENSE THIRD-PARTY-NOTICES \
    docs/handbuch/proava-handbuch.pdf "$APPDIR/usr/share/proava/"
for n in "${SIZES[@]}"; do
    install -m 644 "assets/icons/proava-$n.png" "$APPDIR/usr/share/proava/icons/proava-$n.png"
    install -D -m 644 "assets/icons/proava-$n.png" "$APPDIR/usr/share/icons/hicolor/${n}x${n}/apps/proava.png"
done
# Was AppImage-Werkzeuge (Startmenü-Helfer, Dateimanager) lesen.
install -m 644 assets/linux/proava.desktop "$APPDIR/proava.desktop"
install -m 644 assets/linux/proava.desktop "$APPDIR/usr/share/applications/proava.desktop"
install -m 644 assets/linux/proava-mime.xml "$APPDIR/usr/share/mime/packages/proava.xml"
install -m 644 assets/icons/proava-256.png "$APPDIR/proava.png"
ln -s proava.png "$APPDIR/.DirIcon"
# Doku; Hilfe ▸ Handbuch nimmt das PDF von hier (usr/bin/../share/doc/proava).
install -m 644 LICENSE THIRD-PARTY-NOTICES assets/linux/APPIMAGE-LAUFZEIT.txt \
    docs/handbuch/proava-handbuch.pdf "$APPDIR/usr/share/doc/proava/"

rm -f "$ZIEL"
# appimagetool ist selbst ein AppImage: ohne FUSE entpackt es sich und läuft so.
if ! ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream --runtime-file "$RUNTIME" \
    "$APPDIR" "$ZIEL" >"$ARBEIT/appimagetool.log" 2>&1; then
    tail -n 30 "$ARBEIT/appimagetool.log" >&2
    fehlt "appimagetool ist gescheitert."
fi
chmod 755 "$ZIEL"
echo "AppImage: $ZIEL ($(du -h "$ZIEL" | cut -f1))"
