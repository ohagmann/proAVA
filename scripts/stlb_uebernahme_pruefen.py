#!/usr/bin/env python3
"""STLB-Bau-Übernahme prüfen: GAEB-Dateien (z. B. aus STLB-Bau Online) über den
MCP-Server in ein frisches proAVA-Projekt übernehmen, als X83 exportieren und je
STLB-Bau-Position vergleichen, ob Kennung (STLBBau) und Textergänzungen (Marke,
Art, ArtChrIdent, Zahlenwert, Ansprache, Wert, Nachsatz) unverändert ankommen –
das, was STLB-Bau Check bei öffentlichen Auftraggebern bemängeln würde.

Aufruf (liest die Dateien nur; Projekte und Exporte in einem Temp-Ordner):

    cargo build --release -p proava-mcp
    python3 -I scripts/stlb_uebernahme_pruefen.py DATEI.X83 [WEITERE …]

Umgebung: PROAVA_MCP (Pfad des Servers, Standard target/release/proava-mcp
neben diesem Skript), PROAVA_PRUEF_ORDNER (Arbeitsordner, Standard ein neuer
Temp-Ordner). Ergebnis je Datei: Anzahl STLB-Positionen, davon gleich, die
ersten Unterschiede. Exit-Code 1, wenn etwas abweicht.
"""

import html
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def server(mcp, folders):
    env = {"PROAVA_MCP_ORDNER": folders, "HOME": tempfile.mkdtemp(), "PATH": "/usr/bin:/bin"}
    proc = subprocess.Popen([mcp], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, text=True, env=env)
    counter = [0]

    def call(method, params=None, notify=False):
        counter[0] += 1
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        if not notify:
            message["id"] = counter[0]
        proc.stdin.write(json.dumps(message) + "\n")
        proc.stdin.flush()
        return None if notify else json.loads(proc.stdout.readline())

    call("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": {"name": "stlb-pruefen", "version": "1"}})
    call("notifications/initialized", notify=True)

    def tool(name, arguments):
        result = call("tools/call", {"name": name, "arguments": arguments}).get("result", {})
        return result.get("isError", False), result.get("content", [{}])[0].get("text", "")

    return proc, tool


def text_of(xml):
    text = re.sub(r"^\s+<", "<", xml)
    text = re.sub(r">\s+$", ">", text)
    text = re.sub(r">\s+<", "><", text)
    text = re.sub(r"<br\s*/>", "\n", text)
    return html.unescape(re.sub(r"<[^>]+>", "", text))


def stlb_items(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        content = handle.read()
    items = []
    for match in re.finditer(r"<Item\b.*?</Item>", content, re.S):
        item = match.group(0)
        key = re.search(r"<STLBBau>.*?</STLBBau>", item, re.S)
        if not key:
            continue
        complements = []
        for complement in re.findall(r"<TextComplement\b.*?</TextComplement>", item, re.S):
            head = re.match(r"<TextComplement([^>]*)>", complement).group(1)
            attributes = dict(re.findall(r'(\w+)="([^"]*)"', head))
            number = re.search(r'<ComplBody(Dec|Int) Value="([^"]*)"', complement)

            def part(name):
                found = re.search(r"<%s>(.*?)</%s>" % (name, name), complement, re.S)
                return text_of(found.group(1)) if found else ""

            body = part("ComplBody").strip().strip("'").strip()
            if set(body) <= set(".… \n"):
                body = ""
            complements.append((attributes.get("MarkLbl"), attributes.get("Kind"),
                                attributes.get("ArtChrIdent"),
                                number.groups() if number else None,
                                part("ComplCaption"), body, part("ComplTail")))
        items.append((re.sub(r"\s+", "", key.group(0)), complements))
    return items


def main(files):
    if not files:
        print(__doc__)
        return 2
    mcp = os.environ.get("PROAVA_MCP", os.path.join(HERE, "..", "target", "release", "proava-mcp"))
    work = os.environ.get("PROAVA_PRUEF_ORDNER") or tempfile.mkdtemp(prefix="stlb-pruefen-")
    sources = [os.path.realpath(file) for file in files]
    folders = ":".join([work] + sorted({os.path.dirname(source) for source in sources}))
    proc, tool = server(mcp, folders)
    failed = False
    for number, source in enumerate(sources, 1):
        folder = os.path.join(work, f"p{number:02d}")
        os.makedirs(folder, exist_ok=True)
        project = os.path.join(folder, "T", "T.proava")
        tool("projekt_anlegen", {"ordner": folder, "name": "T"})
        tool("positionen_einfuegen", {"projekt": project, "zeilen": [
            {"titel": "A", "ebene": 1}, {"titel": "B", "ebene": 2}]})
        error, answer = tool("stlb_uebernehmen", {"projekt": project, "datei": source})
        if error:
            print(f"{os.path.basename(source)}: {answer}")
            failed = True
            continue
        export = os.path.join(folder, "export.X83")
        tool("gaeb_exportieren", {"projekt": project, "art": "X83", "ziel": export,
                                  "ueberschreiben": True})
        written = {}
        for key, complements in stlb_items(export):
            written.setdefault(key, []).append(complements)
        original = stlb_items(source)
        same, differences = 0, []
        for key, complements in original:
            candidates = written.get(key)
            if not candidates:
                differences.append(("Kennung fehlt", key[:80]))
                continue
            again = candidates.pop(0)
            if again == complements:
                same += 1
            else:
                pair = next(((a, b) for a, b in zip(complements, again) if a != b),
                            ("Anzahl Textergänzungen", len(complements), len(again)))
                differences.append(pair)
        failed |= bool(differences)
        print(f"{os.path.basename(source)}: {len(original)} STLB-Bau-Positionen, gleich {same}")
        for difference in differences[:3]:
            print("    Unterschied:", repr(difference)[:300])
    proc.stdin.close()
    proc.wait()
    print(f"Arbeitsordner: {work}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
