#!/usr/bin/env python3
"""Erzeugt THIRD-PARTY-NOTICES: alle Crates, die in proava, proava-mcp und das
Browserfenster proava-web (eigener Workspace apps/proava-web) eingebaut werden
(Linux und Windows), mit Version, Lizenz und Quelle, dazu die
Lizenz- und NOTICE-Dateien aus den Crate-Quellen (gleiche Texte nur einmal),
die eingebetteten Schriften und die Datensätze von Typst.

Nur `cargo metadata` (kein cargo-about nötig). Aufruf vom Repo-Ordner:

    python3 scripts/third_party_notices.py            # schreibt THIRD-PARTY-NOTICES
    python3 scripts/third_party_notices.py --check    # nur prüfen, ob die Datei aktuell ist
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-NOTICES"
# (Workspace-Ordner, Programme): das Hauptprogramm und das Browserfenster.
WORKSPACES = [
    (ROOT, ["proava", "proava-mcp"]),
    (ROOT / "apps" / "proava-web", ["proava-web"]),
]
PLATFORMS = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"]
LICENSE_NAMES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "AUTHORS")
MAX_FILE = 200_000

EXTRA = [
    (
        "Liberation Sans (Schrift, in proava eingebettet)",
        "SIL Open Font License 1.1",
        "https://github.com/liberationfonts",
        ROOT / "crates" / "proava-print" / "fonts" / "OFL.txt",
    ),
    (
        "Microsoft Edge WebView2 Loader (WebView2LoaderStatic.lib aus dem WebView2 SDK 1.0.3650.58, "
        "über webview2-com-sys statisch in proava-web.exe gebunden)",
        "Microsoft, BSD-artige Lizenz (Text unten)",
        "https://www.nuget.org/packages/Microsoft.Web.WebView2",
        ROOT / "assets" / "licenses" / "webview2-sdk-LICENSE.txt",
    ),
]


MIT_TEMPLATE = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

ZERO_BSD = """Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE."""


def standard_texts(packages: list[dict]) -> list[tuple[str, str]]:
    """Full standard texts for crates that ship no licence file, taken from
    other crates of the same Cargo.lock (Apache-2.0, BSL-1.0) or the SPDX text."""
    folders = {pkg["name"]: Path(pkg["manifest_path"]).parent for pkg in packages}
    def first(candidates: list[tuple[str, str]]) -> str:
        for crate, name in candidates:
            folder = folders.get(crate)
            if folder and (folder / name).is_file():
                return (folder / name).read_text(encoding="utf-8").strip()
        return ""
    texts = [
        ("Apache-2.0", first([("ryu", "LICENSE-APACHE"), ("serde", "LICENSE-APACHE"), ("itoa", "LICENSE-APACHE")])),
        ("MIT (Vorlage; Copyright: die beim Crate genannten Autoren)", MIT_TEMPLATE),
        ("BSL-1.0", first([("ryu", "LICENSE-BOOST"), ("error-code", "LICENSE")])),
        ("0BSD (Copyright: die beim Crate genannten Autoren)", ZERO_BSD),
    ]
    return [(title, text) for title, text in texts if text]


def cargo_metadata(platform: str, folder: Path) -> dict:
    cargo = os.environ.get("CARGO") or shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo")
    out = subprocess.run(
        [cargo, "metadata", "--format-version", "1", "--locked", "--filter-platform", platform],
        cwd=folder,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def shipped(meta: dict, roots: list[str]) -> set[str]:
    """Package ids reachable from the programs over normal dependencies."""
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    names = {pkg["id"]: pkg["name"] for pkg in meta["packages"]}
    todo = [pid for pid, name in names.items() if name in roots and pid in meta["workspace_members"]]
    seen: set[str] = set()
    while todo:
        pid = todo.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    return seen


def license_files(manifest: Path) -> list[Path]:
    folder = manifest.parent
    files = []
    for entry in sorted(folder.iterdir()):
        upper = entry.name.upper()
        if entry.is_file() and upper.startswith(LICENSE_NAMES) and entry.stat().st_size <= MAX_FILE:
            files.append(entry)
        elif entry.is_dir() and upper in ("LICENSES", "LICENSE"):
            files.extend(sorted(p for p in entry.iterdir() if p.is_file() and p.stat().st_size <= MAX_FILE))
    return files


def build() -> str:
    packages: dict[str, dict] = {}
    for folder, roots in WORKSPACES:
        for platform in PLATFORMS:
            meta = cargo_metadata(platform, folder)
            by_id = {pkg["id"]: pkg for pkg in meta["packages"]}
            members = set(meta["workspace_members"])
            for pid in shipped(meta, roots):
                # Eigene Crates (Workspace oder Pfad, z. B. proava-web-protocol) zählen nicht.
                if pid not in members and by_id[pid].get("source") is not None:
                    packages[pid] = by_id[pid]

    rows = sorted(packages.values(), key=lambda pkg: (pkg["name"].lower(), pkg["version"]))
    texts: dict[str, tuple[str, list[str]]] = {}
    per_crate: dict[str, list[str]] = {}
    for pkg in rows:
        key = f'{pkg["name"]} {pkg["version"]}'
        refs = []
        for file in license_files(Path(pkg["manifest_path"])):
            text = file.read_text(encoding="utf-8", errors="replace").strip()
            digest = hashlib.sha256(" ".join(text.split()).encode()).hexdigest()[:12]
            if digest not in texts:
                texts[digest] = (text, [])
            texts[digest][1].append(f"{key} ({file.name})")
            refs.append(digest)
        per_crate[key] = refs

    licenses: dict[str, int] = {}
    for pkg in rows:
        licenses[pkg.get("license") or "siehe Lizenzdatei"] = licenses.get(pkg.get("license") or "siehe Lizenzdatei", 0) + 1

    out = []
    out.append("proAVA – Hinweise zu Bestandteilen Dritter (THIRD-PARTY-NOTICES)")
    out.append("=" * 68)
    out.append("")
    out.append("proAVA selbst: © 2026 Oliver Hagmann, Business Source License 1.1 (Datei LICENSE;")
    out.append("kommerzielle Nutzung erlaubt, nicht als Produkt verkaufen oder als SaaS anbieten;")
    out.append("ab 20.11.2030 AGPL-3.0). Ohne Gewährleistung, soweit gesetzlich zulässig.")
    out.append("Diese Datei wird von scripts/third_party_notices.py aus Cargo.lock erzeugt –")
    out.append("nicht von Hand ändern. Sie gilt für proava, proava-mcp und das Browserfenster")
    out.append("proava-web unter Linux und Windows (x86_64).")
    out.append("")
    out.append("Übersicht der Lizenzen (Anzahl Crates):")
    for name, count in sorted(licenses.items(), key=lambda item: (-item[1], item[0])):
        out.append(f"  {count:4}  {name}")
    out.append("")
    out.append("Weitere Bestandteile")
    out.append("-" * 20)
    for title, license_name, source, _ in EXTRA:
        out.append(f"{title}")
        out.append(f"    Lizenz: {license_name}")
        out.append(f"    Quelle: {source}")
    out.append("Systembibliotheken des Browserfensters (nicht mitgeliefert, zur Laufzeit vom")
    out.append("System geladen): unter Linux WebKitGTK 4.1 und GTK 3 (LGPL-2.1 oder später),")
    out.append("unter Windows die Microsoft Edge WebView2 Runtime (Teil von Windows).")
    out.append("Datensätze, die Typst mitbringt (Literaturstile von hayagriva/typst-assets,")
    out.append("Syntaxdefinitionen von two-face): Lizenzhinweise in den Lizenzdateien der")
    out.append("jeweiligen Crates unten.")
    out.append("")
    out.append(f"Crates ({len(rows)})")
    out.append("-" * 20)
    for pkg in rows:
        key = f'{pkg["name"]} {pkg["version"]}'
        source = pkg.get("repository") or pkg.get("homepage") or f'https://crates.io/crates/{pkg["name"]}'
        out.append(f"{key}")
        out.append(f"    Lizenz: {pkg.get('license') or 'siehe Lizenzdatei'}")
        if pkg.get("authors"):
            out.append(f"    Autoren: {', '.join(pkg['authors'])}")
        out.append(f"    Quelle: {source}")
        refs = per_crate[key]
        out.append(f"    Lizenztexte: {', '.join('[' + r + ']' for r in refs) if refs else 'keine Datei im Paket – Standardtext unten (Abschnitt „Standardtexte“)'}")
    out.append("")
    out.append("Lizenztexte")
    out.append("-" * 20)
    out.append("")
    out.append("Standardtexte (für Crates ohne eigene Lizenzdatei)")
    for title, text in standard_texts(rows):
        out.append("")
        out.append(f"===== {title} =====")
        out.append(text)
    for title, license_name, _, path in EXTRA:
        out.append("")
        out.append(f"===== {title} – {license_name} =====")
        out.append(path.read_text(encoding="utf-8").strip())
    for digest, (text, users) in texts.items():
        out.append("")
        out.append(f"===== [{digest}] =====")
        shown = users[:8]
        more = f" und {len(users) - 8} weitere" if len(users) > 8 else ""
        out.append(f"Für: {', '.join(shown)}{more}")
        out.append("")
        out.append(text)
    out.append("")
    return "\n".join(out)


def main() -> int:
    text = build()
    if "--check" in sys.argv:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if current != text:
            print("THIRD-PARTY-NOTICES ist veraltet – scripts/third_party_notices.py ausführen.")
            return 1
        print("THIRD-PARTY-NOTICES ist aktuell.")
        return 0
    OUT.write_text(text, encoding="utf-8")
    print(f"geschrieben: {OUT} ({len(text) // 1024} KB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
