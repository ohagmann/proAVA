# Fachliche Grundlagen für Entwickler

Normen und Regelwerke, auf die sich proAVA stützt, und die Besonderheiten der GAEB-Formate, die man zum Verstehen
des Codes braucht. Die Bedienung beschreibt das Handbuch (`docs/handbuch/`), den MCP-Server `docs/MCP.md`.

## Normen und Regelwerke

| Regelwerk | Wofür in proAVA |
|---|---|
| GAEB DA XML 3.3, Fachdokumentation (Ausgabe 2021-05 bzw. 2023-01) und XSD-Paket „Leistungsverzeichnis“ (2021-05) | Lesen und Schreiben der Phasen X81–X86, Nachträge (`CONo`, `COStatus`), Rechnung X89/X89B, Mengenermittlung X31; Positionsarten (Kap. 4.5), Nachlässe (4.6.7), Grafiken (4.6.8), Textergänzungen (4.6.16), Rundung (4.6.17), X84 (4.6.19), Regeln für X80–X86 (11.2.3) |
| GAEB DA XML 3.0–3.2 | nur lesen (Namensraum `…/200407` und neuer) |
| GAEB 90 (DATAN): „Regelungen für den Datenaustausch Leistungsverzeichnis“, Ausgabe 1990 | Satzaufbau, Feldlagen und Zeilenarten der Phasen 81–86 (`crates/proava-gaeb/src/datan/`) |
| VOB/A | Bedarfspositionen und Stundenlohnarbeiten (§ 7), rechnerische Prüfung der Angebote – der EP ist maßgebend (§ 16c), Wertung und Nachlässe (§ 16d) |
| VOB/B | Nachträge (§ 1 Abs. 3/4, § 2 Abs. 3/5/6/8, § 6), Abrechnung, Zahlung und Sicherheiten (§§ 14, 16, 17) |
| VHB Bund | Bedarfs- und Wahlpositionen (Richtlinie 100 Nr. 4.6), Aufgreifschwelle bei ungewöhnlich niedrigen Angeboten (Richtlinie 321) |
| DIN 276:2018-12 „Kosten im Bauwesen“ | Kostengruppen (Vorschlagsliste KG 410–499 in `cost_group.rs`), Stufen und Begriffe der Kostenermittlung (Kostenschätzung, -berechnung, Kostenvoranschlag, Kostenanschlag, Kostenfeststellung) |
| STLB-Bau (GAEB/DIN, Umsetzung Dynamische BauDaten) | Kennung `STLBBau` der Positionen, Leistungsbereiche (Vorschlagsliste 041–078 in `work_category.rs`) |
| BVBS-Prüfkriterien GAEB DA XML 3.3 (AVA, Bauausführung, Texterstellung) | Prüffälle für die eigenen Tests |
| § 13b UStG, § 48 EStG | Steuerschuldnerschaft bei Bauleistungen (Rechnung ohne USt), Hinweis Bauabzugssteuer |

Umgang: Normtexte, Schemas und Musterdateien sind nur gelesen; ihre Regeln stehen in eigenen Worten im Code. Im Repo
liegen keine XSDs (© DIN, Weitergabe nicht erlaubt) und keine fremden Musterdateien; die Testdateien in
`crates/proava-gaeb/tests/data` sind selbst geschrieben. Fremder Code steckt nur in den Bibliotheken
(`THIRD-PARTY-NOTICES`).

Bezugsquellen:

- GAEB-Datenaustausch (Fachdokumentationen, XSDs, Musterdateien, GAEB-XML-Checker):
  <https://www.gaeb.de/de/service/downloads/gaeb-datenaustausch/>
- GAEB 90, Regelwerk 1990 (Webarchiv):
  <http://web.archive.org/web/20090824100840/http://www.gaeb.de:80/download/da1990.pdf>
- BVBS-Zertifizierung (Prüfkriterien und Prüfdateien): <https://www.bvbs.de/zertifizierungen/>
- BVBS „Das freie GAEB Buch“: <https://bvbs.de/wp-content/uploads/2018/07/Das-Freie-GAEB-Buch.pdf>
- VOB/A 2019 Abschnitt 1:
  <https://bmwsb.bund.de/SharedDocs/downloads/DE/veroeffentlichungen/bauen/vob-teil-a-abschnitt-1-2019.pdf>
- VHB Bund 2017, Lesefassung 2023: <https://fib-bund.de/Inhalt/Vergabe/VHB/VHB_2017_Lesefassung_2023.pdf>
- STLB-Bau: <https://www.dbd.de/stlb-bau>, GAEB-FAQ STLB-Bau <https://gaeb.de/en/service/faq/stlb-bau>

