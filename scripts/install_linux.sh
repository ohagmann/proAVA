#!/usr/bin/env bash
# proAVA für den eigenen Benutzer unter Linux einrichten – ohne sudo, nur in
#   ~/.local/bin                      proava, proava-mcp, proava-web (Browserfenster, wenn gebaut)
#   ~/.local/share/applications       proava.desktop (Startmenü, „Öffnen mit“)
#   ~/.local/share/icons/hicolor      Programmsymbol 16–512 px
#   ~/.local/share/mime               MIME-Typen .proava, GAEB DA XML (.X81–.X86), GAEB 90 (.D81–.D86)
#   ~/.local/share/doc/proava        LICENSE, THIRD-PARTY-NOTICES, proava-handbuch.pdf
#
#   Im entpackten Archiv: ./install_linux.sh
#   scripts/install_linux.sh [ORDNER]          einrichten; ORDNER = Ordner mit proava und
#                                              proava-mcp (Vorgabe: dist/<Version>/linux,
#                                              sonst target/release)
#   scripts/install_linux.sh --gaeb-standard   zusätzlich proAVA als Standard für GAEB-Dateien
#   scripts/install_linux.sh --entfernen       alles wieder entfernen (Projekte bleiben)
#   install_linux.sh --appimage DATEI          nur Startmenü, Dateitypen, Symbole und Doku für
#                                              das AppImage DATEI (Exec = DATEI), keine Programme
#                                              nach ~/.local/bin; ruft das AppImage selbst auf
#                                              („Ins Startmenü eintragen“, „DATEI --integrieren“)
#
# Doppelklick auf eine .proava öffnet das Projekt; GAEB-Dateien bekommen
# „Öffnen mit proAVA“ (→ Neues Projekt aus GAEB). Standard für GAEB wird proAVA
# nur mit --gaeb-standard – sonst bleibt der bisherige Standard (z. B. Editor).
set -euo pipefail

HIER=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# Zwei Lagen: im Repo (scripts/, assets/) oder im entpackten Linux-Archiv (alles in einem Ordner).
if [ -f "$HIER/proava.desktop" ]; then
    ROOT=""
    ASSETS_DESKTOP="$HIER/proava.desktop"
    ASSETS_MIME="$HIER/proava-mime.xml"
    ASSETS_ICONS="$HIER/icons"
    NOTICES="$HIER/THIRD-PARTY-NOTICES"
    LIZENZ="$HIER/LICENSE"
    HANDBUCH="$HIER/proava-handbuch.pdf"
else
    ROOT=$(cd "$HIER/.." && pwd)
    ASSETS_DESKTOP="$ROOT/assets/linux/proava.desktop"
    ASSETS_MIME="$ROOT/assets/linux/proava-mime.xml"
    ASSETS_ICONS="$ROOT/assets/icons"
    NOTICES="$ROOT/THIRD-PARTY-NOTICES"
    LIZENZ="$ROOT/LICENSE"
    HANDBUCH="$ROOT/docs/handbuch/proava-handbuch.pdf"
fi
BIN="$HOME/.local/bin"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$SHARE/applications"
ICONS="$SHARE/icons/hicolor"
MIME="$SHARE/mime"
DOC="$SHARE/doc/proava"
SIZES=(16 24 32 48 64 128 256 512)
MIME_TYPES=(application/x-proava application/x-gaeb-xml application/x-gaeb90)

fehlt() {
    echo "FEHLER: $*" >&2
    exit 1
}

aktualisieren() {
    command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$MIME" || true
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t -f "$ICONS" || true
}

if [ "${1:-}" = "--entfernen" ]; then
    rm -f "$BIN/proava" "$BIN/proava-mcp" "$BIN/proava-web" "$APPS/proava.desktop" "$MIME/packages/proava.xml"
    for n in "${SIZES[@]}"; do
        rm -f "$ICONS/${n}x${n}/apps/proava.png"
    done
    rm -rf "$DOC"
    # Standard-Zuordnungen, die dieses Skript gesetzt hat
    LISTE="${XDG_CONFIG_HOME:-$HOME/.config}/mimeapps.list"
    [ -f "$LISTE" ] && sed -i '/^application\/x-\(proava\|gaeb-xml\|gaeb90\)=proava\.desktop$/d' "$LISTE"
    aktualisieren
    echo "proAVA entfernt (Projektdateien und Einstellungen bleiben)."
    exit 0
fi

GAEB_STANDARD=0
QUELLE=""
APPIMAGE_DATEI=""
while [ $# -gt 0 ]; do
    case "$1" in
        --gaeb-standard) GAEB_STANDARD=1 ;;
        --appimage)
            [ $# -ge 2 ] || fehlt "--appimage braucht den Pfad des AppImage."
            APPIMAGE_DATEI="$2"
            shift
            ;;
        -*) fehlt "Unbekannte Option: $1" ;;
        *) QUELLE="$1" ;;
    esac
    shift
done

