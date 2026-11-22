#!/usr/bin/env bash
# GAEB-Schemas (XSD) und offizielle Muster-/Prüfdateien an einen festen Ort laden,
# der einen Neustart übersteht (nicht /tmp). Prüfstand und Tests finden sie dort
# von selbst; die Dateien kommen nie ins Repository (Urheberrecht GAEB/DIN/BVBS,
# siehe docs/BUILD.md).
#
#   scripts/gaeb_muster_laden.sh            XSDs (3.3, 3.2, 3.1) und Musterdateien
#   scripts/gaeb_muster_laden.sh --nur-xsd  nur die Schemas
#
# Ziel: $PROAVA_GAEB_DATA, sonst ${XDG_DATA_HOME:-~/.local/share}/proava/gaeb
#   xsd/     Schemas (entpackt, je Paket ein Unterordner)
#   muster/  GAEB-Musterdateien und BVBS-Prüfdateien (entpackt)
#   zip/     die geladenen Archive (vorhandene werden nicht neu geladen)
# Braucht curl und python3 (zum Entpacken). Lädt nur von gaeb.de und bvbs.de.

set -euo pipefail

DATA="${PROAVA_GAEB_DATA:-${XDG_DATA_HOME:-$HOME/.local/share}/proava/gaeb}"
ONLY_XSD=0
[ "${1:-}" = "--nur-xsd" ] && ONLY_XSD=1
mkdir -p "$DATA/xsd" "$DATA/muster" "$DATA/zip"

XSD=(
    https://www.gaeb.de/wp-content/uploads/2021/06/2021-05_Leistungsverzeichnis.zip
    https://www.gaeb.de/wp-content/uploads/2019/04/Leistungsverzeichnis.zip
    https://www.gaeb.de/wp-content/uploads/2022/11/Schema_GAEB_DA_XML_3.1_2009-12_X81-X83_u_X85-X87.zip
    https://www.gaeb.de/wp-content/uploads/2022/11/Schema_GAEB_DA_XML_3.1_2009-12_X84.zip
    https://www.gaeb.de/wp-content/uploads/2022/11/schema_gaeb-da-xml-3.1_20-11-2007_X81-X83_u_X85-X88.zip
    https://www.gaeb.de/wp-content/uploads/2022/11/schema_gaeb-da-xml-3.1_20-11-2007_X84.zip
)
MUSTER=(
    https://www.gaeb.de/wp-content/uploads/2022/11/Musterdateien_GAEB_DA_XML_3.1_2009-12.zip
    https://www.gaeb.de/wp-content/uploads/2022/11/Musterdateien_GAEB_DA_XML_3.1_20-11-2007.zip
)

# get <url> <zielordner>: Archiv laden (wenn noch nicht da) und entpacken.
get() {
    local url="$1" target="$2" name
    name="$(basename "$url")"
    if [ ! -s "$DATA/zip/$name" ]; then
        echo "lade $name"
        curl -sSfL --max-time 120 -o "$DATA/zip/$name.part" "$url"
        mv "$DATA/zip/$name.part" "$DATA/zip/$name"
    fi
    python3 -I - "$DATA/zip/$name" "$target/${name%.zip}" <<'EOF'
import sys, zipfile, os
archive, target = sys.argv[1], sys.argv[2]
os.makedirs(target, exist_ok=True)
with zipfile.ZipFile(archive) as z:
    for info in z.infolist():
        # Pfade nur unterhalb des Zielordners (keine absoluten oder ../-Pfade).
        path = os.path.normpath(os.path.join(target, info.filename))
        if not path.startswith(os.path.abspath(target) + os.sep) and path != os.path.abspath(target):
            path = os.path.join(target, os.path.basename(info.filename))
        if info.is_dir():
            os.makedirs(path, exist_ok=True)
            continue
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with z.open(info) as src, open(path, "wb") as dst:
            dst.write(src.read())
EOF
}

for url in "${XSD[@]}"; do get "$url" "$DATA/xsd"; done
# Dem X84-Paket 2007 fehlen xmldsig/xml.xsd: aus dem Nachbarpaket nehmen.
for need in xmldsig-core-schema.xsd xml.xsd; do
    found="$(find "$DATA/xsd" -name "$need" -print -quit)"
    for dir in "$DATA"/xsd/schema_gaeb-da-xml-3.1_20-11-2007_X84*; do
        [ -n "$found" ] && [ -d "$dir" ] && [ ! -e "$dir/$need" ] && cp "$found" "$dir/" || true
    done
done

if [ "$ONLY_XSD" = 0 ]; then
    for url in "${MUSTER[@]}"; do get "$url" "$DATA/muster/gaeb"; done
    # BVBS-Prüfdateien der Zertifizierung: Liste aus der öffentlichen Medienliste von bvbs.de.
    python3 -I -c '
import json, urllib.parse, urllib.request
urls = set()
for term in ("Pruefdatei", "Prüfdatei", "2008-001"):
    query = urllib.parse.urlencode({"search": term, "per_page": 100})
    try:
        with urllib.request.urlopen("https://www.bvbs.de/wp-json/wp/v2/media?" + query, timeout=60) as answer:
            for item in json.load(answer):
                url = item.get("source_url", "")
                if url.lower().endswith(".zip"):
                    urls.add(url)
    except (OSError, ValueError):
        pass
print("\n".join(sorted(urls)))' | while read -r url; do
        [ -n "$url" ] && get "$url" "$DATA/muster/bvbs"
    done
fi

echo
echo "XSDs:   $(find "$DATA/xsd" -iname '*.xsd' | wc -l) Dateien in $DATA/xsd"
echo "Muster: $(find "$DATA/muster" -iname '*.x8?' -o -iname '*.d8?' | wc -l) GAEB-Dateien in $DATA/muster"
echo "Prüfstand: PROAVA_GAEB_TESTDATA=$DATA/muster:<weitere Ordner> cargo run --release -p proava-gaeb --example gaeb_check"