## GAEB DA XML (`crates/proava-gaeb`, Modul `xml`)

- **Lesen** 3.0–3.3, Phasen X81–X86, unabhängig von Namensraum und Präfix; UTF-8 mit/ohne BOM, ISO-8859-1,
  Windows-1252. Unbekannte Elemente werden übergangen, Fehler kommen als `GaebXmlError` mit deutscher Meldung.
- **Schreiben** nur 3.3 (UTF-8, `ProgName` „proAVA“), je Phase nur, was das Schema dort erlaubt:
  - X81/X83: Mengen ohne Preise und Summen; X83 ohne `NotAppl` und `TimeQu` (gibt es dort nicht), bei `QtyTBD` ohne
    Menge; Textergänzungen ohne Inhalt bekommen einen Punkte-Platzhalter.
  - X82: EP, Preisanteile, Nachlass je Position, `IT` (fehlt es, aus Menge × EP), Summen je Titel und LV.
  - X84: nur OZ, EP, Anteile, GP, „nicht angeboten“, Bieterangaben, Summen und Bieteradresse (`CTR`).
  - X86: Auftrags-LV mit `OWN` und `CTR`; Nachtrags-LV mit `CONo`/`COStatus` an `BoQInfo` und Position.
- **Langtext** ↔ `RichText`: `p`, `span` mit Schriftschnitt (auch von `p` geerbt), `br`, Listen, Tabellen mit
  Spaltenbreiten, Bilder (`<image>`, Base64; nur mit `include_images`, nie in X84). Hart umbrochene Texte (ein `<p>`
  oder `<br/>` je Zeile, wie aus GAEB 90 umgesetzt) werden zu Absätzen zusammengeführt (`src/text_reflow.rs`), aber
  nur bei eindeutigem Bild: keine Formatierung, längste Zeile 30–80 Zeichen, mindestens vier Folgezeilen, die meisten
  fast so lang wie die längste.
- **Textergänzung im Satz:** Eine Textergänzung steht in `DetailTxt` zwischen zwei `Text`-Elementen; ein `<p>` kann
  diese Grenze nicht überspannen. Text ohne `<p>` setzt den Absatz davor fort (so schreiben STLB-Bau und andere
  Programme Felder im Satz). proAVA schreibt einen Absatz mit Feld im Satz deshalb als lose `span`s; Ausrichtung und
  Absatzgrenze trägt ein leeres `<span style="text-align:…"/>` am Absatzanfang (für andere Programme unsichtbar,
  schemagültig).
- **Summen:** Wahlpositionen und Wahlbereiche zählen nicht, eine Pauschalsumme (`TotalLSUM`) ersetzt die Titel- bzw.
  LV-Summe, Zuschläge `AllInCat`/`IdentAsMark`/`ListInSubQty` beziehen sich auf die Positionen seit dem letzten
  Zuschlagsblock bzw. die genannten Positionen (mit Teilmenge).
- **Normalisierung beim Lesen** (wie im GAEB-90-Codec): numerische OZ-Teile auf Maskenlänge aufgefüllt
  (`RNoPart="1"` → „01“), Index groß, Nachlass in Prozent, Textergänzungen getrennt vom Langtext mit Art
  „Owner“/„Bidder“, Punkt-Platzhalter = leerer Text. Eine X84 trägt keine Positionsarten und kein `QtyTBD`; eine
  Menge darin ist immer die des Bieters, Summen brauchen Arten und Mengen aus der X83.
- **Rundlauf:** `read(write(doc, p)) == as_written(doc, p)`; `as_written` bildet die gewollten Verkürzungen ab.
- **Grenzen:** Nicht gelesen bzw. geschrieben werden u. a. `WICNo`, `CtlgAssign` (außer Kostengruppe und
  Leistungsbereich), `QtySplit`, `Accepted`. Positionen liegen nur auf der untersten Titelstufe; tiefer stehende schreibt der Export ohne Zwischenstufe
  (GAEB: in einer Stufe nur Titel oder nur Positionen). Je Position nur ein Bezug (Wiederholung oder
  Ausführungsbeschreibung).

## GAEB 90 (`crates/proava-gaeb`, Modul `datan`)

- Sätze zu 80 Zeichen, Zeichensatz IBM-Codepage 850 oder Windows-1252 (Erkennung nach Plausibilität, nicht nach
  Bytezählung), DIN-66003-Ersetzungen; kein €.
- Langtext ohne Formatierung: Listen als „- “/„1. “ im Text, Zeilen zu 55 Zeichen, Unterpunkte als Einrückung.
- Höchstens 9 OZ-Stellen, keine Buchstabenstufen; „pauschal“ nur über die Einheit „psch“; Zuschläge nur an
  Prozentsatz/Bezugssumme (Satz 23) erkennbar.
