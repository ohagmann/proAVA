# proAVA bauen und entwickeln

Ziel: je Plattform **eine** Binärdatei ohne Installer und ohne mitgelieferte DLLs/Bibliotheken. Programme:
`proava` (Fenster, egui/eframe mit OpenGL über `glow`), `proava-mcp` (MCP-Server über stdin/stdout, `docs/MCP.md`)
und optional `proava-web` (Browserfenster für STLB-Bau Online und AUSSCHREIBEN.DE). PDF-Druck läuft über
eingebettetes Typst mit eingebetteten Liberation-Schriften – keine Systemschriften, keine Zusatzdateien.

Alle Befehle vom Repo-Ordner aus. Auf einem Rechner, an dem nebenher gearbeitet wird, begrenzt bauen
(`scripts/check.sh`, `build_windows.sh` und `release.sh` tun das selbst):

```bash
export PATH="$HOME/.cargo/bin:$PATH"
systemd-run --user --scope -q -p MemoryMax=24G -p TasksMax=2048 nice -n 10 cargo build --release -j 8 -p proava -p proava-mcp
```

## Aufbau

Die Crates stehen im `README.md` („Aufbau“). Abhängigkeiten zeigen nach innen: Die Domäne kennt niemanden, die
Oberfläche kennt keinen Speicher, die Infrastruktur setzt nur Ports um. `proava` und `proava-mcp` dürfen keine
Browser-Engine im Abhängigkeitsbaum haben (`proava-web` ist ein eigener Workspace; geprüft von
`crates/proava-web-protocol/tests/no_browser_engine.rs` und `scripts/check.sh`). Fachliche Grundlagen (Normen,
GAEB-Besonderheiten, Fachregeln): `docs/FACHLICHES.md`.

## Linux (x86_64)

```bash
cargo build --release -p proava -p proava-mcp
```

Ergebnis `target/release/proava` und `target/release/proava-mcp`. Gelinkt nur gegen `libc`, `libm`, `libgcc_s`;
benötigt glibc ≥ 2.35 (Ubuntu 22.04, Debian 12 und neuer). `objdump -T` zeigt auch `GLIBC_2.39`
(`pidfd_spawnp`, `pidfd_getpid`) – schwache Symbole, die Rust-std nur nutzt, wenn vorhanden. Fenstersystem
(X11/Wayland, `libxkbcommon`) und OpenGL/EGL lädt winit/glutin zur Laufzeit.

**Browserfenster `proava-web`** (optional, eigener Cargo-Workspace mit wry + tao):

```bash
cargo build --release --manifest-path apps/proava-web/Cargo.toml --target-dir target
```

Zum Bauen `libwebkit2gtk-4.1-dev`, zur Laufzeit `libwebkit2gtk-4.1-0`. `--target-dir target` legt es neben
`proava`, wo proAVA es sucht. `scripts/check.sh` baut es mit, wenn `pkg-config` WebKitGTK 4.1 findet. Fehlt das
Fenster, nimmt proAVA den System-Browser.

## Windows (x86_64) – Cross-Build unter Linux

```bash
scripts/build_windows.sh            # Release
scripts/build_windows.sh --debug    # Debug (mit Konsolenfenster)
scripts/build_windows.sh --tests    # alle Tests für Windows übersetzen (nicht ausführen)
scripts/build_windows.sh --clippy   # Clippy für Windows (alle Crates und Tests)
```

Ergebnis `target/x86_64-pc-windows-msvc/release/proava.exe` (GUI-Programm), `proava-mcp.exe`
(Konsolenprogramm) und `proava-web.exe`, mit Symbol und Versionsinfo; die `.pdb` wird nicht ausgeliefert.
`--tests` übersetzt auch den Code hinter `#[cfg(windows)]`. **Unter Windows 11 nur kurz erprobt (Start, Projekt
laden); nicht umfassend getestet.**

So geht es (wie `cargo-xwin`, aber mit vorhandenem Sysroot und ohne Download):

- Rust-Ziel `x86_64-pc-windows-msvc` (`rustup target add x86_64-pc-windows-msvc`).
- Linker `lld-link` mit `/winsysroot:<Sysroot>`; C-Teile (`psm`/`stacker` aus Typst) baut `clang-cl`, MASM ist
  nicht nötig. `+crt-static`: keine `VCRUNTIME140.dll` nötig.
