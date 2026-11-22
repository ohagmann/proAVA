#!/usr/bin/env bash
# Auslieferung bauen: Linux und Windows (Release) plus LICENSE und THIRD-PARTY-NOTICES nach
#   dist/<version>/linux/    proava, proava-mcp, proava-web (Browserfenster, wenn WebKitGTK 4.1
#                            zum Bauen da ist; gestrippt), LICENSE, THIRD-PARTY-NOTICES, Symbole,
#                            LIESMICH.txt, .desktop, MIME-Typen, install_linux.sh (im entpackten
#                            Archiv ausführbar)
#   dist/<version>/windows/  proava.exe, proava-mcp.exe, proava-web.exe, LICENSE.txt, THIRD-PARTY-NOTICES.txt,
#                            LIESMICH.txt,
#                            proava-handbuch.pdf (auch unter linux/; zusätzlich im Programm eingebettet),
#                            dictionaries\LIESMICH.txt (Platz für ein Hunspell-Wörterbuch)
#   dist/<version>/proava-<version>-linux-x86_64.tar.gz, proava-<version>-windows-x86_64.zip
#   dist/<version>/SHA256SUMS
#
#   scripts/release.sh                beides
#   scripts/release.sh --nur-linux    ohne Windows (z. B. ohne Cross-Toolchain)
#   scripts/release.sh --nur-windows  ohne Linux
#   OA_DIST=/ordner scripts/release.sh   anderer Zielordner statt dist/<version>
#
# Vorher: THIRD-PARTY-NOTICES muss zu Cargo.lock passen (sonst Abbruch mit Hinweis).
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
NUR_LINUX=0
NUR_WINDOWS=0
[ "${1:-}" = "--nur-linux" ] && NUR_LINUX=1
[ "${1:-}" = "--nur-windows" ] && NUR_WINDOWS=1

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$VERSION" ] || { echo "FEHLER: Version nicht gefunden." >&2; exit 1; }
DIST="${OA_DIST:-$ROOT/dist/$VERSION}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
# Builds begrenzt wie scripts/check.sh.
begrenzt() {
    systemd-run --user --scope -q -p MemoryMax="${OA_MEMORY_MAX:-24G}" -p TasksMax="${OA_TASKS_MAX:-2048}" \
        nice -n 10 "$@"
}

python3 scripts/third_party_notices.py --check \
    || { echo "Bitte erst: python3 scripts/third_party_notices.py (und committen)." >&2; exit 1; }
begrenzt cargo run -q -p proava-ui --example tastenkuerzel | cmp -s - docs/handbuch/tastenkuerzel.csv \
    || { echo "Handbuch veraltet (Tastenkürzel geändert) – bitte erst: scripts/handbuch.sh (und committen)." >&2; exit 1; }
if [ -n "$(git status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    echo "Hinweis: Arbeitsstand hat ungespeicherte Änderungen – Commit-Angabe im Programm endet auf „+geändert“."
fi

# Nur den Teil neu, der gebaut wird: --nur-windows lässt ein fertiges
# Linux-Paket im selben Ordner stehen (und umgekehrt).
if [ "$NUR_WINDOWS" = 1 ]; then
    rm -rf "$DIST/windows" "$DIST/proava-$VERSION-windows-x86_64.zip"
elif [ "$NUR_LINUX" = 1 ]; then
    rm -rf "$DIST/linux" "$DIST/proava-$VERSION-linux-x86_64.tar.gz"
else
    rm -rf "$DIST"
fi
mkdir -p "$DIST"

if [ "$NUR_WINDOWS" = 0 ]; then
    echo "== Linux (Release)"
    begrenzt cargo build --release -j "${OA_JOBS:-8}" -p proava -p proava-mcp
    mkdir -p "$DIST/linux"
    BINS=(proava proava-mcp)
    # Browserfenster (eigener Workspace, WebKitGTK 4.1); ohne die Entwicklerpakete bleibt
    # es weg – proAVA nimmt dann den System-Browser.
    if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
        begrenzt cargo build --release -j "${OA_JOBS:-8}" \
            --manifest-path apps/proava-web/Cargo.toml --target-dir "$TARGET_DIR"
        BINS+=(proava-web)
    else
        echo "Hinweis: WebKitGTK 4.1 (libwebkit2gtk-4.1-dev) fehlt – Linux-Paket ohne Browserfenster proava-web."
    fi
    for bin in "${BINS[@]}"; do
        cp "$TARGET_DIR/release/$bin" "$DIST/linux/$bin"
        strip "$DIST/linux/$bin" 2>/dev/null || true
    done
    cp LICENSE THIRD-PARTY-NOTICES docs/handbuch/proava-handbuch.pdf assets/linux/LIESMICH.txt "$DIST/linux/"
    mkdir -p "$DIST/linux/icons"
    cp assets/icons/proava-*.png "$DIST/linux/icons/"
    cp assets/linux/proava.desktop assets/linux/proava-mime.xml scripts/install_linux.sh "$DIST/linux/"
    tar -C "$DIST" -czf "$DIST/proava-$VERSION-linux-x86_64.tar.gz" --transform "s|^linux|proava-$VERSION|" linux
fi

if [ "$NUR_LINUX" = 0 ]; then
    echo "== Windows (Release, Cross-Build)"
    scripts/build_windows.sh
    mkdir -p "$DIST/windows"
    for exe in proava.exe proava-mcp.exe proava-web.exe; do
        cp "$TARGET_DIR/x86_64-pc-windows-msvc/release/$exe" "$DIST/windows/"
    done
    # Windows-Zeilenenden, damit Notepad die Dateien ordentlich zeigt.
    sed 's/$/\r/' LICENSE >"$DIST/windows/LICENSE.txt"
    sed 's/$/\r/' THIRD-PARTY-NOTICES >"$DIST/windows/THIRD-PARTY-NOTICES.txt"
    sed 's/$/\r/' assets/windows/LIESMICH.txt >"$DIST/windows/LIESMICH.txt"
    mkdir -p "$DIST/windows/dictionaries"
    sed 's/$/\r/' assets/windows/dictionaries-LIESMICH.txt >"$DIST/windows/dictionaries/LIESMICH.txt"
    cp docs/handbuch/proava-handbuch.pdf "$DIST/windows/"
    # Zip mit Unterordnern (dictionaries), Einträge unter proava-<version>/.
    (cd "$DIST" && rm -f "proava-$VERSION-windows-x86_64.zip" \
        && python3 -c "
import sys, zipfile, os
with zipfile.ZipFile(sys.argv[1], 'w', zipfile.ZIP_DEFLATED) as z:
    for folder, subfolders, files in sorted(os.walk('windows')):
        subfolders.sort()
        inner = os.path.relpath(folder, 'windows')
        for name in sorted(files):
            target = os.path.normpath(os.path.join('proava-' + sys.argv[2], inner, name))
            z.write(os.path.join(folder, name), target)
" "proava-$VERSION-windows-x86_64.zip" "$VERSION")
fi

(cd "$DIST" && find . -type f ! -name SHA256SUMS -printf '%P\n' | sort | xargs sha256sum >SHA256SUMS)

echo
echo "Fertig: $DIST"
(cd "$DIST" && du -h --apparent-size $(find . -maxdepth 2 -type f ! -name '*.png' ! -name '*.xml' ! -name '*.desktop' -printf '%P\n' | sort) | sed 's/^/  /')