- Keine STLB-Bau-Kennung, nur die alte 17-stellige Standardleistungsnummer (Satz 21 Stellen 44–61, Satz 24 für
  Unterbeschreibungen).

## STLB-Bau-Kennung

Jede STLB-Bau-Position trägt in GAEB DA XML ihre vollständige Kennung. Bleibt sie unverändert, kann jedes
STLB-Bau-fähige Programm die Position wieder öffnen, prüfen und aktualisieren. Aufbau laut XSD 3.3
(`tgDescription`, `tgSTLBBau…`, gleich von 3.1 bis 3.4):

```
Description
  ( STLBBau | StLNo )?          höchstens eins von beiden
  WICNo?
  ( CompleteText | OutlineText )
STLBBau
  STLBBauCtlg
    CtlgName   (max. 10)        z. B. „STLB-Bau“
    VersDate?  (gYearMonth)     Katalogversion, z. B. 2026-04
    WCtg?      (max. 3)         Leistungsbereich, z. B. 070
    Group      (Integer)        Leistung
    CostGrp?   (max. 20)        Kostengruppe
  STLBBauID*                    Merkmale, Reihenfolge beliebig
    ArtChrIdent, ArtChIdx?, ChVIdent?, OutlTextPart?
StLNo (Ziffern, max. 17), Attribut Type: StLB | StLK | STLB-BauZ
TextComplement … Attribut ArtChrIdent: Merkmal der Textergänzung
```

- Erlaubt in X81, X82, X83, X85, X86, nicht in X84; `ArtChrIdent` an Textergänzungen auch in X84. Auch an
  Unterbeschreibungen, Ausführungsbeschreibungen und Hinweistexten (Standardbeschreibungen).
- Textergänzungen werden zeichengleich durchgereicht: Ansprache (`ComplCaption`) und Nachsatz (`ComplTail`) samt
  Leerzeichen und Zeilenumbrüchen, Wert in Hochkommas, Zahlenwerte numerischer Merkmale als
  `ComplBodyDec`/`ComplBodyInt`, leere Lücken mit `Empty="Yes"` – sonst meldet der „STLB-Bau Check“ des GAEB
  Abweichungen.
- Kostengruppe und Leistungsbereich stehen in `CostGrp`/`WCtg` oder als `CtlgAssign` (DIN 276 2018 vor 2008).
- Der Langtext einer Position mit Kennung ist in proAVA schreibgeschützt (Fingerabdruck über den Langtext). Wird er
  dennoch geändert (ältere Dateien, Umwandlung), schreibt der Export die Position ohne `STLBBau` als freien Text und
  warnt. Der Kurztext ist nicht an den Katalog gebunden.
- Eine *Standardbeschreibung* liefert STLB-Bau als `Item` ohne Einheit und Menge; proAVA schlägt dafür „Hinweis“ vor.

## Fachregeln R1–R13 (`proava_gaeb::rules`)

Was die Fachdokumentation über das XSD hinaus verlangt (FD = Fachdokumentation GAEB DA XML 3.3). Schwere: F Fehler,
W Warnung, H Hinweis. Angewendet in der Exportprüfung („LV prüfen“, MCP `lv_pruefen`), nach jedem GAEB-Export und
beim Einlesen von Angeboten (R11).

