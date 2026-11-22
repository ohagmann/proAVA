#!/usr/bin/env python3
"""Validates GAEB DA XML 3.3 files against the official GAEB XSDs.

Usage: validate_xsd.py <xsd-folder|-> <file-or-folder> ...

The XSDs are searched in <xsd-folder> and all its subfolders. With "-" (or a
folder without them) the usual places are searched: $PROAVA_GAEB_XSD_DIR,
$PROAVA_GAEB_DATA/xsd, ${XDG_DATA_HOME:-~/.local/share}/proava/gaeb/xsd.

The XSDs are not part of the repository (copyright DIN/GAEB). Load them with
scripts/gaeb_muster_laden.sh (from https://www.gaeb.de, "2021-05_Leistungsverzeichnis.zip").
Uses xmllint when installed, otherwise python3-lxml. Problems with the
schemas themselves go to stdout as well, so a calling test shows them.
"""
import glob
import os
import shutil
import subprocess
import sys


def candidates(given):
    """Folders to search for the XSDs, the given one first."""
    folders = []
    if given and given != "-":
        folders.append(given)
    for variable in ("PROAVA_GAEB_XSD_DIR",):
        if os.environ.get(variable):
            folders.append(os.environ[variable])
    if os.environ.get("PROAVA_GAEB_DATA"):
        folders.append(os.path.join(os.environ["PROAVA_GAEB_DATA"], "xsd"))
    data_home = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
    folders.append(os.path.join(data_home, "proava", "gaeb", "xsd"))
    return folders


def schema_for(folders, phase):
    for folder in folders:
        pattern = os.path.join(folder, "**", f"GAEB_DA_XML_{phase}_3.3_*.xsd")
        matches = sorted(glob.glob(pattern, recursive=True))
        if matches:
            return matches[-1]
    print(f"Kein Schema GAEB_DA_XML_{phase}_3.3_*.xsd gefunden in: {', '.join(folders)}"
          " – laden mit scripts/gaeb_muster_laden.sh")
    sys.exit(2)


def files(paths):
    for path in paths:
        if os.path.isdir(path):
            for name in sorted(os.listdir(path)):
                if name[-4:-1].upper() == ".X8":
                    yield os.path.join(path, name)
        else:
            yield path


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    folders = candidates(sys.argv[1])
    xmllint = shutil.which("xmllint")
    schemas = {}
    failures = 0
    count = 0
    for path in files(sys.argv[2:]):
        phase = path[-2:]
        count += 1
        if xmllint:
            result = subprocess.run(
                [xmllint, "--noout", "--schema", schema_for(folders, phase), path],
                capture_output=True, text=True)
            ok, errors = result.returncode == 0, result.stderr.strip().splitlines()
        else:
            from lxml import etree
            if phase not in schemas:
                schemas[phase] = etree.XMLSchema(etree.parse(schema_for(folders, phase)))
            schema = schemas[phase]
            ok = schema.validate(etree.parse(path))
            errors = [f"{e.line}: {e.message}" for e in schema.error_log]
        if not ok:
            failures += 1
            print(f"UNGÜLTIG {path}")
            for line in errors[:5]:
                print(f"   {line}")
    print(f"{count} Dateien geprüft, {failures} ungültig")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