# Symbole, Startmenü-Eintrag (Exec = Programm), Dateitypen, Doku.
einrichten_ohne_programm() {
    local exec_programm=$1 zeile
    mkdir -p "$APPS" "$MIME/packages" "$DOC"
    for n in "${SIZES[@]}"; do
        mkdir -p "$ICONS/${n}x${n}/apps"
        install -m 644 "$ASSETS_ICONS/proava-$n.png" "$ICONS/${n}x${n}/apps/proava.png"
    done
    # Exec mit vollem Pfad: ~/.local/bin steht nicht immer im PATH des Desktops.
    while IFS= read -r zeile || [ -n "$zeile" ]; do
        case "$zeile" in
            "Exec=proava "*) printf 'Exec=%s %s\n' "$exec_programm" "${zeile#Exec=proava }" ;;
            *) printf '%s\n' "$zeile" ;;
        esac
    done <"$ASSETS_DESKTOP" >"$APPS/proava.desktop"
    chmod 644 "$APPS/proava.desktop"
    install -m 644 "$ASSETS_MIME" "$MIME/packages/proava.xml"
    install -m 644 "$LIZENZ" "$DOC/LICENSE"
    install -m 644 "$NOTICES" "$DOC/THIRD-PARTY-NOTICES"
    install -m 644 "$HANDBUCH" "$DOC/proava-handbuch.pdf"
    aktualisieren
    if command -v xdg-mime >/dev/null 2>&1; then
        xdg-mime default proava.desktop application/x-proava
        if [ "$GAEB_STANDARD" = 1 ]; then
            xdg-mime default proava.desktop application/x-gaeb-xml application/x-gaeb90
        fi
    fi
}

if [ -n "$APPIMAGE_DATEI" ]; then
    case "$APPIMAGE_DATEI" in
        /*) ;;
        *) fehlt "--appimage braucht einen vollen Pfad ($APPIMAGE_DATEI)." ;;
    esac
    [ -f "$APPIMAGE_DATEI" ] || fehlt "$APPIMAGE_DATEI fehlt."
    # Desktop Entry Specification: Programm in Anführungszeichen; darin \ als \\\\, " ` $ mit \,
    # % als %%.
    ESC=$(printf '%s' "$APPIMAGE_DATEI" | sed -e 's/\\/\\\\\\\\/g' -e 's/["`$]/\\&/g' -e 's/%/%%/g')
    einrichten_ohne_programm "\"$ESC\""
    echo "proAVA (AppImage) eingerichtet:"
    echo "  Programm   $APPIMAGE_DATEI"
    echo "  Startmenü  $APPS/proava.desktop"
    echo "  Dateitypen ${MIME_TYPES[*]}"
    if [ -x "$BIN/proava" ]; then
        echo "Hinweis: In $BIN liegt außerdem ein installiertes proAVA – das Startmenü startet jetzt das AppImage."
    fi
    exit 0
fi

if [ -z "$QUELLE" ]; then
    if [ -z "$ROOT" ]; then
        QUELLE="$HIER"
    else
        VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
        if [ -x "$ROOT/dist/$VERSION/linux/proava" ]; then
            QUELLE="$ROOT/dist/$VERSION/linux"
        else
            QUELLE="$ROOT/target/release"
        fi
    fi
fi
[ -x "$QUELLE/proava" ] || fehlt "$QUELLE/proava fehlt – erst scripts/release.sh oder cargo build --release."

mkdir -p "$BIN"
install -m 755 "$QUELLE/proava" "$BIN/proava"
[ -x "$QUELLE/proava-mcp" ] && install -m 755 "$QUELLE/proava-mcp" "$BIN/proava-mcp"
# Browserfenster für AUSSCHREIBEN.DE/STLB-Bau Online: liegt es neben proava, nimmt proAVA es.
if [ -x "$QUELLE/proava-web" ]; then
    install -m 755 "$QUELLE/proava-web" "$BIN/proava-web"
else
    rm -f "$BIN/proava-web"
fi
einrichten_ohne_programm "$BIN/proava"

echo "proAVA eingerichtet:"
echo "  Programm   $BIN/proava ($("$BIN/proava" --version 2>/dev/null || echo '?'))"
if [ -x "$BIN/proava-web" ]; then
    echo "  Browser    $BIN/proava-web (Fenster für AUSSCHREIBEN.DE und STLB-Bau Online)"
else
    echo "  Browser    – (proava-web nicht gebaut: proAVA nimmt den System-Browser)"
fi
echo "  Startmenü  $APPS/proava.desktop"
echo "  Dateitypen ${MIME_TYPES[*]}"
case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Hinweis: $BIN steht nicht im PATH – im Terminal mit vollem Pfad starten." ;;
esac

# Noch laufende Programme mit der alten Version (vor diesem Einrichten gestartet): Linux zeigt
# ihren Programmpfad jetzt als „$BIN/<Name> (deleted)“. Nur melden, nichts beenden – proAVA
# sagt es auch selbst in der Statusleiste („Neu starten“), der MCP-Server in seiner nächsten
# Antwort.
alte_laeufe() {
    local name=$1 n=0 pid kandidaten
    if command -v pgrep >/dev/null 2>&1; then
        kandidaten=$(pgrep -u "$(id -u)" -x "$name" 2>/dev/null || true)
    else
        kandidaten=$(ls /proc 2>/dev/null | grep -E '^[0-9]+$' || true)
    fi
    for pid in $kandidaten; do
        if [ "$(readlink "/proc/$pid/exe" 2>/dev/null)" = "$BIN/$name (deleted)" ]; then
            n=$((n + 1))
        fi
    done
    echo "$n"
}
ALTE_FENSTER=$(alte_laeufe proava)
if [ "$ALTE_FENSTER" = 1 ]; then
    echo "Es läuft noch 1 proAVA-Fenster mit der alten Version – bitte neu starten."
elif [ "$ALTE_FENSTER" -gt 1 ]; then
    echo "Es laufen noch $ALTE_FENSTER proAVA-Fenster mit der alten Version – bitte neu starten."
fi
ALTE_MCP=$(alte_laeufe proava-mcp)
if [ "$ALTE_MCP" -gt 0 ]; then
    echo "Es läuft noch proava-mcp mit der alten Version ($ALTE_MCP×) – MCP-Client (z. B. opencode) neu starten."
fi