| Nr. | Prüfung | Quelle | Schwere |
|---|---|---|---|
| R1 | Gliederung `BoQBkdn`: jede Stufe einmal, Länge ≤ 14, Los nur an erster Stelle, genau ein `Item`, höchstens 5 Stufen davor, Index einstellig nach `Item`, EP-Anteile passend | FD 11.2.3 BoQInfo, BoQBkdn | F |
| R2 | OZ-Teile gegen die Maske: Länge, Ziffern in numerischen Stufen, erlaubte Zeichen | FD 4.2.1 | F (H) |
| R3 | OZ eindeutig und aufsteigend | FD 4.2.1 | F/W |
| R4 | Gliederungstiefe: keine Titel tiefer als die Maske, Titel und Positionen nicht nebeneinander in einer Stufe (unbelegte untere Stufen sind erlaubt) | FD 4.2.2, 4.3.4 | F |
| R5 | Grund- und Wahlpositionen bzw. -bereiche gehören zusammen | FD 4.5.2 | F |
| R6 | kein Gesamtbetrag bei Wahlposition, Bedarf ohne GB, „entfällt“ | FD 4.5.3 | W |
| R7 | Zuschlagspositionen: Betrag = gerundete Bezugssumme × Prozentsatz, Bezüge vorhanden | FD 11.2.3 MarkupItem, 4.6.14 | F |
| R8 | Summen und Nachlässe nach der Rundungsregel; Titel- und LV-Summen gegen die ausgewiesenen Gesamtbeträge | FD 11.2.3 Totals, 4.6.7, 4.6.17 | F/H |
| R9 | Positionsregeln: Menge fehlt nur bei freier Menge oder Pauschale, Bezüge, Leitbeschreibung, Σ EP-Anteile = EP | FD 11.2.3 Item, 4.6.5, 4.6.11 | F |
| R10 | Beschreibungen: `ComplTSA`/`ComplTSB` zu Textergänzungen, Ausführungsbeschreibung vor ihren Positionen, Bieterangaben nur im Langtext | FD 11.2.3 Description, PerfDescr, 4.6.16 | F |
| R11 | X84 gegen X83: Gliederung, Textergänzungen, Menge nur bei freier Menge, Vergabenummer, Währung | FD 4.6.1, 4.6.4, 4.6.16, 4.6.19 | F |
| R12 | Dateikopf: Version, `VersDate`, `ProgSystem`; bei fremden Dateien Version zum Namensraum, Phase zur Endung | BVBS-Prüfkriterien | W |
| R13 | Eignung für die e-Vergabe: Position ohne Langtext, Menge 0, Punktfolgen ohne Textergänzung, Textergänzung in Hinweisen, Preise in 83; GAEB 90 Satznummern, Anfang und Ende | Meldungen üblicher e-Vergabe-Prüfer | H/W |

## Rechenregeln

- GP = Menge × EP, kaufmännisch auf Cent (`Money::times`); Titelsumme = Summe der gerundeten GP der Positionen mit
  Gesamtbetrag, jede höhere Stufe = Summe der gerundeten Summen darunter (FD 4.6.17). Nachlass: GB nach Nachlass
  erneut auf Cent, Nachlassbetrag = Differenz.
- Angebotsprüfung: GP ≠ gerundet(Menge × EP) wird korrigiert, der EP ist maßgebend (§ 16c VOB/A); bei Pauschalen die
  Pauschalsumme. Nachlässe zählen in der Wertung nur, wenn an der vorgesehenen Stelle angeboten (§ 16d Abs. 4 VOB/A).
- Rechnungsfreigabe (`proava_domain::billing_check`, je Schritt auf Cent): Leistung netto − Nachlass des Auftrags −
  Abzüge/Umlagen = Rechnung netto; − Sicherheitseinbehalt − anrechenbare Freigaben ± Korrektur = Freigabe netto;
  + USt (oder 0 % nach § 13b UStG) = Freigabe brutto; Skonto nur als Information. Kumulierte und
  Zuwachs-Rechnungen lassen sich mischen; die Schlussrechnung ist immer kumuliert.
- Nachträge: Status wie GAEB `COStatus`; nur beauftragte Nachtragsleistungen erhöhen die Auftragssumme und sind
  abrechenbar.
- Kostenermittlung nach Datenpunkten: Summe je Bauteil, Aufteilung auf KG 481–489 in Prozent; bei genau 100 % trägt
  die größte KG den Rundungsrest, damit die Summe der KG die Gesamtsumme ergibt.

## Texte von Dritten (STLB-Bau, AUSSCHREIBEN.DE)

proAVA nutzt nur offene, dokumentierte Wege:

- **GAEB-Dateien**, die STLB-Bau Online oder AUSSCHREIBEN.DE im Browser exportieren; der Übernahme-Dialog liest sie
  aus dem Download-Ordner bzw. aus dem Browserfenster `proava-web`.
- **Katalogverzeichnisse von AUSSCHREIBEN.DE**, die die Seite für KI-Assistenten veröffentlicht (`llms.txt`); vor
  jeder Suche gilt `robots.txt`. Suche, interne Schnittstelle und Merkliste der Web-Anwendung ruft proAVA nicht auf.
- Das **Browserfenster** zeigt die Seiten unverändert: kein Skript in der Seite, kein automatisches Klicken, Suchen,
  Ausfüllen oder Exportieren, keine gespeicherten Zugangsdaten außer den Cookies der Seite. Anmeldedienste fremder
  Hosts stehen in einer engen Liste (`Site::sign_in_hosts`).
- Die Nutzungsbedingungen von AUSSCHREIBEN.DE erlauben, die Texte für Ausschreibungen zu übernehmen und zu ändern,
  nicht aber, sie Dritten zu überlassen. Deshalb gehören keine dieser Texte ins Repo, in Tests oder Beispieldaten.
  Die Herkunft („Textquelle“) speichert proAVA nur in der Projektdatei, nicht in GAEB oder PDF.
