# proAVA-MCP – Leistungsverzeichnisse für KIs

`proava-mcp` ist ein MCP-Server (Model Context Protocol) über stdio. Damit lesen und schreiben
KI-Assistenten in einem MCP-fähigen Programm (z. B. opencode) Leistungsverzeichnisse direkt in
`.proava`-Projektdateien – mit denselben Domänenregeln wie die Oberfläche (OZ-Maske,
Summen, Positionsarten, Preisspiegel).

- Binary: `apps/proava-mcp` (Kompositionswurzel wie `apps/proava`), bauen mit
  `cargo build --release -p proava-mcp` → `target/release/proava-mcp`.
- Protokoll: JSON-RPC 2.0, eine Nachricht je Zeile, `initialize` (Protokoll 2025-06-18,
  auch 2025-03-26 und 2024-11-05), `tools/list`, `tools/call`, `ping`. Kein Netzwerk-Port,
  keine Async-Laufzeit, nur `serde_json` (die Rust-SDK `rmcp` bräuchte tokio und mehr).
- Arbeitet dateibasiert: jedes Werkzeug bekommt `projekt` (Pfad der `.proava`), lädt sie,
  ändert über die Domäne und speichert sofort (atomar über `.tmp`).
- Oder **in den geöffneten proAVA-Fenstern** über die lokale Brücke (Abschnitt „Brücke zu den
  geöffneten Fenstern“): `fenster_*` – Änderungen laufen durch die normale Bearbeitung des
  Programms (Strg+Z, kein Konflikt mit ungespeicherten Änderungen), der Anwender sieht die Zeilen, an
  denen die KI arbeitet.

## Werkzeuge

