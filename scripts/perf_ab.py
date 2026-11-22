#!/usr/bin/env python3
"""Leistung vorher/nachher: zwei (oder mehr) gebaute Messprogramme abwechselnd
laufen lassen und je Messung den Median ausgeben (Markdown-Tabelle).

  cargo build --release -p proava-ui --example perf   # je Stand, Datei kopieren
  RUNS=5 scripts/perf_ab.py /tmp/perf-vorher /tmp/perf-nachher

Abwechselnd, weil die Maschine oft von anderen Builds belastet ist: so trifft
eine Lastspitze beide Stände. Umgebung: RUNS (Standard 3), SKIP (Standard
„pdf“ = PDFs auslassen; leer = mit PDFs), dazu alle PROAVA_PERF_*-Variablen.
Siehe docs/BUILD.md (Messprogramme)."""
import os, subprocess, sys, statistics, re, tempfile
runs = int(os.environ.get("RUNS", "3"))
bins = sys.argv[1:]
results = {b: {} for b in bins}
order = []
for r in range(runs):
    for b in bins:
        with tempfile.NamedTemporaryFile(suffix=".md", delete=False) as handle:
            out = handle.name
        env = dict(os.environ, PROAVA_PERF_SKIP=os.environ.get("SKIP", "pdf"), PROAVA_PERF_REPORT=out)
        subprocess.run([b], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        for line in open(out).read().splitlines()[2:]:
            cells = [c.strip() for c in line.strip("|").split("|")]
            name, value = cells[0], cells[1]
            if name not in order:
                order.append(name)
            m = re.match(r"([\d.]+) (ms|MB)", value)
            if m:
                results[b].setdefault(name, []).append(float(m.group(1)))
        os.unlink(out)
names = [os.path.basename(b) for b in bins]
print("| Messung | " + " | ".join(names) + " |")
print("|---|" + "---:|" * len(bins))
for name in order:
    cells = []
    for b in bins:
        vals = results[b].get(name)
        cells.append(f"{statistics.median(vals):.1f}" if vals else "–")
    print(f"| {name} | " + " | ".join(cells) + " |")