- Umgebung: `OA_LLVM_BIN` (LLVM 20 mit `clang-cl`, `lld-link`, `llvm-lib`, `llvm-rc`), `OA_WINSYSROOT` (MSVC und
  Windows SDK im Layout von `xwin --use-winsysroot-style`), `OA_JOBS` (8), `OA_MEMORY_MAX` (24G), `OA_TASKS_MAX`
  (2048). MSVC-Bibliotheken und Windows SDK unterliegen der Microsoft-Lizenz; wer baut, holt sie selbst. Das Skript
  lädt nichts nach, nichts davon gehört ins Repo.
- Direkt unter Windows: Visual Studio Build Tools (C++) und rustup, dann
  `cargo build --release -p proava -p proava-mcp`.

Abhängigkeiten der EXEs sind nur Windows-Systembibliotheken (`kernel32`, `user32`, `gdi32`, `OPENGL32`, `ole32`,
`shell32` …; prüfen mit `llvm-objdump -p … | grep "DLL Name"`). Programmsymbol und Versionsinfo kommen aus
`apps/windows_resource.rs` (Ressourcenskript, mit `llvm-rc` bzw. `rc.exe` übersetzt; fehlt der Compiler, gibt es
eine Warnung und die EXE ohne Symbol). Kein Manifest, die DPI-Bewusstheit setzt winit.

**Windows im Code:**

