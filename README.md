# proAVA

AVA-Programm nach GAEB DA XML (Ausschreibung, Vergabe, Abrechnung) in Rust, für Linux und Windows
(macOS vorbereitet, nicht erprobt) – ausgerichtet auf die Planung der Gebäudeautomation.

> **proAVA comes with ABSOLUTELY NO WARRANTY, to the extent permitted by applicable law. Use at
> your own risk; check all results (LV, quantities, prices, totals, GAEB files) before you rely on
> them.**
>
> proAVA wird ohne jegliche Gewährleistung bereitgestellt, soweit gesetzlich zulässig. Die Nutzung
> erfolgt auf eigenes Risiko; alle Ergebnisse (LV, Mengen, Preise, Summen, GAEB-Dateien) vor
> Verwendung prüfen. Die Haftung für Vorsatz und grobe Fahrlässigkeit bleibt unberührt.

**Unter Windows 11 nur kurz erprobt (Start, Projekt laden); nicht umfassend getestet.**

## Funktionsumfang

- Leistungsverzeichnisse schreiben: Gliederung nach OZ-Maske, Positionen mit Kurz- und Langtext
  (Formatierung, Tabellen, Bilder), Positionsarten, Ausführungsbeschreibungen, Hinweise,
  Textergänzungen, Kostengruppen nach DIN 276, Positionsbibliothek, STLB-Bau-Texte.
- GAEB: DA XML 3.1–3.3 und GAEB 90 lesen und schreiben (X81–X86, D81–D86), Prüfung vor dem
  Export.
- Vergabe: Angebote der Bieter einlesen, Preisspiegel, Bieterverzeichnis, Auftrag, Schriftverkehr.
- Abrechnung und Nachträge, Kostenermittlung nach DIN 276.
- Druck als PDF (eingebettetes Typst), MCP-Server für KI-Assistenten (`apps/proava-mcp`).

Einzelheiten: Handbuch [`docs/handbuch/proava-handbuch.pdf`](docs/handbuch/proava-handbuch.pdf),
für Entwickler [`docs/BUILD.md`](docs/BUILD.md) (Bauen, Prüfen), [`docs/MCP.md`](docs/MCP.md) (MCP-Server) und
[`docs/FACHLICHES.md`](docs/FACHLICHES.md) (Normen, GAEB-Besonderheiten).

Einstellungen und Projektliste liegen in `~/.config/proava/settings.json`
(Windows: `%APPDATA%\proava\settings.json`, macOS: `~/Library/Application Support/proava`).

Ein Projekt ist ein Ordner mit der Projektdatei `<Name>.proava` (Zip mit `projekt.xml`,
Bildern und Schriftverkehr). Sicherungen sind Zip-Dateien mit `_Sicherung_JJJJ-MM-TT_HHMM` im
Namen und bleiben neben dem Projekt.

## Aufbau (Clean Architecture)

| Crate | Aufgabe |
|---|---|
| `crates/proava-domain` | Projekt, Leistungsverzeichnis, Positionen, OZ-Maske, Geldbeträge, Angebote, Abrechnung – ohne I/O |
| `crates/proava-application` | Anwendungsfälle und Ports |
| `crates/proava-infrastructure` | Projektdatei, Sicherung, Einstellungen (JSON), Ordner |
| `crates/proava-gaeb` | GAEB-Codecs DA XML und GAEB 90, Fachregeln |
| `crates/proava-exchange` | Adapter Domäne ↔ GAEB und Domäne → Druck |
| `crates/proava-print` | PDF über eingebettetes Typst |
| `crates/proava-spell` | Rechtschreibprüfung Deutsch |
| `crates/proava-ui` | Oberfläche (egui) |
| `crates/proava-bridge-protocol` | Absprache MCP-Server ↔ geöffnete proAVA-Fenster (lokale Brücke, JSON-Zeilen) |
| `crates/proava-web-protocol` | Absprache proAVA ↔ Browserfenster (JSON-Zeilen, Navigationsregel, Downloads) |
| `apps/proava` | Programm (Kompositionswurzel) |
| `apps/proava-mcp` | MCP-Server über stdio (`docs/MCP.md`) |
| `apps/proava-web` | Browserfenster für AUSSCHREIBEN.DE und STLB-Bau Online (wry, eigener Workspace) |

Regel: Abhängigkeiten zeigen nach innen. Die Domäne kennt niemanden, die UI kennt keinen
Speicher, die Infrastruktur ersetzt nur Ports.

## Bauen und starten

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo run --release          # Linux
scripts/check.sh             # Format, Clippy und Tests (begrenzt)
```

Windows: `scripts/build_windows.sh` baut die EXEs unter Linux (Cross-Build). Unter Windows 11
nur kurz erprobt (Start, Projekt laden); nicht umfassend getestet. macOS nur auf
einem Mac oder per CI. Einzelheiten zu allen drei Plattformen: [`docs/BUILD.md`](docs/BUILD.md).
Das Fenster zeichnet `eframe` überall mit OpenGL (`glow`).

## Lizenz

© 2026 Oliver Hagmann. proAVA ist **quelloffen und kostenlos** unter der **Business Source
License 1.1** (BSL 1.1), siehe [`LICENSE`](LICENSE). Die BSL ist keine von der OSI anerkannte
Open-Source-Lizenz; proAVA ist darum quelloffen, aber nicht „Open Source“. Lizenzen werden
nicht verkauft.

Kurz zusammengefasst – **verbindlich ist allein der englische Lizenztext**:

- **Erlaubt:** proAVA für eigene geschäftliche Zwecke nutzen, auch kommerziell – in
  Planungsbüros, bei Bauherren und öffentlichen Auftraggebern, bei Bietern und Firmen für ihre
  Angebote. Dienstleistungen rund um proAVA anbieten (Schulung, Einrichtung, Beratung,
  Anpassung). proAVA als Quelltext oder Programm weitergeben.
- **Nicht von der Erlaubnis umfasst:** proAVA oder ein davon abgeleitetes Werk als
  eigenständiges Produkt oder als Hauptbestandteil eines kostenpflichtigen Softwarepakets
  verkaufen; proAVA oder ein davon abgeleitetes Werk als gehosteten oder verwalteten Dienst
  (SaaS) anbieten.
- **Abwandlungen** bleiben unter dieser Lizenz (nach dem Change Date unter der Change License).
- **Später:** Ab dem 20.11.2030 (Change Date) gilt für diese Version die GNU Affero General
  Public License v3.0 oder später.

Lizenzen der verwendeten Bibliotheken und Schriften: `THIRD-PARTY-NOTICES`.

## Namen und Normen Dritter

proAVA ist eigenständig entwickelt; Grundlage sind die offen dokumentierten GAEB-Formate und
die einschlägigen Normen (VOB, DIN 276). STLB-Bau, GAEB, AUSSCHREIBEN.DE und DIN 276 sind
Namen bzw. Normen Dritter. proAVA ist mit diesen Stellen nicht verbunden und von ihnen nicht
autorisiert.
