#!/usr/bin/env bash
# Handbuch übersetzen: Tastenkürzel-Tabelle aus dem Programm erzeugen, dann
# docs/handbuch/handbuch.typ → docs/handbuch/proava-handbuch.pdf (Typst ≥ 0.13,
# Schriften aus crates/proava-print/fonts). Das PDF ist ins Programm eingebettet
# (Hilfe ▸ Handbuch) – nach Änderungen neu bauen und mit committen.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
cargo run -q -p proava-ui --example tastenkuerzel >docs/handbuch/tastenkuerzel.csv
# --root: das Handbuch liest den Lizenztext unverändert aus LICENSE (Anhang).
typst compile --root "$ROOT" --font-path crates/proava-print/fonts --ignore-system-fonts \
    docs/handbuch/handbuch.typ docs/handbuch/proava-handbuch.pdf
echo "geschrieben: docs/handbuch/proava-handbuch.pdf ($(du -h docs/handbuch/proava-handbuch.pdf | cut -f1))"