| Werkzeug | Zweck | Art |
|---|---|---|
| `projekte_suchen` | `*.proava` in einem Ordner (rekursiv, ohne „Alte Versionen“), neueste zuerst | lesen |
| `fenster_lesen` | die geöffneten proAVA-Fenster (Brücke): ohne `fenster` die Liste mit PID, „links“/„rechts“ (Lage auf dem Bildschirm), „aktiv“ (zuletzt benutzt), Projekt, Datei, LV, Auswahl, ungespeichert, schreibgeschützt, offener Dialog, KI erlaubt; mit `fenster` die Einzelheiten (LVs, Auswahl – die erste gewählte Zeile ist die Schreibmarke); mit `zeilen`/`ab`/`anzahl`/`oz`/`ids`/`auswahl` (+ `langtext`, `lv`) die Zeilen im Fenster (Stand im Speicher, auch ungespeichert) | lesen |
| `fenster_zeigen` | Zeilen im Fenster wählen, hinscrollen, Titel aufklappen, optional Positionsfenster (`positionsfenster`), mit KI-Markierung und Text in der Statusleiste (`markierung`); `oz`, `ids`, `auswahl`, `weitere` (n folgende Positionen/Hinweise/AB), `lv`; `nur_markieren` (+ `sekunden`) lässt die Auswahl, `aufheben` nimmt die Markierung weg. Ändert nichts am LV | lesen (Ansicht) |
| `fenster_suchen` | Suche wie im Fenster „Übernehmen aus Projekt“ (`proava_domain::lv_search`) im LV eines Fensters (`suche`, `kg`, `einheit`, `lv`, `max_treffer`); Treffer im Fenster gewählt und markiert, Antwort mit OZ, Kurztext, Menge, EP, KG, Titelpfad, `#Id`, Fundstelle | lesen (Ansicht) |
| `fenster_einfuegen` | Zeilen in ein Fenster einfügen wie Strg+C/Strg+V zwischen zwei Fenstern (Langtext, Bilder, Bieterangaben, AB, KG, STLB; neue OZ/Ids, Status offen, ein Strg+Z): Ziel `fenster` (Standard aktiv), an der Schreibmarke oder `nach_oz`/`nach_id`, `lv`; Quelle `quelle_fenster` (ohne `oz`/`ids` die dort gewählten Zeilen, `weitere`, `quelle_lv`) oder `quelle` (Projekt, Bibliothek, GAEB – `oz`/`ids`); `mengen` (Standard ja), `preise` (Standard nein). Schreibgeschützt/Dialog offen: abgelehnt; versandtes LV: Rückfrage im Fenster, Antwort „wartet auf den Anwender“ bzw. nach seiner Antwort. Gespeichert wird erst, wenn der Anwender speichert | schreiben (Freigabe) |
| `fenster_speichern` | Projekt im Fenster speichern wie Strg+S – `projekt` (das Fenster, das es bearbeitet) oder `fenster`; beim ersten Speichern je Serverlauf vorher eine Sicherung neben der Datei (wie das erste Schreiben auf die Datei); schreibgeschützt oder Dialog offen: abgelehnt | schreiben (Freigabe) |
| `fenster_projekt_oeffnen` | Projekt öffnen: im Fenster mit der Startseite, sonst in einem neuen Fenster (`jobs::new_window`, nur aus dem echten Programm), `schreibgeschuetzt`, `fenster` (wer öffnet); schon offen → Hinweis; wartet bis zu 20 s, bis sich das Fenster meldet, und nennt seine PID | schreiben (Freigabe) |
| `lv_suchen` | Positionen in anderen LVs finden (erster Schritt von „Hole die Positionen für … aus dem LV von … und füge sie hier ein“): Volltext über Kurztext, Langtext, Bieterangaben (Vortext/Eintrag/Nachtext), Unterbeschreibungen, Hinweise, Ausführungsbeschreibungen und Titel; alle Wörter müssen vorkommen, Groß/klein und Umlaute egal (ä = ae, ß = ss); Filter `kg` (auch geerbt, Anfang genügt) und `einheit` (nur Positionen). Ziel: genau eines von `projekt` (+ optional `lv`, sonst alle LVs), `ordner` (alle `.proava` darunter bis 8 Ebenen, ohne „Alte Versionen“/„00- Alte Versionen“ und versteckte Ordner, `mit_alten_versionen`; liest je Projekt nur `projekt.xml`, keine Bilder) oder `datei` (GAEB X81–X86/D81–D86, auch `.zip`, oder CSV). Antwort je Datei und LV gruppiert: OZ, Art, Kurztext, Menge Einheit, EP, KG, Titelpfad, `#Id`, Textausschnitt um den Fund, `quelle_lv`; `max_treffer` (Standard 50, gezählt werden alle), `zeitlimit_s` (Standard 20, höchstens 120 – danach Hinweis, wie viele Projekte fehlen); nicht lesbare Dateien übersprungen und genannt; `PROAVA_MCP_ORDNER` gilt auch für jeden Unterordner | lesen |
| `projekt_lesen` | Projektstammdaten (Adressen, Vergabe, Fristen, Sicherheiten, Skonto, Abzüge, Zahlungsbedingungen, Hinweise wie Bindefrist vor Abgabe), LVs des Projekts, LV-Kopf, OZ-Maske, Nummerierung, Anzahl, Summen, oberste Titel | lesen |
| `lvs_lesen` | die LVs des Projekts: Position, Nr., Bezeichnung, Art (Ausschreibung oder Kosten-LV mit Stufe), Maske, Positionen, netto, Angebote, aktiv in proAVA | lesen |
| `lv_lesen` | Gliederung `OZ \| Art \| Kurztext \| Menge Einheit \| EP \| GP \| Id`, `ab_oz`/`bis_oz`, `nur_titel`, `mit_langtext` | lesen |
| `position_lesen` | eine Zeile vollständig inkl. Langtext (Auszeichnung), Textquelle (AUSSCHREIBEN.DE, STLB-Bau …, mit Vermerk „in freie Position umgewandelt am …“), Kennungszeile „Kennung: STLB-Bau 10/2026 · LB 070 · Leistung … · KG … · n Merkmale“ mit Schreibschutz bzw. „STLB-Bau*“ und Bieterpreisen | lesen |
| `lv_pruefen` | Exportprüfung: Fehler (doppelte/fehlende OZ, leere Kurztexte, ohne Menge/Einheit, STLB offen) und Hinweise (u. a. „Langtext weicht von STLB-Bau ab (STLB-Bau*)“ – geht ohne Kennung in GAEB, umwandeln oder aus STLB-Bau aktualisieren; TA leer, Zahlenwert fehlt, Hinweis ohne Text); dazu die GAEB-Fachregeln R1–R13 auf die Datei, wie proAVA sie schreibt (`art` X83/X82/X81, Standard X83); `alle: true` prüft alle LVs der Ausschreibung (Kosten-LVs nur, wenn `lv` ein Kosten-LV ist), vorneweg eine Übersicht „LV: n Fehler, m Hinweise“, dann je LV „== LV … ==“ mit seiner Prüfung | lesen |
| `rechtschreibung_pruefen` | Rechtschreibung (Deutsch): unbekannte Wörter mit Anzahl und Fundstellen (`OZ Kurztext`/`OZ Langtext`, Hinweise, Ausführungsbeschreibungen), häufigste zuerst, `vorschlaege` (Standard 20, 0 = keine) Wörter mit Vorschlägen; Wörterbuch und eigene Wörter wie in proAVA, GA-Fachwörter, Wörter nur aus Großbuchstaben oder mit Ziffern bleiben ungeprüft; korrigieren mit `zeile_aendern` | lesen |
| `angebote_lesen` | Angebote mit Summen, Nachlass, fehlenden EP, Rechenfehlern und Kurzfassung der Strukturabweichungen; mit `angebot` das ganze Abweichungsprotokoll und die Zuordnungsvorschläge (sicher/prüfen, Ähnlichkeit) | lesen |
| `preisspiegel_lesen` | Rangfolge, Abstand zum Mindestbieter, Ausreißer, optional je Titel / je Position | lesen |
| `kostengruppen_lesen` | Kosten nach Kostengruppen (DIN 276): je KG Positionen, Planersumme und je gewertetem Angebot (netto vor Nachlass), Summe je Zehnergruppe (z. B. 480), Positionen ohne KG; Positionen ohne eigene KG zählen zu der ihres Titels | lesen |
| `projekt_anlegen` | neues Projekt `<ordner>/<name>/<name>.proava` (Maske, Schrittweite, LV-Titel; ohne Angabe wie in den proAVA-Einstellungen, Vorgabe `11.222.PPPPI`); `vorlage`: „leer“ (Standard), `kostenschaetzung`/`kostenberechnung` (GA-Vorlage mit einer Kostenermittlung, siehe `kosten_lesen`), „GA“ = mitgelieferte Vorlage „GA nach DIN 276 (481–489)“ (Titel 01.481–01.485, 01.489 mit KG, Vorbemerkung, Ausführungsbeschreibung, übliche Abzüge; bringt ihre OZ-Maske `11.222.PPPPI` mit, `oz_maske` ersetzt sie – die erste Ebene mit 3 Ziffern trägt die KG als Nummer; Schrittweite aus dem Aufruf oder den proAVA-Einstellungen), Name einer eigenen Vorlage aus dem Vorlagenordner von proAVA oder absoluter Pfad einer Vorlagendatei; `positionen` (Standard ja) nimmt die Positionen der Vorlage mit. Eine eigene Vorlage bringt Maske und Gliederung mit (`oz_maske`/`schrittweite` dann abgelehnt); ein unbekannter Name nennt die vorhandenen Vorlagen | schreiben |
| `projektangaben_setzen` | Stammdaten: Bezeichnung, Kurzname, Baumaßnahme, `bauherr`, `auftraggeber` (eigener Auftraggeber/Vergabestelle, geht als OWN in den GAEB-Export), `auftraggeber_wie_bauherr` (true = kein eigener, Vorgabe), Aufsteller, Baustelle, `adressen` (Rolle + Adresse), Vergabeart/-nummer, Abgabetermin (+ `abgabe_uhrzeit`)/-ort, Bindefrist, Ausführungsbeginn/-ende, weitere Fristen, Sicherheiten %, Skonto % + Tage, `abzuege` [{bezeichnung, prozent}], Zahlungsbedingungen, weitere Vereinbarungen – geprüft (TT.MM.JJJJ, Ende ≥ Beginn, 0–100 %); LV-Kopf des gewählten LV: `lv_nummer`, LV-Titel, MwSt, Maske, Schrittweite, `ebenen` (Bezeichnungen der Titelebenen, GAEB LblBoQBkdn; nur genau „Los“ wird ein GAEB-Los), `ep_anteile` (Bezeichnungen der EP-Anteile, Lohn/Material/Gerät/Sonstiges mit GAEB-Art); eigene Kopf-/Fußzeile des Projekts (siehe unten) bzw. `kopf_fusszeile_standard: true` = wieder die aus den Einstellungen | schreiben |
| `lv_anlegen` | weiteres LV (Los/Gewerk): `bezeichnung`, `nummer` (sonst nächste freie, doppelte abgelehnt), Maske/Schrittweite/MwSt/Währung wie das aktive LV; `art` `kostenschaetzung`/`kostenberechnung` = Kosten-LV (Kostenermittlung nach DIN 276 als Kurztext-LV, Maske `111.PPPP` bzw. `111.222.PPPP`, Titel je KG aus `kostengruppen` – Standard alle KG 410–490 –, Vorbemerkung für die Angaben nach DIN 276); `aus` (mit `kostenberechnung`) kopiert eine Kostenschätzung als Kostenberechnung (dann ohne `bezeichnung`) | schreiben |
| `titel_einfuegen` | Titel/Untertitel (`unter_titel`, `nach_oz`, `ebene`; `oz` von Hand, z. B. „481“) | schreiben |
| `position_einfuegen` | eine Position (Kurztext, Langtext, Menge, Einheit, EP, Art, Pauschal …; `oz` von Hand) | schreiben |
| `positionen_einfuegen` | viele Zeilen auf einmal (Positionen, Titel, Hinweise gemischt, je Zeile `oz` möglich), alles oder nichts | schreiben |
| `hinweis_einfuegen` | Hinweistext ohne OZ: `text` und/oder `kurztext` (Überschrift, GAEB-Kurztext); ohne beides meldet die Prüfung ihn – für eine Leerzeile in der Bearbeitung `abstand_einfuegen` | schreiben |
| `abstand_einfuegen` | Abstandshalter: Leerzeile nur für die Bearbeitung (keine OZ, kein Text, kein Preis, zählt nirgends), Ort wie bei Positionen; erscheint weder im Druck noch in GAEB oder CSV; `lv_lesen` zeigt „Abstand“ | schreiben |
| `bild_einfuegen` | Bild aus einer Datei (PNG/JPEG, `datei`, `breite_mm`, `absatz` = nach diesem Block, ohne = am Ende) in den Langtext einer Position, eines Hinweises oder einer Ausführungsbeschreibung; steht dann als `[Bild: name, breite mm]` im Text; nicht in den Langtext von STLB-Bau-Positionen (schreibgeschützt, vorher `zeile_aendern` mit `in_freie_position`) | schreiben |
| `bieterangabe_einfuegen` | Bieterangabe (GAEB Textergänzung vom Bieter, z. B. „Fabrikat: ………… (vom Bieter)“) in den Langtext einer Position oder eines Hinweises: `vortext` (Pflicht), `art` „Bieter“ (Standard) oder „AG“ mit `inhalt` (Angabe des Auftraggebers), `nachtext`, `kennung` (sonst die nächste freie); Stelle: `nach` = direkt hinter diesem Text (erstes Vorkommen, ein Leerzeichen davor kommt dazu), `absatz` = eigene Zeile nach diesem Block (wie bei `bild_einfuegen`), ohne beides unter dem Langtext. Vortext bekommt ein Leerzeichen am Ende wie bei STLB-Bau. `position_lesen` zeigt die Stelle als `{TE 01}`, `lv_pruefen` zählt die Bieterangaben und nennt solche ohne Vortext; nicht in den Langtext von STLB-Bau-Positionen (schreibgeschützt, vorher `zeile_aendern` mit `in_freie_position`) | schreiben |
| `pdf_ausschnitt_einfuegen` | Ausschnitt einer PDF-Seite (z. B. Systemtopologie ohne Rahmen und Plankopf) in einen Langtext: `datei` (PDF, ≤ 100 MB), `seite` (ab 1), Rechteck `ausschnitt_mm` {links, oben, breite, hoehe} von der linken oberen Ecke oder `ausschnitt_anteile` {links, oben, rechts, unten} (0…1), ohne beides die ganze Seite; `breite_mm` im Text (10–180, Standard 150 bzw. schmaler), `absatz` wie bei `bild_einfuegen`. Ins Projekt kommt nur die gewählte Seite (wenn kleiner als die Datei), dieselbe PDF bei mehreren Ausschnitten nur einmal. Im proAVA-PDF Vektorgrafik, in GAEB DA XML ein PNG mit 300 dpi. Im Text steht er wie ein Bild als `[Bild: name.pdf, breite mm]`; nicht in den Langtext von STLB-Bau-Positionen (schreibgeschützt, vorher `zeile_aendern` mit `in_freie_position`) | schreiben |
| `ausfuehrungsbeschreibung_einfuegen` | Ausführungsbeschreibung (GAEB PerfDescr): `nummer`, `bezeichnung`, `kurztext`, `langtext`, Ort wie bei Positionen; Kennung `A0001` (Antworten und Listen; in GAEB `PerfNo` 1), ohne `nummer` zählt proAVA nach Lage; Positionen beziehen sich darauf mit `ausfuehrungsbeschreibung: "A0001"` („0001“, „1“ und „AB 0001“ gehen auch; `position_einfuegen`, `positionen_einfuegen`, `zeile_aendern`, „“ löst) | schreiben |
| `zeile_aendern` | nur die angegebenen Felder; Titel-Ebene; OZ von Hand (`neue_oz`); `status` offen / in Bearbeitung / fertig (Titel: alle Positionen darunter) – fertige Positionen sind gegen Ändern, Löschen, Verschieben und `neue_oz` gesperrt, „in Bearbeitung“ im selben Aufruf entsperrt; `kg` = Kostengruppe DIN 276 (drei Ziffern, „“ = keine eigene) an Positionen und Titeln – erben fertige Positionen die neue KG eines Titels, nur mit `fertige_erben: true` (sonst nennt die Antwort sie und ändert nichts); `lb` = Leistungsbereich (Gewerk), STLB-Bau-Nummer wie „070“ oder eigener Name („“ = keiner), an Positionen und Titeln (Vorgabe für alles darunter, bei erbenden fertigen Positionen wie bei `kg` nur mit `fertige_erben: true`), in GAEB Katalog „work category“; `mengenermittlung` = Zeilen der Mengenermittlung `[{ort, rechenansatz}]` (Planungsmenge, dieselben Zeilen wie das Aufmaß der Abrechnung), ohne `menge` wird die Summe die Menge – auch bei `position_einfuegen`/`positionen_einfuegen`; `position_lesen` zeigt die Zeilen; **STLB-Bau-Positionen** (Kennung, kein Platzhalter): `langtext` und `einheit` (eine andere als die vorhandene) werden abgelehnt, „Nichts gespeichert“ mit dem Weg (STLB-Bau über `stlb_pruefdatei`/`stlb_aktualisieren` oder `in_freie_position`); frei bleiben Kurztext, Menge, EP, KG, LB, Status, Art, Bezug und die Werte der `textergaenzungen` (je `kennung` nur `inhalt`; Art, Vortext, Nachtext und neue Angaben abgelehnt); `in_freie_position: true` entfernt die Kennung (Textquelle vermerkt „in freie Position umgewandelt am …“), danach sind Langtext und Einheit frei – auch im selben Aufruf | schreiben |
| `zeile_loeschen` | Zeile; Titel mit Inhalt nur mit `mit_inhalt: true` | schreiben |
| `zeile_umwandeln` | Zeilenart ändern: `zeilenart` position / hinweis / ausfuehrungsbeschreibung (Zeile per `oz` oder `id`); Kurztext, Langtext mit Bildern, Bieterangaben, STLB-Bau-Kennung und Textquelle bleiben; fällt etwas weg (OZ, Menge, EP, KG, Nummer der Ausführungsbeschreibung, Bezüge, Bieterpreise, versandte X83), ohne `bestaetigen: true` nur Vorschau; neue Position mit freier OZ, Menge 0; fertige Positionen gesperrt | schreiben |
| `zeile_verschieben` | hoch/runter, Titel samt Inhalt | schreiben |
| `neu_nummerieren` | alle OZ nach Maske und Schrittweite | schreiben |
| `angebot_nachlass_setzen` | Nachlass in % oder €, Angebot ausschließen | schreiben |
| `gaeb_exportieren` | X83/X82/X81 als GAEB DA XML (`.X8x`) oder GAEB 90 (`format: gaeb90`, `.D8x`); `bilder` (Standard ja) gibt die Bilder der Langtexte in DA XML mit, GAEB 90 kann keine (Warnung) | schreiben |
| `stlb_uebernehmen` | Positionen aus einer GAEB-Datei von STLB-Bau oder AUSSCHREIBEN.DE (auch `.zip`; `quelle` z. B. „AUSSCHREIBEN.DE“ für die Textquelle, sonst der Programmname der Datei): Platzhalter füllen (`platzhalter_oz`/`platzhalter_id`, seine Menge bleibt; fertiger Platzhalter wird abgelehnt) oder einfügen (Ort wie `positionen_einfuegen`), Auswahl per `positionen` (OZ der Datei); STLB-Bau-Kennung und Textergänzungen (mit Zahlenwerten) bleiben, Status offen, KG aus der Kennung, wenn der Titel keine gleiche gibt; `als` = Zeilenart für alle oder je OZ (`{"0001": "hinweis"}`), ohne Angabe der Vorschlag der Datei (Standardbeschreibung ohne Einheit und Menge → Hinweis, Hinweistext/Ausführungsbeschreibung mit Kennung bleiben das; ein Platzhalter wird zur gewählten Zeilenart) | schreiben |
| `ausschreiben_suchen` | Herstellertexte auf AUSSCHREIBEN.DE finden – nur in den für KI-Assistenten veröffentlichten Katalogverzeichnissen (`llms.txt`), vorher `robots.txt` (beachtet), Zwischenspeicher 7 Tage: `suche` muss Hersteller/Katalog nennen (Wortanfang genügt), übrige Wörter filtern Titel, Artikelnummer, Ordner; `max_treffer` (Standard 50, höchstens 300). Je Treffer Titel, Art.-Nr., Katalog › Ordner, Link; kein Langtext. `openWorldHint` | lesen |
| `ausschreiben_oeffnen` | öffnet `url` (Seite von ausschreiben.de, z. B. Treffer, sonst Startseite) im Browserfenster von proAVA **für den Anwender**; er exportiert dort selbst, die KI bedient das Fenster nicht. Exporte in seinem Download-Ordner (Antwort nennt ihn und die bisher gespeicherten Dateien), danach `stlb_uebernehmen` mit `quelle` „AUSSCHREIBEN.DE“. Ohne installiertes `proava-web` oder mit `PROAVA_KEINE_PROGRAMME` nur der Link | schreiben (Freigabe) |
| `stlb_pruefdatei` | die Positionen mit STLB-Bau-Kennung (mit Titeln, OZ wie im LV) als X81 – zum Prüfen/Aktualisieren in STLB-Bau; `ziel`, `ueberschreiben` | schreiben |
| `stlb_aktualisieren` | Datei von STLB-Bau nach Prüfen/Aktualisieren: Zuordnung über OZ, sonst LB + Gruppe; Unterschiede wie `lv_vergleichen` und Kennung alt → neu; Vorschau, mit `uebernehmen: true` Kurz-/Langtext, Einheit, Textergänzungen, Kennung übernehmen (OZ, Menge, Preise bleiben, Status offen, fertige bleiben) | schreiben |
| `positionen_uebernehmen` | Positionen wiederverwenden: `quelle` (anderes Projekt, Bibliothek oder GAEB-LV, auch `.zip`, nur gelesen, `quelle_lv`), `oz` (Liste; Titel samt Inhalt) und/oder `ids` (Zeilen-Ids der Quelle aus `lv_suchen`/`lv_lesen`, Zahl oder „#12“ – auch Hinweise ohne OZ), Ort wie `positionen_einfuegen`, `mengen` (Standard ja), `preise` (Standard nein) – Ausführungsbeschreibungen mit Bezügen, Bilder, Textergänzungen und STLB-Kennungen kommen mit, Status offen, neue Ids und freie OZ | schreiben |
| `lv_vergleichen` | Änderungsliste gegen einen anderen Stand: `vergleich` (Projekt oder GAEB-Datei) und/oder `vergleich_lv`; neu/entfallen/verschoben/geändert mit Kurztext, Langtext-Wortunterschied, Menge, Einheit, Art, EP, Textergänzungen; `nur` filtert. Zuordnung über die OZ, bei geänderter OZ über Textähnlichkeit (≥ 80 % gemeinsame Wörter) | lesen |
| `bibliothek_lesen` | eigene Positionsbibliothek (ein proAVA-Projekt, Standard `~/proAVA-Bibliothek.proava`) mit `suche` | lesen |
| `bibliothek_suchen` | alle Bibliotheken (eigene und weitere aus den Einstellungen) durchsuchen: Volltext über Kurz-/Langtext/Stichwörter (Umlaute egal), Filter `kg` (auch geerbt, Anfang genügt), `art`, `stichwort`, `bibliothek`; je Treffer Erfahrungspreis mit Datum, Stand, Stichwörter, `#Id`; `mit_langtext` | lesen |
| `bibliothek_einfuegen` | Einträge (`oz` oder `ids`) einer Bibliothek ins LV wie aus dem Seitenfenster: Ort wie `positionen_einfuegen`, `mengen` (Standard nein), `preise` (Erfahrungspreise, Standard ja); jede Position merkt sich ihren Eintrag | schreiben |
| `bibliothek_ablegen` | Zeilen des LV (`oz`) in die eigene Bibliothek unter `titel` (OZ dort) oder `neuer_titel`; Stand heute, `stichwoerter`, `preis_datum`; legt die Bibliothek beim ersten Mal an, schreibgeschützte abgelehnt | schreiben |
| `bibliothek_abgleichen` | Positionen aus einer Bibliothek gegen ihren Eintrag (Wortunterschied, Einheit, Art, Textergänzungen, mit `preise` auch EP); `uebernehmen: ["alle"]` oder OZ-Liste bringt sie auf den Bibliotheksstand (Menge, OZ, Status bleiben) | lesen/schreiben |
| `angebot_einlesen` | Bieterangebot `.X84`/`.D84` ins Projekt; `ins_bieterverzeichnis` (Standard ja): Firma ins Bieterverzeichnis aufnehmen bzw. vorhandenen Eintrag (gleicher Name/Anschrift) zuordnen; die LV-Mengen beim Einlesen sind die Angebotsmengen; jedes Angebot wird mit dem LV verglichen (Abweichungsprotokoll am Angebot: nicht im LV, fehlend, OZ passt nicht zum Inhalt, Menge, Einheit, Kurz-/Langtext, Positionsart, Titel, Dateisumme mit Aufschlüsselung), gewertet wird mit Menge und Text des LV; `zuordnung` `oz` (Standard) oder `text` (sichere Vorschläge des Strukturvergleichs übernehmen), `zuordnungen` {"Bieter-OZ": "LV-OZ"} von Hand | schreiben |
| `lv_aus_gaeb` | neues Projekt aus GAEB (OZ bleiben erhalten) | schreiben |
| `gaeb_importieren` | wie „GAEB ▸ Importieren …“: X81–X86 (DA XML 3.0–3.3), D81–D86, auch in `.zip` – auch Zulieferungen (Vortexte, Herstellertexte ohne Mengen/Preise) und eigene Stände unter anderem Namen. `ziel` `lv` (Standard: neues LV in `projekt`, `nummer`/`bezeichnung` mit Vorschlag aus der Datei, doppelte Nummer abgelehnt, ein leeres Einzel-LV wird ersetzt) oder `projekt` (`ordner`, `name`); `preise` (Standard: ja, wenn die Datei Preise hat, z. B. X82); `vorschau: true` liest nur. OZ wie in der Datei, Bilder und Formatierung bleiben, alle Positionen „offen“. Antwort = Importprotokoll, eine Angabe je Zeile „Bezeichnung: Wert“ (Datei, Art, Format, LV, Titel, Positionen …, Übernommen als, Speicherort, Preise, Verworfen oder umgewandelt mit Einzelheiten „  – …“ darunter) | schreiben |
| `csv_importieren` | wie „GAEB ▸ CSV importieren …“: CSV-Tabelle im proAVA-Format (`OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:`, Kopfzeile optional, Spalten nach Namen, Trenner `;` `,` Tab, UTF-8/Windows-1252, deutsche Zahlen; Folgezeilen ohne OZ = Langtext, Leerzeilen optional; fehlende Titel aus den OZ angelegt, OZ-Maske aus den OZ). GP ≠ Menge × EP → errechneter GP gilt, Position markiert („GP prüfen“), Protokoll mit Abweichung je Position und Summe. `ziel` `lv`/`projekt` wie `gaeb_importieren`, `waehrung` EUR (Standard bzw. aus der Datei)/USD/CHF, `vorschau: true` liest nur | schreiben |
| `csv_exportieren` | das LV als CSV im selben Format (Kopfzeile, je Eintrag eine Zeile mit OZ, Langtext je Absatz darunter, Leerzeile dazwischen, Titel/AB/Hinweise mit Positionsart, KG wie sie gilt); `zeichensatz` `utf-8` (Standard, mit BOM) oder `windows-1252`; Formatierung, Bilder, Unterbeschreibungen fallen weg (Antwort nennt es); Ziel ohne Angabe `<Projekt>_LV_<Nummer>.csv` neben dem Projekt | schreiben |
| `pdf_drucken` | wie der Druckdialog: `angebotsaufforderung` oder `lv_ohne_preise` (LV ohne Preise für die Bieter), `bepreistes_lv`, `lv_mit_preisen` oder `kostenvoranschlag` (LV mit Preisen = Kostenvoranschlag nach DIN 276; `kostenanschlag` gilt weiter), `kurztext_lv`, `zusammenstellung`, `auftrags_lv`, `preisspiegel`, `bieterpreise` (`angebot`), `kostengruppen`, `niederschrift`, `aufmasspruefung` und `rechnungsfreigabe` (`rechnung`, sonst die letzte), `kostenermittlung` (Datenpunktrechnung; `kostenermittlung` = Nummer/Bezeichnung aus `kosten_lesen`, sonst die neueste; `diagramm`, `anlagenliste`, `kostenkontrolle`, Datei `<Projekt>_Kostenberechnung.pdf`), `kosten_lv` (Kosten-LV aus `lv`: Angaben, Zusammenstellung nach KG, Kurztext-Liste; `zusammenstellung: false` ohne Kurztext-Liste); `kostenverfolgung` (A4 quer, Ansicht wie `kostenverfolgung_lesen`: `ebene`, `ansicht`, `brutto`, `abweichung`, `vergleich`; `deckblatt`, `diagramm`, `stil`); Vergabe- und Abrechnungsdokumente gibt es für Kosten-LVs nicht; Optionen `langtexte`, `bilder`, `hinweise`, `ausfuehrungsbeschreibungen`, `zusammenstellung`, `titel_neue_seite`, `quer`, `deckblatt` (`standard`, `briefkopf`, `tabelle1`, `tabelle2`, `kein` oder true/false), `anteil` (Zusammenstellung mit Spalte „Anteil“ in % der Summe netto, Standard an), `inhaltsverzeichnis`, `uebertrag`, `preise` (nur `kurztext_lv`: false = EP/GP zum Ausfüllen), `stil` (`klassisch` oder `proava`; Vorgabe und Akzentfarbe aus den Einstellungen), `blattrahmen`, `gitter` (Tabellen voll umrandet; ohne Angabe die Einstellung, Preisspiegel an), `kostengruppe` (Standard an), `leistungsbereich` (Standard aus), `schrift_pt`, `titel` (OZ mit Komma), `kopftext`, `fusstext` (Zusatzzeilen `{kopftext}`/`{fusstext}`); Kopf-/Fußzeile-Schlüssel (siehe unten) nur für diesen Ausdruck; Antwort nennt die Seitenzahl; ein LV-Druck, der voraussichtlich mehr Arbeitsspeicher braucht als frei ist, wird mit Meldung abgelehnt (Seiten, Bedarf, frei, Bedarf ohne Übertrag, Hinweis auf `titel`) | schreiben |
| `kosten_lesen` | Kostenermittlungen des Projekts (ganzes Projekt): zuerst die Kosten-LVs mit Summe, ihre Datenpunkt-Abschnitte (GU-Fall) und die Summen je Stufe und KG (2. Ebene) über Kosten-LVs und Datenpunktrechnungen; mit `lv` (Kosten-LV, `abschnitt` = OZ bei mehreren) der Datenpunkt-Abschnitt dieses LV im Einzelnen (ohne Kostenkontrolle); dann die Datenpunktrechnungen: Liste mit Stufe, Bezeichnung, Stand, Bauteilen, DP, Summe und die neueste bzw. `kostenermittlung` im Einzelnen – DP-Arten mit Preis/Anzahl/Betrag, je Bauteil DP und Summe, Aufteilung 481–489 mit Prüfsumme, Zusammenfassung je KG, Kostenkontrolle gegen die LVs, Hinweise; `zeilen: true` mit Anlagenlisten | lesen |
| `kosten_anlegen` | neue Kostenermittlung nach Vorlage: `stufe` `kostenschaetzung` (2. Ebene, Kennwert 450 €/DP) oder `kostenberechnung` (3. Ebene, EP 400/200 €, Aufteilung 40/12/10/30/5/3 %); `aus` = Kostenberechnung aus einer Kostenschätzung (Anlagenliste übernommen); `titel`, `stand`, `bearbeiter`, `bemerkung`, `bauteil`, `kennwert`, `kennwert_von`/`_bis`, `kennwert_je_dp_art` | schreiben |
| `kosten_aendern` | Kopf, Kennwert, `dp_arten` [{kuerzel, bezeichnung, preis, entfernen}] (AV/BV/MV virtuell, alle übrigen physikalisch – aus dem Kürzel), `aufteilung` {"481": 40, …} als Vorgabe oder mit `bauteil` eigene Aufteilung, `vorgabe_aufteilung_fuer`, `bauteile_umbenennen`, `loeschen`; mit `lv` der Datenpunkt-Abschnitt dieses Kosten-LV (ohne `titel`/`stand`/`bearbeiter` – die des LV; `loeschen` nimmt die Datenpunkte, der Titel bleibt) | schreiben |
| `kosten_zeilen_schreiben` | Anlagenliste: `zeilen` [{bauteil, ort, gewerk, anlage, bemerkung, dp: {"AI": 12, …}}] anhängen oder mit `ersetzen` die Bauteile neu füllen (neue Bauteile/DP-Arten werden angelegt, Zeilen ohne `bauteil` ins erste Bauteil); mit `lv` statt `kostenermittlung` in den Datenpunkt-Abschnitt dieses Kosten-LV (GU-Fall: KG 410–470 als Kurztext-Positionen, 480 nach Datenpunkten in einer Kostenermittlung) – ohne Abschnitt wird er angelegt (leerer Titel 480 der Vorlage oder neuer Titel 480); Antwort mit Protokoll und Summen | schreiben |
| `kostenverfolgung_lesen` | Kostenverfolgung des ganzen Projekts (kein `lv`): Kennzahlen (Kostenvorgabe mit Art/Stand/Grundlage, Prognose zur Kostenvorgabe, Auftrag, Abrechnung mit Freigaben, Schlussrechnungen), Tabelle je KG (`ebene` 1–3, Standard 2) oder je Vergabeeinheit (`ansicht: vergabeeinheiten`) mit Kostenvorgabe, Kostenschätzung, Kostenberechnung, Kostenvoranschlag, Angebot, Auftrag, Prognose, Abrechnung, Kostenfeststellung – nur Stufen mit Werten –, Abweichung in Klammern (`abweichung` budget bzw. kostenvorgabe/vorstufe/stand/keine), Markierung „[über Kostenvorgabe]“/„[über Vorstufe]“, Erläuterung je KG; `brutto`; `vergleich` = Nummer oder Datum eines Kostenstands (jede Stufe gegen dieselbe Stufe des Stands, Abweichung `stand`); dazu Vergabeeinheiten (Angebot, Prognose aus, Freigaben), Kostenermittlungen mit Schlüssel und „zählt“, Grundlagen, Annahmen der Prognose, Hinweise, Positionen ohne KG, Kostenstände | lesen |
| `kostenvorgabe_setzen` | Kostenvorgabe (Budget des Bauherrn, DIN 276, 4.6): `budget` {"420": 185000, "481": "62.500", "430": null} je KG netto (null entfernt), `ersetzen` leert vorher, `aus` kostenschaetzung/kostenberechnung (alle zählenden) oder Schlüssel einer Kostenermittlung ersetzt das Budget durch deren Summen je KG; `art` obergrenze/zielgroesse, `stand`, `grundlage`, `schwelle` (% über der Vorstufe, Standard 10); Antwort mit Budget gesamt und Hinweisen (Gruppenbudget kleiner als seine Untergruppen) | schreiben |
| `kostenverfolgung_aendern` | `erlaeuterungen` {"420": "…"} je KG („“/null entfernt), `kostenermittlungen` {Schlüssel: true/false/null} (null = Vorgabe), `angebot_wahl` {angebot: Id oder null = Mindestbieter, begruendung} für das LV aus `lv`, solange es nicht beauftragt ist, `kostenstand_festhalten` {datum, anlass}, `kostenstand_loeschen` (Nummer); Antwort mit neuer Prognose | schreiben |
| `bieter_lesen` | Bieterverzeichnis des LV: Id, Firma, Ort, Ansprechpartner, E-Mail, angefordert, Status, verknüpftes Angebot, Eingang, Nebenangebote, Bemerkung, Absagegrund; Angebote ohne angefragten Bieter mit Vorschlag | lesen |
| `bieter_anlegen` | angefragten Bieter aufnehmen (`name1`, Adresse, `ansprechpartner`, `angefordert` TT.MM.JJJJ, `status`, `angebot`, `eingang`, `nebenangebote`, `bemerkung`, `absagegrund`) | schreiben |
| `bieter_aendern` | Bieter ändern (`bieter` = Id), Angebot verknüpfen (`angebot`, null löst), Status, `absagegrund` (für das Absageschreiben), `entfernen` | schreiben |
| `submission_drucken` | Niederschrift über die Angebotsöffnung (PDF): Eröffnungstermin/Ort/Vergabe aus den Stammdaten, Bieter mit Eingang, Summen netto/Nachlass/brutto, Nebenangebote, Bemerkungen, Unterschriftsfelder | schreiben |
| `anschreiben_drucken` | Schriftverkehr aus der Projektvorlage: `art` aufforderung (setzt Versanddatum im Bieterverzeichnis) / zuschlag (beauftragter Bieter) / absage (§ 134 GWB, übrige Bieter) / freigabe (Abrechnung: an Auftraggeber bzw. Bauherrn zur Rechnung `rechnung`, Datei `<Projekt>_Freigabe_<Rechnungs-Nr.>_<Datum>.pdf`); `bieter` (Id) oder `alle`, sonst Vorschlag je Art; `einzeln` = ein PDF je Bieter in den Ordner `ziel`; Dateinamen wie in proAVA `<Projekt>_<Art>_<JJJJ-MM-TT>[_<Bieter>].pdf`; `ablegen` (Standard true, wie „Im Projekt ablegen“): jedes Schreiben zusätzlich als PDF in der Projektdatei – je Empfänger ein Eintrag, auch beim Sammel-PDF; Logo, Kopf-/Fußzeile und Druckstil aus den Einstellungen; Platzhalter siehe unten | schreiben |
| `schriftverkehr_lesen` | Schriftverkehr im Projekt (ganzes Projekt): abgelegte Schreiben mit Id, Datum, Art (aufforderung, zuschlag, absage, freigabe, eigene), Empfänger (mit Bieter-Id), Betreff, LV, Größe, Datei im Archiv, Bemerkung; Gesamtgröße, Hinweis ab 25 MB; Filter `art`, `empfaenger` (Namensteil) | lesen |
| `schriftverkehr_pdf_speichern` | PDF eines abgelegten Schreibens (`id`) als Datei, `ziel` ohne Angabe neben dem Projekt unter dem Archivnamen (`0003_Absage_Müller_GmbH.pdf`); überschreibt nur mit `ueberschreiben` | schreiben |
| `auftrag_erteilen` | Auftrag an ein Angebot (Datum; Auftragssumme = netto nach Nachlass; nur eines je LV), `aufheben: true` nimmt ihn zurück | schreiben |
| `auftrags_lv_exportieren` | Auftrags-LV des beauftragten (oder genannten) Angebots: `x86` (Standard), `d86`, `pdf` – Bieterpreise, Nachlass, Bieterangaben, AG/AN, Vergabenummer, Ausführungszeit | schreiben |
| `preise_uebernehmen` | Bieterpreise als Planerpreise: `quelle` angebot/mittelwert/median/minimum, `ueberschreiben` | schreiben |
| `abrechnung_lesen` | Abrechnung des LV: festgeschriebener Auftrag (Firma, Auftrags-Nr., Summe, Nachlass, beauftragte Nachträge, Vertragsbedingungen), Rechnungen mit Id, Art, Forderung, geprüfter Leistung, Freigabe, Befunden; mit `rechnung` (Id) je Position Auftrag/bisher/Rechnung/geprüft/Quelle/EP/Beträge/Vermerk/Befunde (`alle_positionen`, `hinweise`) | lesen |
| `freigabe_berechnen` | Rechenweg der Rechnungsfreigabe (`rechnung`): Leistung, Nachlass, Abzüge, Einbehalt, anrechenbare Freigaben, Korrektur, Freigabe netto/brutto, Skonto, Stand, bisherige Freigaben, Befunde | lesen |
| `auftrag_festschreiben` | Auftrag als Grundlage der Abrechnung: aus dem beauftragten Angebot oder mit `datei` aus X86/D86 (Mengen und Preise der Datei); vorhandener Auftrag nur mit `neu: true`; `auftrags_nr` | schreiben |
| `rechnung_anlegen` | neue Rechnung von Hand oder mit `datei` aus X89/X89B (Kopf, Mengen, EP, GP, Prüfvermerke, Aufmaß); Kopf: `art` (abschlag/schluss/einzel/sicherheit), `nummer`, `datum`, `eingang`, `zahlungsziel`, `leistung_von`/`_bis`, `mengen`/`freigabe` (kumuliert/zuwachs), `ust` (19, 7, 13b), `forderung_netto`, `einbehalt_prozent`, `angerechnet`, `pauschal_gefordert`/`_geprueft`, `korrektur`(`_text`), `pruefvermerk`, `geprueft_am`/`_von` | schreiben |
| `rechnung_aendern` | Kopf einer Rechnung (`rechnung`, Felder wie oben) oder `loeschen: true` (Aufmaß bleibt, nicht zugeordnet) | schreiben |
| `rechnungsposition_setzen` | je Position (`oz` oder Liste `positionen`): `rechnung_menge`, `rechnung_ep`, `rechnung_gp`, `gepruefte_menge`, `fortschritt_prozent` (braucht `vermerk`), `quelle` (auto/rechnung/geprueft/aufmass/fortschritt), `vermerk`, `nachweis`; null leert; Antwort mit Befunden und neuer Freigabe | schreiben |
| `aufmass_setzen` | Aufmaßzeilen einer Position (`oz`, `zeilen` je {`ort`, `rechenansatz`}) für `rechnung` (Zuwachs dort) oder nicht zugeordnet; ersetzt, `anhaengen: true` hängt an; mit `datei` eine X31 (Aufmaßmenge je Position, REB-Zeilen als Text) | schreiben |
| `nachtraege_lesen` | Nachträge des LV: Liste (Nr., Bezeichnung, Anlass, Status mit Datum, angeboten, geprüft, beauftragt nach Nachlass, Prognose), Summen je Status, Auftragssumme alt/neu, Prognose; mit `nachtrag` („N03“ oder 3) ein Nachtrag mit Kopf, Fristen, Prüfvermerk, Empfehlung, Nachtragsbereich und nächster OZ, Positionen (neu/Mehr-/Mindermenge, angeboten/geprüft, Abw. %, Status, Vermerk), **Prüfhilfe** je Position (Auftrags-EP, ähnliche Auftragsposition, Planerpreis, Preisspiegel mit Abweichung), Statusverlauf, Hinweise | lesen |
| `nachtrag_anlegen` | Nachtrag anlegen (`bezeichnung`, `anlass` geändert/zusätzlich/mengenänderung/ohne_auftrag/behinderung/sonstig, `begruendung`, `firma`, Fristen …; Nummer = nächste freie); liegt noch kein Auftrag vor, aber ein beauftragtes Angebot, wird es als Auftrag übernommen (Antwort sagt es) | schreiben |
| `nachtrag_aendern` | Kopf und Prüfung: `bezeichnung`, `anlass`, `begruendung`, `firma`, `angekuendigt`, `angebotsfrist`, `angebot_nr`, `angebot_vom`, `entscheidung_bis`, `entschieden_am`, `geprueft_am`, `geprueft_von`, `pruefvermerk`, `empfehlung`, `nachlass_gilt`, `prognose`, `nummer`; `loeschen: true` (mit `im_lv_loeschen`, Standard ja) | schreiben |
| `nachtragsposition_setzen` | `neu: true` = neue Position im Nachtragsbereich am Ende des LV (Felder wie `position_einfuegen`, OZ-Vorschlag oder `oz_neu`); sonst `oz`/`id`: Auftragsposition → Mehr-/Mindermenge (Differenz, ohne EP Auftrags-EP), andere Position → neue Nachtragsposition, Titel → alle freien Positionen darunter; Werte `angeboten_menge`, `angeboten_ep`, `geprueft_menge`/`geprueft_ep` (null = wie angeboten), `vermerk`, `status` (abweichend, z. B. abgelehnt); `entfernen` (+ `im_lv_loeschen`) | schreiben |
| `nachtrag_status` | Statuswechsel mit `datum` (Standard heute) und `vermerk` → Statusverlauf; beauftragt = abrechenbar | schreiben |
| `nachtrag_drucken` | PDF `art` `pruefung` (Prüfung Nachtragsangebot je `nachtrag`) oder `uebersicht` (Nachtragsübersicht); Kopf-/Fußzeile und Logo wie in proAVA | schreiben |
| `nachtrag_gaeb_exportieren` | Nachtrags-LV als `X83` (Nachtragsangebotsaufforderung, ohne Preise, COStatus Filed) oder `X86` (Nachtragsauftrag, geprüfte Preise, COStatus je Position, abgelehnte als „entfällt“; Fachregeln der geschriebenen Datei als Hinweis) mit CONo = Nachtragsnummer; Mehr-/Mindermengen nicht (GAEB kennt keine Teilmengen, Hinweis) | schreiben |
| `nachtragsangebot_einlesen` | GAEB-Datei der Firma (X84, X86, X82, X83, D8x) in einen Nachtrag: gleiche OZ (oder gleicher Kurztext + Einheit bei fremder OZ) → angebotene Menge/EP (geprüfte Werte bleiben, Datei ohne Preise lässt den EP), Auftragspositionen → Mengenänderung, unbekannte → neue Positionen mit Text im Nachtragsbereich; angekündigt → angeboten (Datum aus der Datei) | schreiben |

**Schriftverkehr – Platzhalter** (in Betreff und Text der Vorlagen, in proAVA unter Angebote ▸
Schriftverkehr bearbeitbar): `{bieter.name}`, `{bieter.zusatz}`, `{bieter.strasse}`, `{bieter.plz_ort}`,
`{bieter.anschrift}`, `{bieter.email}`, `{ansprechpartner}`, `{anrede}`, `{projekt}`, `{projektbezeichnung}`,
`{baumassnahme}`, `{bauherr}`, `{baustelle}`, `{lv}`, `{lv.nummer}`, `{vergabeart}`, `{vergabenummer}`,
`{abgabetermin}`, `{abgabeort}`, `{bindefrist}`, `{ausfuehrung_beginn}`, `{ausfuehrung_ende}`, `{planer.name}`,
`{planer.anschrift}`, `{planer.telefon}`, `{planer.email}`, `{datum}`, `{angebot.summe}`, `{auftrag.summe}`,
`{zuschlag.bieter}`, `{absagegrund}` (je Bieter im Bieterverzeichnis, `bieter_aendern absagegrund`); für die
Freigabe an den Bauherrn zusätzlich `{auftragnehmer}`, `{auftrag.nummer}`, `{rechnung.nummer}`, `{rechnung.art}`,
`{rechnung.datum}`, `{rechnung.eingang}`, `{rechnung.zahlungsziel}`, `{rechnung.forderung}`, `{rechnung.ust}`,
`{freigabe.netto}`, `{freigabe.brutto}`, `{freigaben.gesamt}`, `{einbehalt}` (in den übrigen Schreiben „–“). Leere Angaben erscheinen als „–“. Die Niederschrift über die Angebotsöffnung gibt es
auch als `pdf_drucken` mit `dokument: niederschrift`.

**Kosten-LVs:** Ein LV mit der Art Kostenschätzung/Kostenberechnung (Kosten-LV) geht nie an Bieter: `bieter_anlegen`/`_aendern`, `submission_drucken`, `anschreiben_drucken`, `angebot_einlesen`, `angebot_nachlass_setzen`, `auftrag_erteilen`, `preise_uebernehmen`, `auftrags_lv_exportieren`, `auftrag_festschreiben`, die Rechnungs-, Aufmaß- und Nachtragswerkzeuge, `gaeb_exportieren` mit `X83` und `pdf_drucken` mit einem Vergabe-/Abrechnungsdokument lehnen es mit Meldung ab (`apps/proava-mcp/src/cost_lv_tools.rs`). `lv_pruefen` meldet im Kosten-LV KG, die fehlen oder gröber sind als die Stufe verlangt.

**Mehrere LVs:** Ein Projekt kann mehrere LVs haben (`lvs_lesen`). Alle Werkzeuge, die mit
einem LV arbeiten (lesen, einfügen, ändern, Angebote, Preisspiegel, GAEB, PDF …), nehmen das
optionale Argument `lv`: zuerst die LV-Nummer (`"2"`, `"481"`, auch `"LV 2"`), sonst die
Position aus `lvs_lesen` (`1` = erstes). Ohne `lv` gilt das in proAVA aktive LV. Das
Speichern ändert nicht, welches LV proAVA beim Öffnen zeigt.

Zeilen werden per OZ angesprochen (`01.02.0010`, führende Nullen dürfen fehlen: `1.2.10`)
oder per Zeilen-Id (`#12` aus `lv_lesen`) – Hinweise haben keine OZ, und bei doppelten OZ
nennt der Server die Ids.

### Langtext-Auszeichnung

```
Liefern und montieren:
- **DDC-Station** mit *Bedienteil*
- __Netzteil__ 24 V
1. erster Schritt
| Typ | Anzahl |
|---|---|
| AI | 8 |
[Bild: plan.png, 60 mm]
```

Zeilen = Absätze, `**fett**`, `*kursiv*`, `__unterstrichen__`, `- ` Aufzählung, `1. `
nummeriert, `| … |` Tabelle, `\` vor einem Zeichen = wörtlich, `\` am Zeilenende = Zeilenumbruch
im Absatz (GAEB `<br/>`). Leere Zeilen am Ende bleiben leere Absätze (wie GAEB-Texte oft enden); ein
einzelner Zeilenumbruch am Ende zählt nicht.

**Unterpunkte** (Listenebenen): je Ebene zwei Leerzeichen (oder ein Tab) mehr vor
`- ` bzw. `1. ` – bis zu acht Ebenen:

```
Inhalte:
1. Automationsebene
  - Geräteparameter
  - Alarme und Störungen
2. Leitebene
  1. Software prüfen
    - Firmware
  2. Datensicherung
```

`position_lesen` zeigt sie genauso; die Zahlen zählen je Ebene. Gedruckt werden Nummern je Ebene als
„1.“, „a)“, „i.“, Aufzählungszeichen auf allen Ebenen als „•“ (GAEB trägt nur nummeriert/unnummeriert, nicht
die Art der Nummer). In GAEB DA XML stehen Unterpunkte als Liste im Listenpunkt (`<ul>` in `<li>`), in
GAEB 90 als Einrückung. Ein Absatz, der mit Leerzeichen beginnt, ohne `- `/`1. ` dahinter, bleibt ein
Absatz mit diesen Leerzeichen.

Textergänzungen (nur Positionen): `{TE 11}` markiert die Stelle der Textergänzung mit Kennung 11 im
Langtext – allein auf einer Zeile oder mitten im Satz; die Angaben stehen im Feld `textergaenzungen`
(`[{kennung: 11, art: "AG"|"Bieter", vortext, inhalt, nachtext}]`, ersetzt die Liste; bei `zeile_aendern`
behalten ausgelassene Felder ihren Wert). Ohne Marke steht sie nach dem Text. `position_lesen` zeigt die
Marken und die Liste. Weitere Positionsfelder: `ep_aufgliederung` (GAEB UPBkdn) und `ep_anteile_werte`
(ein Wert je EP-Anteil des LV). Bilder kommen mit `bild_einfuegen` aus einer Datei ins Projekt; danach
kann `[Bild: name, breite mm]` auch in `langtext`/`text` stehen (ein unbekannter Name wird abgelehnt).

Weitere Formatierung:

- Ausrichtung als Zeilenanfang: `{mittig} `, `{rechts} `, `{blocksatz} ` (links ist Standard),
  bei Aufzählungen nach dem Zeichen: `- {rechts} Text`; in Tabellenzellen ebenso.
- Schrift und Größe: `{schrift: Times New Roman; größe: 12}Text{/}` – eines von beiden genügt
  (`{größe: 8}klein{/}`), `{/}` kehrt zur Standardschrift (Arial, 10 pt) zurück. Gedruckt wird mit
  Liberation Sans/Serif/Mono (Arial/Times/Courier); der geschriebene Name bleibt erhalten.
- Tabellenkopf: eine Trennzeile `|---|---|` nach der ersten Zeile macht sie zur Kopfzeile;
  `:---:` / `---:` richten die Spalte mittig / rechts aus.
- Spaltenbreiten in mm stehen in der Trennzeile hinter den Strichen: `|--- 15,35 mm|:---: 48,95 mm|`
  (Komma oder Punkt, Spalten ohne Angabe passen sich an). Steht die Trennzeile vor der ersten
  Zeile, hat die Tabelle keine Kopfzeile – so bekommt auch eine Tabelle ohne Kopf Breiten.
  `position_lesen` zeigt gesetzte Breiten genauso; in GAEB gehen sie als `width` mit.

### Nummerierung

Ist jede OZ die, die „neu nummerieren“ schreiben würde („fortlaufend“), nummeriert der
Server nach jeder Strukturänderung neu. Hat das LV eigene OZ (z. B. aus GAEB übernommen),
bleiben sie; eine neue Zeile bekommt eine freie OZ zwischen den Nachbarn (0015 zwischen 0010
und 0020, 0020 zwischen 0010 und 0040, Titel 02 zwischen 01 und 03). Ist dort nichts frei,
meldet das Werkzeug einen Fehler und schlägt `neu_nummerieren` vor. Die Logik liegt in der
Domäne: `Lv::is_auto_numbered`, `Lv::insert_numbered`, `Lv::free_ordinal_at`.

Eigene OZ gleich beim Einfügen: `oz` bei `titel_einfuegen`, `position_einfuegen` und je Zeile von
`positionen_einfuegen` (z. B. LVs mit Titeln 481, 482 … oder Lücken wie 489.05). Geprüft gegen
die OZ-Maske (Ebene, Stellen, Höchstwerte; führende Nullen werden ergänzt), vergebene OZ und den Titel
darüber; die übrigen Zeilen behalten ihre OZ.

### Eingabegrenzen

Zahlen prüft der Server mit denselben Grenzen wie die Eingabefelder des Programms
(`proava_domain::input_limits`, eine Stelle für beide): Menge und EP höchstens drei
Nachkommastellen, Betrag unter 100.000.000 (auch negativ); Nachlass in % über 0 und unter 100,
als Betrag über 0 € mit zwei Nachkommastellen (0 = kein Nachlass); Kostenermittlung: Kennwert,
Spanne, Preis je DP-Art und Anteil in % mit zwei Nachkommastellen, nicht negativ; Anzahl
Datenpunkte ganzzahlig unter 100.000.000 (auch im CSV-Import). Zahlen als Text werden deutsch
gelesen wie im Programm („1.150“ = 1150, „1.234,5“, „2.5“ = 2,5). Ein Verstoß lehnt den Aufruf
mit Feldname und Grenze ab, z. B. „„menge“: die Zahl ist zu groß (höchstens 99.999.999,999).“, und
ändert nichts. Texte haben keine harte Grenze; Kurztext über 70 und Einheit über 4 Zeichen
nennen `position_einfuegen` und `zeile_aendern` als „Hinweis: …“ (GAEB-Grenze, wie die Oberfläche
am Feld). Die Abrechnung (Prozente, Beträge) prüft weiter nur das Zahlenformat – wie ihre Felder
im Programm.

### Kopf- und Fußzeile

Jede Seite der Ausdrucke hat eine Kopf- und eine Fußzeile mit je drei Feldern (links, Mitte, rechts),
ein- oder zweizeilig (`\n`), aus Text und Platzhaltern. Vorgabe in proAVA unter Extras ▸ Einstellungen ▸
Kopf-/Fußzeile (der MCP-Server liest sie beim Start), im Projekt überschreibbar (Projektstammdaten ▸
Kopf-/Fußzeile bzw. `projektangaben_setzen`). Briefe nutzen nur die Fußzeile.

Schlüssel (in `projektangaben_setzen` und `pdf_drucken`): `kopf_links`, `kopf_mitte`, `kopf_rechts`,
`fuss_links`, `fuss_mitte`, `fuss_rechts` (Text), `kopf_linie`, `fuss_linie` (true/false), `kopf_logo`
(`aus`, `links`, `rechts`), `kopf_schrift_pt` (7–10). Unbekannte Platzhalter werden abgelehnt.

Platzhalter: `{projekt}`, `{projektnummer}`, `{projektbezeichnung}`, `{baumassnahme}`, `{lv}`, `{lv_nr}`,
`{lv_bezeichnung}`, `{dokumentart}`, `{titel}` (Titel, in dem die Seite beginnt – LV-Ausdrucke), `{datum}`,
`{stand}`, `{seite}`, `{seiten}` (bei Briefen je Brief), `{planer}`, `{bearbeiter}` und `{ersteller}` (Bearbeiter aus
Einstellungen ▸ Drucken, sonst der Planer), `{bauherr}`,
`{auftraggeber}`, `{vergabenummer}`, `{waehrung}` (Währung des LV, z. B. EUR), `{datei}` (Name der PDF), `{kopftext}`, `{fusstext}`. Eine Zeile, deren
Platzhalter alle leer sind, entfällt; zu lange Texte werden mit „…“ gekürzt.

Standard: Kopf `{projekt}` | `{dokumentart}` + `{kopftext}` | `{datum}`, Fuß `{lv}` | `{bearbeiter}` +
`{fusstext}` | `Seite {seite} von {seiten}`, Linien an, Logo nur im Titelblock, 8 pt.

### Weitere Felder

- **Vor- und Schlussbemerkungen:** `hinweis_einfuegen`, Hinweiszeilen von `positionen_einfuegen` und `zeile_aendern`
  nehmen `vorbemerkung` bzw. `schlussbemerkung` (true/false, nie beides). GAEB DA XML schreibt Vorbemerkungen vor das
  LV und Schlussbemerkungen dahinter (nicht in X84), GAEB 90 Schlussbemerkungen als Hinweis am Ende; der Druck setzt
  sie nach den Summen. Textergänzungen (`{TE n}` + `textergaenzungen`) gehen auch in Hinweisen und
  Ausführungsbeschreibungen.
- **Zeit:** `zeitansatz` je Position (GAEB `TimeQu`, höchstens vier Nachkommastellen), `projektangaben_setzen`
  `zeiteinheit` (GAEB `LblTime`, höchstens 20 Zeichen).
- **Zuschlagspositionen:** `zuschlag` `{art: "alle" | "gekennzeichnet" | "liste", prozent, bezug: ["01.0010", {oz,
  teilmenge}]}` (`false`/„“ = wieder normale Position; ohne Menge und Einheit), dazu `zu_bezuschlagen` (GAEB
  `MarkupIt`), `entfaellt` (`NotAppl`) und `unterbeschreibungen` (`[{nummer, kurztext, langtext, menge, einheit}]`,
  ersetzt die Liste).
- **Aufteilungszeilen im Kosten-LV:** `aufteilung` wie `zuschlag` (`{art, prozent, bezug}`) oder als Zahl
  (`aufteilung: 40` = 40 % auf die Positionen davor im Titel), KG mit `kg`. Die bezogenen Positionen sind
  Rechengrundlage und zählen nicht selbst; ein Block mit 100 % gibt den Rundungsrest der größten Zeile; `lv_pruefen`
  meldet Blöcke ≠ 100 %. Nur im Kosten-LV.
- **Grund-/Wahlpositionen und Wiederholungen:** `wahlgruppe` und `wahl_nr` (Standard je 1); `wiederholung_von` = OZ der
  Position, deren Beschreibung diese wiederholt (GAEB `RefDescr`, „“ löst).
- **Pauschalsummen:** `zeile_aendern` an einem Titel bzw. `projektangaben_setzen` für das LV mit `pauschalsumme`
  (GAEB `TotalLSUM`; null/„“ = keine).
- **OZ:** Index mit Buchstaben („01.01.0004.A“, wird groß geschrieben); Positionsnummern mit Buchstaben („01.01.3A“),
  wenn `projektangaben_setzen positionen_mit_buchstaben` sie erlaubt. Eine Position direkt unter einem höheren Titel
  (wie aus GAEB übernommen) schreibt der GAEB-Export ohne Zwischenstufe.
- **Ausführungsbeschreibungen:** ohne `nummer` zählt proAVA nach Lage im LV; Einfügen, Kopieren, Verschieben und
  Löschen halten fortlaufende Nummern fortlaufend, Bezüge hängen an der Zeilen-Id. In GAEB steht die Nummer ohne
  Nullen (`PerfNo`).
- **Anschriften:** `bauherr`, `auftraggeber`, `aufsteller`, `baustelle`, `adressen` und Bieter mit allen GAEB-Feldern,
  auch `name3`, `name4`, `land`, `ansprechpartner`, `fax`, `ust_id`, `iln`. Ohne eigenen Auftraggeber gilt der Bauherr
  (GAEB `OWN`, Deckblatt, Kopfzeile, Niederschrift). `projektangaben_setzen` dazu: `datenaustauschnummer`,
  `baustelle_nummer`, `baustelle_bezeichnung`, `benachrichtigungsort`, `weitere_ausfuehrungsorte`, `bedarfstraeger`,
  `debitorennummer`, `auftraggeber_staat`, `auftraggeber_kfz_kennzeichen`. Es nennt alle unplausiblen Angaben auf
  einmal und speichert nur ganz oder gar nicht.
- **Sonst:** `pdf_drucken lesezeichen_positionen` (Lesezeichen je Position, Standard an); `preisspiegel_lesen
  ausreisser_prozent` (Standard 20); `rechnung_anlegen`/`rechnung_aendern` `art`: `abschlag` (Standard), `schluss`
  (immer kumuliert, alle Freigaben angerechnet), `einzel` (Regie, Nachtrag; außerhalb der Folge), `sicherheit`
  (Freigabe des Einbehalts, `sicherheit_freigabe`). Nicht per MCP eingebbar: die Art der EP-Anteile (sie folgt aus der
  Bezeichnung).

### Abläufe

- **Positionen aus anderen LVs:** `lv_suchen` (`projekt`, `ordner` oder `datei`) → Treffer nennen →
  `positionen_uebernehmen` mit `quelle` = gefundene Datei, `quelle_lv` und `oz` bzw. `ids`.
- **Zwischen Fenstern:** (a) „im rechten Fenster die Position für den Schaltschrank zeigen und links an die
  Schreibmarke kopieren“ = `fenster_suchen` rechts → `fenster_zeigen` → Bestätigung → `fenster_einfuegen` links mit
  `quelle_fenster` rechts, `auswahl: true`; (b) „LV aus Projekt xy, such nach …, die markierte und die nächsten
  drei“ = `fenster_projekt_oeffnen` → `fenster_suchen` → `fenster_zeigen` → `fenster_einfuegen` mit
  `auswahl: true, weitere: 3`.
- **Nachträge:** `nachtrag_anlegen` → `nachtragsposition_setzen` (neu / Mehrmenge) → `nachtrag_gaeb_exportieren` X83
  an die Firma → `nachtragsangebot_einlesen` → `nachtraege_lesen` mit `nachtrag` (Prüfhilfe) →
  `nachtragsposition_setzen` `geprueft_ep`/`vermerk` → `nachtrag_aendern` Prüfvermerk/Empfehlung →
  `nachtrag_drucken` `pruefung` → `nachtrag_status` beauftragt.
- **Abrechnung:** Rechnungen haben je LV eine Id (#1, #2 …). Mengen gelten im Sinn der Rechnung (kumuliert oder
  Zuwachs); Aufmaßzeilen zählen in der zugeordneten Rechnung als Zuwachs. GAEB: X86/D86 (Auftrag), X89/X89B
  (Rechnung), X31 (Mengenermittlung). REB-Formeln rechnet proAVA nicht nach, es zählt die Aufmaßmenge der Datei;
  Positionen nur mit REB-Zeilen nennt die Antwort.
- **CSV:** Weicht ein GP beim Import von Menge × EP ab, bleibt eine Marke an der Position, bis sie in proAVA bestätigt
  („GP-Abweichung bestätigen“) oder Menge/EP geändert wird. `gaeb_importieren` liest CSV ebenfalls.
- **Schriftverkehr:** Abgelegte Schreiben liegen als PDF im Ordner `schriftverkehr/` der Projektdatei, die Liste in
  `projekt.xml`; je Empfänger ein Eintrag, auch beim Sammel-PDF; die Ablage gilt für das ganze Projekt.
- **Kostenverfolgung:** Die Regeln der Kette (Angebot = beauftragt, gewählt oder Mindestbieter nach Nachlass; Auftrag
  = Auftragssumme anteilig je KG + beauftragte Nachträge; Prognose je LV = Kostenfeststellung, sonst Auftrag + offene
  Nachträge + Mehrleistung der Abrechnung, sonst Angebot oder Kostenvoranschlag) liegen in
  `proava_domain::cost_tracking` und gelten gleich im Programm, im Druck, in der CSV und hier. `vergleich` vergleicht
  jede Stufe mit derselben Stufe des festgehaltenen Kostenstands; mit `brutto` rechnet jedes LV mit seinem eigenen
  USt-Satz.

## Sicherheit

- **Freigabe:** Lesende Werkzeuge haben `readOnlyHint`; alle anderen brauchen die Freigabe des Anwenders und
  sollen im MCP-Client auf Rückfrage (`ask`) stehen.
- **Ordner-Sperre:** `PROAVA_MCP_ORDNER` (Pfade mit `:` getrennt, unter Windows mit `;`) begrenzt alle Lese- und
  Schreibzugriffe auf diese Ordner (Symlinks aufgelöst). Ohne Variable gibt es keine Grenze.
  Nur absolute Pfade, Projekte nur mit Endung `.proava`.
- **Sicherung:** Das erste Schreiben je Serverlauf und Projekt legt vorher eine Zip-Sicherung
  `<Name>_Sicherung_<Datum>.zip` neben das Projekt (wie „Sicherung anlegen“ in proAVA) und
  nennt sie in der Antwort. Selbst angelegte Projekte brauchen keine.
- **Gleichzeitige Bearbeitung:** Hat sich die Datei zwischen Laden und Speichern geändert
  (Zeitstempel und Größe), speichert der Server nicht und meldet es. Ist das Projekt in einem
  proAVA-Fenster zum Bearbeiten offen, arbeitet jedes Werkzeug dort statt in der Datei (Abschnitt
  „Werkzeuge im offenen Fenster“); die Datei bleibt dann unberührt.
- **Andere Programme:** Der Server startet nur `curl` (Katalogverzeichnisse von AUSSCHREIBEN.DE,
  `ausschreiben_suchen`) und das Browserfenster `proava-web` (`ausschreiben_oeffnen`, eigenständig,
  höchstens eins je Server; ein zweites für dasselbe Profil meldet sich und endet). Mit
  `PROAVA_KEINE_PROGRAMME=1` startet er kein Fenster (Tests setzen das).
- **Überschreiben:** GAEB- und PDF-Ziele werden nur mit `ueberschreiben: true` ersetzt;
  `projekt_anlegen`, `lv_aus_gaeb` und `gaeb_importieren` legen nie über ein vorhandenes Projekt.
- **Rückfrage bei Angeboten oder Auftrag:** Kommen in einem LV mit eingelesenen Angeboten oder Auftrag Positionen
  hinzu oder fallen solche weg, die ein Angebot oder der Auftrag enthält, speichert der Server nichts und antwortet
  „Rückfrage – nichts gespeichert: …“ (mit Auftrag zusätzlich der Weg über `nachtrag_anlegen` und
  `nachtragsposition_setzen neu: true`). Erst nach dem Ja des Anwenders mit `bestaetigen: true` wiederholen. Die
  Prüfung sitzt zentral vor dem Speichern (`Server::save`, `Project::uncovered_positions`); Nachtragspositionen zählen
  nicht. Im offenen Fenster fragt stattdessen das Fenster.
- **Projektsperre:** Auf dem Dateiweg speichert der Server nicht, wenn ein anderes proAVA das Projekt gesperrt hat
  (`.~lock.<Datei>#`) und kein Fenster über die Brücke erreichbar ist (anderer Rechner, anderer Benutzer): „Nichts
  gespeichert: … ist in proAVA geöffnet von <Benutzer> auf <Rechner> seit <Zeit> …“. Lesen geht. Eine verwaiste
  Sperre (dieser Rechner, unter der PID läuft kein proAVA mehr) zählt nicht.

## Brücke zu den geöffneten Fenstern

Jedes laufende proAVA (nur das echte Programm, nie in Tests) öffnet einen lokalen Kanal und
meldet sich in einem Verzeichnis an; der MCP-Server liest die Einträge und fragt die Fenster.
Protokoll: `crates/proava-bridge-protocol` (JSON-Zeilen, `Request {v, id, befehl}` →
`Response {v, id, antwort}`, Version 1; eine andere Version wird mit Begründung abgelehnt).

| Teil | Wo |
|---|---|
| Protokoll, Fensterverzeichnis-Eintrag, „links/rechts/aktiv“ | `proava-bridge-protocol` (rein, getestet) |
| Port `WindowChannel`, Weiterreichen an den UI-Thread, Zeilen benennen (Auswahl, OZ, `#Id`, `weitere`) | `proava_application::window_bridge` |
| Ordner, Einträge, Unix-Socket, Named Pipe, Client `ask` | `proava_infrastructure::window_bridge` |
| Befehle im Fenster (UI-Thread), KI-Markierung, Statusleiste, Esc | `proava_ui::bridge`, `workspace/bridge_shell.rs` |
| Werkzeuge `fenster_*`, alle Werkzeuge im offenen Fenster (`window_state`, `adopt`) | `apps/proava-mcp/src/window_tools.rs`, `Server::open_path`/`save` |

- **Ort:** Linux `$XDG_RUNTIME_DIR/proava/` (Ordner 0700) mit `<pid>.json` (Eintrag: Kanal,
  Fensterlage x/y/Breite/Höhe, Bildschirmgröße, Fokus, Projekt, Datei, LV, Auswahl,
  ungespeichert, schreibgeschützt, offener Dialog, KI-Tätigkeit) und `<pid>.sock` (0600);
  Windows `%LOCALAPPDATA%\proava\fenster\<pid>.json` und eine Named Pipe
  `\\.\pipe\proava-fenster-<pid>-<zufall>` mit DACL nur für den Besitzer und SYSTEM,
  `PIPE_REJECT_REMOTE_CLIENTS`, erste Instanz exklusiv. Ohne `XDG_RUNTIME_DIR` gibt es keine Brücke.
- **Aufräumen:** beim Beenden Eintrag und Socket weg; beim Start entfernt jedes Fenster Einträge
  und Sockets von PIDs, die nicht mehr laufen (wie die PDF-Kopien in `viewer_copies`).
- **Befehle** (`Command`): `fenster_lesen`, `zeigen`, `markieren`, `lesen`, `kopieren` (Zeilen
  als proAVA-Zwischenablagetext, dieselben Daten wie Strg+C), `einfuegen` (dieser Text an der
  Schreibmarke über `LvEditor::paste_transfer` – derselbe Weg wie Strg+V, ein Rückgängig-Schritt),
  `projekt_oeffnen`, `suchen`, `stand`, `uebernehmen`, `speichern` (Abschnitt „Werkzeuge im
  offenen Fenster“). Der Kanal reicht jede Anfrage an den UI-Thread und weckt das
  Fenster; es antwortet im nächsten Bild (Wartezeit höchstens 60 s, sonst „beschäftigt“).
- **Rückfragen:** Einfügen in ein versandtes LV löst die übliche Frage „LV bereits versandt“
  aus; die Antwort wartet, bis der Anwender entschieden hat (höchstens 25 s, dann „wartet auf den Anwender“),
  und sagt, ob er behalten oder zurückgenommen hat. Schreibgeschützte Fenster und offene Dialoge
  lehnen das Einfügen ab; fertige Positionen bleiben gesperrt (Einfügen dahinter geht wie Strg+V).
- **Sichtbar:** KI-Markierung (eigene Farbe `Palette::ai`, Rahmen um die Zeilen), Statusleiste
  „KI: …“ und „KI verbunden“ (Anfrage in den letzten 3 Minuten); Esc nimmt beides weg und lehnt
  15 s lang jede Anfrage ab („bitte nachfragen“).
- **Ziehen zwischen Fenstern** (kein KI-Werkzeug, eigene Mausarbeit des Anwenders, `crate::window_drag`,
  `workspace/drag_shell.rs`): Verlässt ein Zug (Zeilen der Tabelle, Fenster „Übernehmen“, Seitenfenster
  „Bibliothek“) das Fenster, rechnet es den Zeiger in Desktop-Pixel um (`inner_rect` × Pixel je Punkt), sucht im
  Verzeichnis das Fenster darunter (`innen_px` im Eintrag, `window_at`; übereinander: das zuletzt aktive) und
  schickt ihm `ziehen_ueber` (höchstens 30 je Sekunde, ohne Bewegung alle 0,2 s), `ziehen_ende` bzw. beim
  Loslassen `ablegen` mit den Zeilen als Zwischenablagetext – auf einem Hilfsthread, Bewegungen gebündelt. Das
  Zielfenster zeigt die Einfügemarke wie beim Ziehen aus dem Übernahme-Fenster, einen Hinweis am Zeiger, scrollt am
  Tabellenrand mit und fügt beim Ablegen über denselben Weg wie `einfuegen` ein (`insert_clip`, Strg+Z ein Schritt;
  schreibgeschützt, offener Dialog, versandtes LV wie von Hand). Immer Kopie. Unter Wayland (keine Fensterlage):
  Hinweis „Zwischen Fenstern ziehen: Strg+C/Strg+V“. Diese drei Befehle gehen auch, wenn die KI-Bedienung aus ist:
  **der Kanal läuft immer, nur die KI-Befehle hängen an der Einstellung.**
- **Einstellung:** Extras ▸ Einstellungen ▸ Darstellung ▸ „KI darf geöffnete Fenster bedienen“
  (`settings.json` `preferences.ai_windows`, nur geschrieben, wenn aus). Aus: jede Anfrage bekommt
  `Reply::Aus` mit Hinweis; der Eintrag bleibt (ohne Auswahl), damit der Server das offene Projekt
  erkennt: Werkzeuge lesen dann die Datei und ändern nichts (Hinweis: Einstellung einschalten oder
  dort speichern und schließen).
- **Sicherheit:** nur der eigene Benutzer, nur lokal (kein Netzwerk-Port), abschaltbar; nichts
  startet einen Prozess außer `projekt_oeffnen` im echten Programm über die Sperre
  `jobs::new_window` (nur proAVA-Programmdateien, höchstens 8 Fenster nacheinander).

### Werkzeuge im offenen Fenster

Ist ein Projekt in einem proAVA-Fenster zum Bearbeiten offen (Eintrag im Brückenordner, nicht schreibgeschützt),
arbeitet **jedes** Werkzeug dort:

- **Lesen:** `Server::open_path` fragt das Fenster nach seinem Stand (`stand`). Das Fenster schreibt sein Projekt –
  auch ungespeichert – in den Austauschordner `<Brückenordner>/austausch` (nur der eigene Benutzer) und merkt sich den
  Stand; der Server liest die Datei und löscht sie. So sehen auch Lesewerkzeuge, Drucke und Exporte den ungespeicherten
  Stand.
- **Ändern:** Das Werkzeug läuft unverändert auf diesem Stand; `Server::save` schreibt das Ergebnis in den
  Austauschordner und schickt `uebernehmen`. Das Fenster nimmt nur Dateien aus seinem Austauschordner und nur auf dem
  Stand, den es zuletzt herausgegeben hat – sonst „inzwischen geändert, Werkzeug noch einmal aufrufen“.
  Schreibgeschützt, offener Dialog, Zelle in Bearbeitung oder Esc-Sperre: abgelehnt wie bei Handarbeit.
- **Ein Schritt:** Der neue Stand ist ein Rückgängig-Schritt („Änderung der KI“, Strg+Z/Strg+Y); das Fenster zeigt
  das geänderte LV und markiert die geänderten Zeilen. Gespeichert wird erst mit `fenster_speichern` (vorher eine
  Sicherung je Serverlauf).
- **Rückfragen:** dieselben wie nach einer Änderung von Hand (versandtes LV, Positionen bei Angeboten oder Auftrag);
  die Antwort wartet höchstens 25 s („wartet auf den Anwender“), „Abbrechen“ nimmt den Schritt zurück. `bestaetigen`
  braucht es im Fenster nicht.
- **Einstellung aus:** Lesen aus der Datei, Ändern abgelehnt mit Hinweis.
- Über den einen Engpass `open_path`/`save` gelten Fenster und Datei für alle Werkzeuge gleich; Dateien (Bild, PDF,
  GAEB, STLB) liest weiter der MCP-Prozess mit seiner Ordnergrenze.

## Einbindung in einen MCP-Client (Beispiel opencode)

`scripts/install_linux.sh` legt das Programm unter `~/.local/bin/proava-mcp` ab. In opencode steht
der Server in `~/.config/opencode/opencode.jsonc` unter `mcp.servers` (Pfad zum Binary anpassen);
die schreibenden Werkzeuge stehen in `permissions` auf `ask`, die Lese-Werkzeuge (readOnlyHint) auf
`allow` (die spätere Regel gilt):

```jsonc
{
  "mcp": {
    "servers": {
      "proava": {
        "type": "local",
        "command": ["/home/<name>/.local/bin/proava-mcp"],
        "environment": {
          "PROAVA_MCP_ORDNER": "/home/<name>/Projekte:/tmp"
        },
        "enabled": true
      }
    }
  },
  "permissions": {
    "proava_*": "ask",
    "proava_projekte_suchen": "allow",
    "proava_lv_suchen": "allow",
    "proava_projekt_lesen": "allow",
    "proava_lvs_lesen": "allow",
    "proava_bieter_lesen": "allow",
    "proava_lv_lesen": "allow",
    "proava_position_lesen": "allow",
    "proava_lv_pruefen": "allow",
    "proava_angebote_lesen": "allow",
    "proava_preisspiegel_lesen": "allow",
    "proava_fenster_lesen": "allow",
    "proava_fenster_zeigen": "allow",
    "proava_fenster_suchen": "allow"
  }
}
```

Der Server liest `~/.config/proava/settings.json` nur, um das Logo wie in
proAVA zu drucken und für `rechtschreibung_pruefen` den Wörterbuch-Ordner und die eigenen
Wörter zu kennen (Einstellungen ▸ Darstellung); er schreibt dort nie. Das Hunspell-Wörterbuch
(`de_DE.aff/.dic`) kommt vom System und wird beim ersten Aufruf geladen.

Für andere MCP-Clients genügt `command` = Pfad des Binarys, Transport stdio.

## Für Entwickler

`cargo test -p proava-mcp`: Binary per stdio (initialize, tools/list, Aufruf, Ordner-Sperre), alle Werkzeuge über den
Handler in Temp-Ordnern (Schreiben, Lesen, Nummerierung, zweiter Schreiber und Sicherung, GAEB 90, PDF, mehrere LVs,
Stammdaten, Bieterverzeichnis, Rechtschreibung mit Testwörterbuch über `Server::with_speller`). Die Fenster-Werkzeuge
laufen in `tests/windows.rs` gegen eine aufzeichnende Test-Brücke; `tests/window_writes.rs` lässt die
Schreibwerkzeuge einmal auf der Datei und einmal über ein offenes Projekt im Speicher laufen (gleiches Ergebnis, Datei
unberührt). Die Brücke im Fenster prüft `crates/proava-ui/tests/ui_bridge.rs` mit echtem Socket im Temp-Ordner.

**Größe der Werkzeugliste:** `tools/list` geht bei jeder Sitzung an das Modell. Allgemeine Regeln stehen deshalb
einmal in den Server-Hinweisen (`INSTRUCTIONS` in `server.rs`), die Werkzeugtexte (`catalog.rs`) sagen nur, was das
Werkzeug tut und was ein Modell sonst falsch macht; Lesewerkzeuge tragen `readOnlyHint`. Die Tests in
`tests/tools.rs` halten fest: `tools/list` höchstens 68.000 Zeichen, je Werkzeug 3.000 (`projektangaben_setzen`
5.500), Server-Hinweise 6.000; mindestens 5.000 Zeichen Luft halten – neue Werkzeuge und Parameter knapp beschreiben,
nicht die Grenze heben. Das Schema steht in `tests/katalog_schema.jsonl` (ohne Texte); nach einer gewollten
Schemaänderung neu schreiben mit
`PROAVA_KATALOG_SCHEMA_SCHREIBEN=1 cargo test -p proava-mcp --test tools the_tool_schemas_are_the_known_ones`.