| Thema | Umsetzung |
|---|---|
| Einstellungen | `%APPDATA%\proava\settings.json` (Roaming-Profil); Linux `~/.config/proava` |
| Dateien öffnen (PDF, Handbuch, Webseiten) | `ShellExecuteW` „open“ (`jobs::open_with_system`): kein `cmd`, kein Konsolenfenster |
| Dateidialog | Laufwerke aus `GetLogicalDrives`; Pfadteile führen zur Wurzel (`C:\`, `\\server\freigabe\`); UNC-Pfade wie andere Pfade |
| Groß-/Kleinschreibung | Endungen ohne Rücksicht darauf; gleiche Dateien über `launcher::same_file` |
| Download-Ordner | Known Folder „Downloads“, dann `%USERPROFILE%\Downloads` |
| Wörterbücher | gewählter Ordner, `dictionaries\` neben der EXE, dann LibreOffice/OpenOffice |
| Speichern | Zwischendatei und Umbenennen; kurz gesperrte Ziele (Virenscanner, OneDrive) bis 1 s wiederholt; offene PDFs (Acrobat) mit eigener Meldung |
| Start mit Datei | `.proava`, GAEB, CSV; `file:///C:/…` wird `C:/…`, relative Pfade werden absolut (`std::path::absolute`, kein `\\?\`) |
| Kommandozeile | GUI-Programm ohne Konsole; `--version`/`--help` schreiben über `AttachConsole(ATTACH_PARENT_PROCESS)` |
| Projektsperre | `.~lock.<Datei>#` wie LibreOffice; `process_alive` über `OpenProcess`, `GetExitCodeProcess`, `QueryFullProcessImageNameW` – als proAVA gilt nur der Programmname |
| PDF aus dem Schriftverkehr öffnen | Kopie nach `%TEMP%\proava-schriftverkehr-<PID>\` mit bereinigtem Namen (`copy_name`, immer `.pdf`), dann `ShellExecuteW` |
| Web-Adressen | nur geprüfte `https`-Adressen (`tender_index::is_item_page`); `curl.exe` nur aus `%SystemRoot%\System32`, ohne Konsolenfenster |
| MCP-Pfade | `PROAVA_MCP_ORDNER` mit `;` getrennt; `\\?\`-Präfixe aus `fs::canonicalize` werden für Antworten entfernt |
| Zeilenenden | GAEB 90 schreibt CRLF und liest beides; Textdateien im Paket mit CRLF |
| Zusatzfenster vor dem Hauptfenster | `owner_window.rs`: Windows `SetWindowLongPtrW(GWLP_HWNDPARENT)`, Linux X11 `WM_TRANSIENT_FOR` |
| Lokale Brücke (KI, Ziehen zwischen Fenstern) | `%LOCALAPPDATA%\proava\fenster\<pid>.json`, Named Pipe mit DACL nur für Besitzer und SYSTEM, `PIPE_REJECT_REMOTE_CLIENTS` |
| F10 | winit reicht `WM_SYSKEYUP` nur bei nativer Menüleiste an `DefWindowProc`; proAVA hat keine, also kein Systemmenü |

**Grenzen:** Das Fenster braucht OpenGL ≥ 2.0; in VMs oder per RDP ohne GPU-Treiber hilft Mesa `opengl32.dll`
(llvmpipe) neben der EXE. Die EXEs sind unsigniert (SmartScreen fragt beim ersten Start). Sperrdatei und
Autosicherung (`.~lock.<Datei>#`, `.<Datei>.autosave`) sind im Explorer sichtbar. Die Sperre wird mit `fs::write`
geschrieben, nicht exklusiv. Pfade nie als Text mit `/` zusammensetzen, sondern mit `Path::join`; Tests mit
Linux-Pfaden mit `#[cfg(windows)]`-Gegenstück versehen und mit `--tests` übersetzen.

## macOS

Der Code ist bereit (`directories` legt Einstellungen unter `~/Library/Application Support/proava` ab,
eframe/glow läuft über das OpenGL von macOS), gebaut und erprobt ist es dort nicht. Cross-Build von Linux braucht das
macOS-SDK aus Xcode, dessen Lizenz nur Apple-Hardware erlaubt. Auf einem Mac (Xcode Command Line Tools und rustup)
oder per CI mit macOS-Runner:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin -p proava -p proava-mcp
cargo build --release --target x86_64-apple-darwin  -p proava -p proava-mcp
lipo -create -output proava target/aarch64-apple-darwin/release/proava target/x86_64-apple-darwin/release/proava
```

Für die Weitergabe zusätzlich `.app`-Bündel, Signatur mit Apple-Developer-ID und Notarisierung.

## Prüfen

`scripts/check.sh` vor jedem Commit: Format, Build, alle Tests (auch die Oberfläche ohne Bildschirm über
egui_kittest), Clippy, „ohne Browser-Engine“, Doku-Regeln (unten). Umgebung `JOBS`, `MEMORY_MAX`, `TASKS_MAX`,
`CPU_QUOTA`; Builds und Tests laufen in einem systemd-Scope.

**Temp-Ordner in Tests:** jeder Test über `tempfile` mit eindeutigem Namen (Hilfen `TempDir`/`TempFolder`), nie ein
fester Name oder nur die PID, nie Ordner nach Muster löschen (`crates/proava-ui/tests/test_ordner.rs`).

**Eigenschafts- und Zufallstests** (`proptest`): Bearbeitungsfolgen (`crates/proava-ui/tests/eigenschaften_lv.rs`),
Rundlauf der Projektdatei und GAEB (`crates/proava-exchange/tests/eigenschaften_rundlauf.rs`), kaputte
Projekt-/GAEB-Dateien (`eigenschaften_kaputt.rs`), Kostenermittlung (`crates/proava-domain/tests/eigenschaften_kosten.rs`),
kaputte `settings.json` und der Affentest der ganzen Oberfläche (`crates/proava-ui/tests/affentest.rs`). Standardläufe
mit fester Saat sind Teil von `cargo test`. Längere Läufe:

```bash
PROAVA_PROPTEST_CASES=5000 PROAVA_PROPTEST_SEED=7 cargo test -p proava-ui --test eigenschaften_lv
cargo test --workspace -- --ignored lang_      # teure Läufe (#[ignore], Name lang_…)
scripts/eigenschaften.sh 8 5000                 # alle Eigenschaftstests mit mehreren Saaten, begrenzt
```

`PROAVA_AFFE_TRACE=alle` zeigt bei einem Fehlschlag den ganzen Verlauf des Affentests, `PROAVA_PROPTEST_STATS=1`
eine Statistik der Zufallsdaten. Ein gefundener Fall wird fester Test und in der Domäne behoben.

**GAEB-Schemas und Musterdateien:** Die offiziellen XSDs und Muster-/Prüfdateien (GAEB, BVBS) dürfen nicht ins Repo.
`scripts/gaeb_muster_laden.sh` holt sie nach `${XDG_DATA_HOME:-~/.local/share}/proava/gaeb` (`xsd/`, `muster/`;
anderer Ort mit `PROAVA_GAEB_DATA`, nur XSDs mit `--nur-xsd`). Tests und `scripts/validate_xsd.py` finden die XSDs
selbst (`proava_gaeb::schemas`: `PROAVA_GAEB_XSD_DIR`, `$PROAVA_GAEB_DATA/xsd`, dann der Standardort); ohne XSDs
prüfen die Tests kein Schema, die ausdrücklichen Schematests (`--ignored`) nennen dann die durchsuchten Ordner. Prüfstand über einen Ordner mit GAEB-Dateien (nur lesen):

```bash
PROAVA_GAEB_TESTDATA="$HOME/.local/share/proava/gaeb/muster:<weitere Ordner>" \
PROAVA_GAEB_OUT=/tmp/gaeb/rt PROAVA_GAEB_REPORT=/tmp/gaeb/bericht.md \
  cargo run -p proava-gaeb --example gaeb_check --release
```

Je Datei: lesen, Summen, Rundlauf, XSD-Prüfung von Original und geschriebener Datei, Fachregeln R1–R13, Angebot
gegen LV. Mit gesetztem `PROAVA_GAEB_TESTDATA` liest `scripts/check.sh` die Dateien zusätzlich ein und schreibt sie
zurück (Test `real_files_read_and_roundtrip`). Die Testdateien in
`crates/proava-gaeb/tests/data` sind synthetisch (dort `README.md`).

**Messprogramme:** `cargo run --release -p proava-ui --example perf` (synthetisches Projekt mit 2000 Positionen:
Dateien, GAEB, Preisspiegel, PDF, Bildzeit der Oberfläche, Speicher je Rückgängig-Schritt; Variablen
`PROAVA_PERF_*`), Vergleich zweier Stände mit `scripts/perf_ab.py vorher nachher`. Druckspeicher:
`cargo run --release -p proava-exchange --example druck_speicher -- titel aufforderung` (Varianten und Dokumente im
Kopf des Beispiels). Ziele: Bildzeit unter 16 ms bei 2000 Positionen, Laden/Speichern unter 1 s, Druck von 500
Seiten mit rund 2 GB; vor einem zu großen Druck schätzt `proava_print::estimate_lv_print` den Bedarf und lehnt mit
Meldung ab.

## Regeln

- Dateien aus echten Projekten sowie GAEB-/BVBS-Musterdateien nie ins Repo; Tests und Beispiele synthetisch.
- Nichts aus fremder Software kopieren oder zerlegen; fremder Code nur aus MIT-, BSD- oder Apache-2.0-Quellen mit
  Fundstelle.
- **Doku:** In `docs/` liegen nur `BUILD.md`, `MCP.md`, `FACHLICHES.md` und das Handbuch; keine Arbeitsnotizen,
  Änderungschroniken oder Zweignamen. `scripts/check.sh` prüft das (Schritt „Doku“).
- **Oberfläche** (Wächtertests in `crates/proava-ui/tests`):
  - Jeder Befehl auch in der Befehlssuche (Strg+K), Taste rechts daneben, „…“ nur, wenn ein Dialog folgt; überall
    derselbe Name. Jeder Befehl genau einmal in der Menüleiste (`workspace/menu_bar.rs`), nicht verfügbare Einträge
    ausgrauen.
  - Neue Tasten in `shortcuts.rs`; der Test dort verbietet Doppelbelegung, die Tabelle im Handbuch entsteht daraus.
  - Dialoge über `widgets::modal`: Knöpfe rechts „Abbrechen“, dann die Aktion (rot bei Löschen/Verwerfen), Enter
    bestätigt, Esc schließt, das erste Feld hat den Fokus. Jedes PDF über einen Druckdialog.
  - Texte deutsch und sachlich, Mehrzahl über `format::count`. Übliche einfarbige Bediensymbole mit Tooltip, keine
    Status-Bildchen in den Daten (`ui_symbols.rs`).
  - Nichts überdeckt sich oder ragt aus seinem Bereich: lange Texte mit „…“ kürzen und Tooltip, sonst umbrechen
    (`ui_start_list.rs`, `ui_header.rs`); Menüs passen bei 768 px Fensterhöhe (`ui_menu_scroll.rs`).

## Auslieferung (`scripts/release.sh`)

```bash
scripts/release.sh                # Linux + Windows
scripts/release.sh --nur-linux    # ohne Cross-Toolchain
scripts/release.sh --nur-windows  # nur Windows
OA_DIST=/tmp/paket scripts/release.sh --nur-windows   # anderer Zielordner
```

Legt nach `dist/<version>/` (nicht im Git); bricht ab, wenn `THIRD-PARTY-NOTICES` nicht zu `Cargo.lock` passt oder
das Handbuch veraltet ist.

| Pfad | Inhalt |
|---|---|
| `linux/` | `proava`, `proava-mcp`, `proava-web` (wenn gebaut), `LICENSE`, `THIRD-PARTY-NOTICES`, `LIESMICH.txt` (`assets/linux/`), `proava-handbuch.pdf`, `icons/`, `proava.desktop`, `proava-mime.xml`, `install_linux.sh` |
| `windows/` | `proava.exe`, `proava-mcp.exe`, `proava-web.exe`, `LICENSE.txt`, `THIRD-PARTY-NOTICES.txt`, `LIESMICH.txt` (`assets/windows/`, alle drei CRLF), `proava-handbuch.pdf`, `dictionaries/LIESMICH.txt` |
| `proava-<v>-x86_64.AppImage` | Linux zum Ausliefern: eine Datei (`scripts/appimage.sh` aus `linux/`) |
| `proava-<v>-windows-x86_64.zip` | Windows zum Ausliefern: der Ordner `windows/` als `proava-<v>/` |
| `SHA256SUMS` | Prüfsummen aller Dateien |

**Handbuch:** `docs/handbuch/handbuch.typ` → `proava-handbuch.pdf` mit `scripts/handbuch.sh` (erzeugt zuerst
`tastenkuerzel.csv` aus den Befehlsdefinitionen). Bilder in `docs/handbuch/bilder/` stammen aus dem synthetischen
Demo-Projekt (`cargo run -p proava --example demo_projekt -- <Datei.proava>`). Das PDF ist ins Programm eingebettet
(`crates/proava-ui/src/help.rs`); `handbook_table_is_current` schlägt fehl, wenn Tastenkürzel sich ändern und das
Handbuch nicht neu gebaut wurde.

**Programmsymbol:** Quelle `assets/icons/proava.svg`; `python3 scripts/make_icons.py` schreibt die PNGs und
`proava.ico` (liegen im Git, der Build braucht kein Python).

**Lizenz und Hinweise:** `LICENSE` (Business Source License 1.1) ist eingebettet („Über proAVA“), steht im Anhang
des Handbuchs und liegt in beiden Paketen. `python3 scripts/third_party_notices.py` schreibt `THIRD-PARTY-NOTICES`
aus `cargo metadata` beider Workspaces; nach jeder Änderung an `Cargo.lock` neu erzeugen (`--check` prüft).

**Linux einrichten:** `scripts/install_linux.sh` (im Paket `./install_linux.sh`) legt ohne sudo Programme nach
`~/.local/bin`, `.desktop`, Symbole und MIME-Typen (`application/x-proava`, `application/x-gaeb-xml`,
`application/x-gaeb90`) nach `~/.local/share`; `--gaeb-standard` macht proAVA zum Standard für GAEB-Dateien,
`--entfernen` nimmt alles wieder heraus. Ersetzt es ein laufendes proAVA, meldet das Skript die alten Prozesse; das
Programm selbst zeigt „proAVA wurde aktualisiert – bitte neu starten“ mit Knopf (`proava_application::program_update`),
der MCP-Server hängt einen Hinweis an seine nächste Antwort.

**AppImage:** Linux wird nur als AppImage ausgeliefert; `linux/` ist seine Zwischenstufe (dort richtet
`./install_linux.sh` auch ohne AppImage ein). `scripts/appimage.sh [LINUX-ORDNER [ZIEL]]` packt den fertigen
`linux/`-Ordner mit `assets/linux/AppRun` in ein AppImage (`release.sh` ruft es auf). Werkzeuge einmal herunterladen, nicht im Repo:
`appimagetool-x86_64.AppImage` (github.com/AppImage/appimagetool) und `runtime-x86_64`
(github.com/AppImage/type2-runtime) nach `~/.local/opt/appimage` (oder `APPIMAGE_TOOLS`). Läuft auf Systemen mit
derselben oder neuerer glibc als das Build-System; das Browserfenster nutzt WebKitGTK 4.1 vom System. `AppRun`:
ohne Argument proAVA, `mcp` der MCP-Server (auch über eine Verknüpfung namens `proava-mcp`), `--integrieren`
Startmenü und Dateitypen (`install_linux.sh --appimage DATEI` aus dem Abbild), `--entfernen`. proAVA erkennt das
AppImage an `APPIMAGE`/`APPDIR` (`program_update::running_appimage`): „Neues Fenster“ und „Neu starten“ starten die
AppImage-Datei, der Update-Hinweis beobachtet sie, und das Startfenster bietet „Ins Startmenü eintragen“, solange
`proava.desktop` nicht auf dieses AppImage zeigt. Lizenz der Laufzeit: `assets/linux/APPIMAGE-LAUFZEIT.txt`.

**Kommandozeile:** `proava [DATEI]` öffnet ein Projekt bzw. startet „Neues Projekt aus …“ mit einer GAEB- oder
CSV-Datei; `--schreibgeschuetzt` öffnet nur zum Lesen; `--version` zeigt Version, Build-Datum und Commit
(`crates/proava-ui/build.rs`, `SOURCE_DATE_EPOCH` wird beachtet).
