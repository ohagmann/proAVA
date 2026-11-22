# Synthetische GAEB-Testdateien

Alle Dateien hier sind für proAVA selbst geschrieben (frei verwendbar, keine Kunden- oder
Fremddaten). Sie bilden Besonderheiten nach, die in den offiziellen GAEB-/BVBS-Prüfdateien
vorkommen (Bezug siehe `docs/BUILD.md`); die Originale liegen nicht im Repo. Die XML-Dateien sind
gegen die offiziellen XSDs geprüft (3.3 2021-05, 3.2 2013-10, 3.1 2009-12).

| Datei | Inhalt |
|---|---|
| `syn1_33.X83` | LV „SYN-1“, DA XML 3.3: Hinweistext, Titel/Untertitel, Index A/B, Bedarfsposition mit und ohne GB, Grund-/Wahlposition, Pauschal- und Stundenlohnposition, Textergänzungen (Owner/Bidder), fett, Liste, Tabelle und PNG-Bild im Langtext, Zuschlagsposition `ListInSubQty` |
| `syn1_32.X83`, `syn1_31.X83` | dasselbe LV mit Namensraum 3.2 bzw. 3.1 (200407) |
| `syn1_33.X84` | Angebot dazu in der Kurzform der BVBS-Prüfdatei (nur OZ, EP, GP, Summen, Bieterergänzung); Summe 7.292,67 € |
| `syn1_33.D83`, `syn1_33.D84` | dasselbe LV und Angebot als GAEB 90 (ohne Zuschlagsposition, ohne Tabelle/Bild); erzeugt mit `scripts/syn_datan.py` |
| `syn2_lose_33.X83` | zwei Lose (OZ-Maske Los/Titel/Position) |
| `syn4_stlb_33.X83` | STLB-Bau-Kennungen (`STLBBau` mit Katalog, Version, LB, Gruppe, Kostengruppe und `STLBBauID`, `OutlTextPart`), Textergänzungen mit `ArtChrIdent` (Owner und Bidder), alte Standardleistungsnummer `StLNo Type="StLB"`, freie Pauschalposition; alle Schlüsselnummern ausgedacht, Texte nicht aus STLB-Bau |
| `syn5_stlb_te_33.X83` | Positionsübergabe wie aus STLB-Bau: Textergänzungen mit Ansprache (Leerzeichen am Ende), Wert in Hochkommas, Zahlenwerten `ComplBodyDec`/`ComplBodyInt`, Nachsatz mit Zeilenumbruch, TA und TB (Lücke mit 52 Punkten, ohne `Empty`, wie STLB-Bau Online schreibt), `ArtChrIdent` auch negativ, Kostengruppe; Schlüsselnummern ausgedacht, Texte nicht aus STLB-Bau |
| `syn7_stlb_online_33.X81` | Form einer GAEB-Datei aus STLB-Bau Online (f:data-Konverter): UTF-8 mit BOM, CRLF, eingerückt, ohne `PrjInfo`, nur Positionsebene mit `Num No`, Preisfelder in X81, Kataloge DIN 276-1 2008-12 vor DIN 276 2018-12 und Leistungsbereich mit `CtlgAssign` (keine `CostGrp` in der Kennung), `STLBBauID` mit `ArtChIdx` ohne `ChVIdent`, `ComplTSA`/`ComplTSB` mit `No`, Textergänzung Bieter mit Nachsatz nach `<br/>`, Ausschreibender mit leerem `<ComplTail />`, Lücke mit 52 Punkten, mehrere `span` je `Text`, Unterbeschreibung mit `SumDescr`, Katalogversionen 2022-10 und 2026-10; Nummern ausgedacht, Texte nicht aus STLB-Bau |
| `syn8_stlb_standardbeschreibung_33.X81` | Form des direkten GAEB-Exports „Positionsliste“ aus STLB-Bau Online: nur Positionsebene, Katalog Leistungsbereich, eine **Standardbeschreibung als `Item` mit leerer Einheit (`<QU></QU>`) und Menge 0**, eine gewöhnliche Position, dazu ein `Remark` und eine `PerfDescr` mit STLB-Bau-Kennung; Nummern ausgedacht, Texte nicht aus STLB-Bau |
| `syn6_herstellertext_32.X81` | Merkliste mit Herstellertexten wie aus einem Textportal (DA XML 3.2, Phase 81): Hinweistext, Zeilen „Hersteller:“/„Fabrikat:“/„Artikelnummer:“, „oder gleichwertig“, Bieterangabe „angebotenes Fabrikat/Typ“ (`Empty="Yes"`), Position ohne Menge; Hersteller und Typen ausgedacht, kein Text von AUSSCHREIBEN.DE |
| `syn3_zuschlag_wahltitel_33.X83/.X84` | Zuschläge `AllInCat` wie BVBS 3.3 AVA Titel 1.30.20 und Wahl-Titel (`ALNBGroupNo`/`ALNBSerNo`) wie 1.10.40; Angebotssumme laut Datei 2.290,00 € |

Benutzt von `tests/regression.rs`; Tests mit `#[ignore = "Befund …"]` beschreiben bekannte Lücken.
