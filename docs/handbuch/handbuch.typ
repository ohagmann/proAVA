// proAVA – Handbuch
// Übersetzen: scripts/handbuch.sh (erzeugt zuerst tastenkuerzel.csv aus dem Programm,
// dann proava-handbuch.pdf). Bilder: bilder/*.jpg, nur synthetische Demo-Daten.

#let stand = "22.11.2026"
#let version = "0.2.0"

#set document(title: "proAVA – Handbuch", author: "Oliver Hagmann")
#set text(font: "Liberation Sans", size: 10pt, lang: "de", hyphenate: true)
#set par(justify: true, leading: 0.62em, spacing: 0.9em)
#set heading(numbering: "1.1")
#set page(
  paper: "a4",
  margin: (top: 24mm, bottom: 22mm, left: 22mm, right: 20mm),
  header: context {
    if counter(page).get().first() > 1 {
      set text(size: 8.5pt, fill: luma(90))
      grid(
        columns: (1fr, auto, 1fr),
        align(left)[proAVA], align(center)[Handbuch], align(right)[#stand],
      )
      v(-0.4em)
      line(length: 100%, stroke: 0.4pt + luma(170))
    }
  },
  footer: context {
    set text(size: 8.5pt, fill: luma(90))
    line(length: 100%, stroke: 0.4pt + luma(170))
    v(-0.4em)
    grid(
      columns: (1fr, 1fr),
      align(left)[Version #version],
      align(right)[Seite #counter(page).display() von #counter(page).final().first()],
    )
  },
)

#show heading.where(level: 1): it => {
  set text(size: 17pt, fill: rgb("#2563C9"))
  block(above: 2.2em, below: 1.1em, sticky: true, it)
}
#show heading.where(level: 2): it => {
  set text(size: 12.5pt)
  block(above: 1.4em, below: 0.7em, sticky: true, it)
}
#show heading.where(level: 3): it => {
  set text(size: 10.5pt)
  block(above: 1.1em, below: 0.6em, sticky: true, it)
}

// Bausteine
#let blau = rgb("#2563C9")
#let menu(..teile) = text(weight: "bold", teile.pos().join[ ▸ ])
#let taste(t) = box(
  inset: (x: 3pt, y: 1.5pt),
  outset: (y: 1.5pt),
  radius: 2pt,
  stroke: 0.5pt + luma(150),
  fill: luma(245),
  text(font: "Liberation Mono", size: 8.5pt, t),
)
#let hinweis(titel: "Hinweis", body) = block(
  width: 100%,
  inset: 8pt,
  radius: 3pt,
  fill: rgb("#EEF3FC"),
  stroke: (left: 2.5pt + blau),
  [#text(weight: "bold", fill: blau)[#titel:] #body],
)
#let bild(datei, breite: 100%, unterschrift) = figure(
  box(stroke: 0.5pt + luma(180), image("bilder/" + datei, width: breite)),
  caption: unterschrift,
)
#show figure.caption: set text(size: 8.5pt, fill: luma(70))
#show raw: set text(font: "Liberation Mono", size: 9pt)
#show table.cell.where(y: 0): set text(weight: "bold")
#set table(stroke: 0.4pt + luma(170), inset: 5pt)

// Gewährleistungsausschluss, englisch und deutsch – derselbe Wortlaut wie im
// Info-Fenster, im README und in den LIESMICH.txt (ein Test prüft das).
#let gewaehrleistung = block(width: 100%, inset: 9pt, radius: 4pt, stroke: 0.8pt + rgb("#b45309"))[
  #text(weight: "bold")[proAVA comes with ABSOLUTELY NO WARRANTY, to the extent permitted by
  applicable law. Use at your own risk; check all results (LV, quantities, prices, totals, GAEB
  files) before you rely on them.]

  proAVA wird ohne jegliche Gewährleistung bereitgestellt, soweit gesetzlich zulässig. Die Nutzung
  erfolgt auf eigenes Risiko; alle Ergebnisse (LV, Mengen, Preise, Summen, GAEB-Dateien) vor
  Verwendung prüfen. Die Haftung für Vorsatz und grobe Fahrlässigkeit bleibt unberührt.
]

// Titelseite
#page(header: none)[
  #v(40mm)
  #box(width: 22mm, height: 22mm, radius: 5mm, fill: blau,
    align(center + horizon, text(size: 34pt, fill: white, weight: "bold")[A]))
  #v(8mm)
  #text(size: 30pt, weight: "bold")[proAVA]
  #v(1mm)
  #text(size: 14pt, fill: luma(80))[Ausschreibung · Vergabe · Abrechnung]
  #v(14mm)
  #text(size: 20pt)[Handbuch]
  #v(3mm)
  #text(size: 11pt)[Leistungsverzeichnisse schreiben, GAEB austauschen, Angebote prüfen, \ Preisspiegel, Vergabe und Abrechnung – für Planer der Gebäudetechnik]
  #v(1fr)
  #text(size: 10pt, fill: luma(80))[Version #version · Stand #stand \ © 2026 Oliver Hagmann · Business Source License 1.1 (@lizenz)]
]

// Titelrückseite
#page(header: none)[
  #v(1fr)
  #gewaehrleistung
  #v(4mm)
  #text(size: 9pt, fill: luma(80))[© 2026 Oliver Hagmann. proAVA steht unter der Business Source
  License 1.1; verbindlich ist der englische Lizenztext (@lizenz). Namen und Normen Dritter:
  ebenda.]
]

#outline(title: [Inhalt], depth: 2, indent: 1.2em)
#pagebreak()

= Worum es geht

proAVA ist ein schlankes AVA-Programm für den eigenen Bedarf: ein Leistungsverzeichnis
schreiben, als GAEB an die Bieter geben, die Angebote einlesen, prüfen und im Preisspiegel
vergleichen, den Auftrag erteilen, alles als PDF drucken – und danach Rechnungen und Nachträge
prüfen. proAVA beschränkt sich bewusst auf diesen Kern und liest und schreibt dafür die
GAEB-Formate DA XML und GAEB 90.

Der Weg durch ein Projekt:

+ Projekt anlegen (leer, aus einer Vorlage, aus einer GAEB-Datei oder CSV-Tabelle),
  Projektstammdaten eintragen.
+ Gliederung und OZ-Maske festlegen, LV schreiben: Titel, Positionen, Langtexte, Bilder,
  Ausführungsbeschreibungen, Hinweise – auch aus der eigenen Bibliothek, aus STLB-Bau oder mit
  Herstellertexten.
+ LV prüfen und als *Angebotsaufforderung (X83)* an die Bieter geben, dazu das PDF „LV ohne
  Preise“.
+ Angebote (X84/D84) einlesen; proAVA prüft sie gegen das LV.
+ Preisspiegel auswerten, Auftrag erteilen, Auftrags-LV ausgeben, Preise als Planerpreise
  übernehmen.
+ Rechnungen prüfen und freigeben (Abrechnung), Nachträge prüfen und beauftragen.

Ein Projekt ist *eine Datei* mit der Endung `.proava` – ein Zip mit den Daten, den Bildern und
den abgelegten Schreiben. Sie liegt in einem eigenen Projektordner; Sicherungen und Ausgaben
kommen daneben.

== Das Fenster

#bild("lv-position.jpg")[Das Programmfenster: Kopfleiste mit Reitern, Menüs, links das Eigenschaftenfenster, rechts die LV-Tabelle]

*Kopfleiste* von links: der Projektname, dann #menu[Start] (Projekt schließen, zurück zur Startseite)
und die fünf Reiter *LV*, *Angebote*, *Preisspiegel*, *Abrechnung* und *Nachträge*
(#taste[Strg+1] bis #taste[Strg+5]; hinter Angebote und Nachträge steht ihre Anzahl). Rechts:
#menu[Befehl …] (Befehlssuche), das Zahnrad für die Einstellungen, Sonne/Mond für helles oder
dunkles Aussehen und #menu[Speichern].

*Schmale und niedrige Fenster:* Mit #taste[Super+←]/#taste[Super+→] legt der Fenstermanager
proAVA auf eine Bildschirmhälfte, mit #taste[Super+↑]/#taste[Super+↓] auf die obere oder untere
Hälfte (beides nacheinander: ein Viertel). Das Fenster lässt sich bis 640 × 420 Pixel verkleinern.
Der Projektname nutzt den Platz bis zu den Knöpfen rechts; passt er nicht ganz, steht „Befehl …“
nur als Symbol. Wird das Fenster schmaler, rücken die Reiter zusammen (ohne Anzahl, „Speichern“
nur als Symbol) und stehen dann mit #menu[Start] in einer eigenen Zeile unter dem Projektnamen.
Menüs ohne
Platz stehen unter #menu[Weitere] am Ende der Menüleiste. Das Eigenschaftenfenster wird schmaler
und lässt der Tabelle den größeren Teil; unter 740 Pixel Breite tritt es ganz zur Seite und kommt
zurück, sobald das Fenster wieder breiter ist (#taste[Alt+Enter] öffnet die Zeile im eigenen
Fenster).

*Menüs* in der üblichen Reihenfolge: #menu[Projekt], #menu[Bearbeiten], #menu[Einfügen],
#menu[LV], #menu[Prüfen], #menu[GAEB], #menu[Angebote], #menu[Kosten], #menu[Drucken],
#menu[Ansicht], #menu[Extras] und #menu[Hilfe]. Befehle mit „…“ öffnen einen Dialog; rechts neben
einem Befehl steht seine Taste.

#table(
  columns: (auto, 1fr),
  table.header[Menü][Inhalt],
  [#menu[Bearbeiten]], [Rückgängig, Wiederholen, Suchen, Suchen und Ersetzen, Bearbeitungsstatus,
    alle Titel zu- oder aufklappen],
  [#menu[Einfügen]], [Neue Position darunter oder davor, Titel, Untertitel, Hinweis darunter
    oder davor, Abstandshalter, Ausführungsbeschreibung; neue Position aus STLB-Bau oder
    AUSSCHREIBEN.DE, aus anderem Projekt oder GAEB – mit denselben Namen wie im Rechtsklick],
  [#menu[LV]], [LVs anlegen, wechseln, kopieren, verschieben, löschen; Gliederung und OZ-Maske,
    neu nummerieren; mit anderem Stand vergleichen; Bibliothek],
  [#menu[Prüfen]], [LV prüfen, Rechtschreibung, leere Mengen, offene Punkte, Kosten nach
    Kostengruppen, leere Hinweise umwandeln; STLB-Bau-Prüfdatei ausgeben und einlesen],
)

Die Gruppen entsprechen dem Rechtsklick in der LV-Tabelle (Einfügen, Bearbeiten, Status, Suchen,
Leistungsverzeichnis). Jedes Menü öffnet direkt unter der Menüleiste und passt bei 768 Pixel
Fensterhöhe ganz. Ist das Fenster noch niedriger, zeigt ein Menü unten einen Pfeil: Mauszeiger auf
den Pfeil oder Mausrad scrollt weiter, oben erscheint dann der Pfeil zurück; mit #taste[↑]/#taste[↓]
folgt die Liste dem gewählten Eintrag.

*Statuszeile* unten: links, was gewählt ist (Zeile, Rechnung, Nachtrag), dann laufende
Arbeiten und Meldungen – nach einem Export mit #menu[Öffnen] für die geschriebene Datei; rechts,
was in der Zwischenablage liegt, und ob das Projekt gespeichert ist. Ungespeicherte Änderungen
zeigt außerdem ein „\*“ hinter dem Projektnamen.

#hinweis(titel: "Befehlssuche")[#taste[Strg+K] öffnet eine Suche über alle Befehle mit ihren
Tastenkürzeln – oft schneller als jedes Menü.]

*Dateidialog* (Öffnen, Speichern, Ordner wählen, Logo, Import …) wie im Dateimanager: Ein Klick
*wählt* einen Ordner oder eine Datei, ein *Doppelklick* oder #taste[Enter] öffnet den Ordner bzw.
übernimmt die Datei. #taste[↑]/#taste[↓] bewegen die Auswahl, #taste[Rücktaste] oder #taste[Alt+↑]
gehen eine Ebene höher. Orte, Laufwerke, zuletzt benutzte Ordner links und die Teile des Pfads
oben springen mit einem Klick. Beim *Ordner wählen* übernimmt der Knopf unten den gewählten
Unterordner, ohne Wahl den gezeigten; er nennt ihn („Ordner „Pläne“ wählen“).

*Neuer Ordner:* Wo gespeichert oder ein Ordner gewählt wird (Speichern unter, Export, Ordner für
ein neues Projekt …), legt #menu[Neuer Ordner] neben #menu[Nach oben] – oder
#taste[Strg+Umschalt+N] bzw. ein Rechtsklick in die Liste – im gezeigten Ordner einen neuen an:
Oben in der Liste steht „Neuer Ordner“ zum Überschreiben, #taste[Enter] legt ihn an,
#taste[Esc] verwirft ihn. Einen vorhandenen Namen oder unerlaubte Zeichen meldet das Feld, ebenso
fehlende Schreibrechte. Danach ist der neue Ordner gewählt; Doppelklick oder #taste[Enter] öffnet
ihn, beim Ordner wählen übernimmt ihn der Knopf unten.

#hinweis(titel: "Hilfe")[#taste[F1] zeigt die Tastenkürzel der Ansicht, in der du gerade bist,
mit Suche. #menu[Hilfe][Handbuch (PDF)] öffnet dieses Handbuch.]

= Projekt anlegen und öffnen <einstieg>

== Startseite

#bild("starter.jpg")[Startseite: Befehle links, zuletzt verwendete Projekte, Vorschau rechts]

Die Startseite zeigt links die Befehle (#menu[Neues Projekt …], #menu[Projekt öffnen …],
#menu[Neues Projekt aus GAEB …], Sicherung anlegen und wiederherstellen), in der Mitte die
*zuletzt verwendeten* Projekte (mit Suche; Doppelklick öffnet) und rechts eine *Vorschau* des
gewählten Projekts: Bezeichnung, Bauherr, die LVs mit ihrem Netto nach Planerpreisen, Zahl der
Angebote, letzte Änderung und Speicherort. Lange Namen und Pfade sind gekürzt; der volle Pfad
steht im Tooltip. Oben rechts schaltest du hell/dunkel um, das Zahnrad öffnet die
Einstellungen.

Ein *Rechtsklick* auf ein Projekt bietet: Öffnen, Schreibgeschützt öffnen, Im Dateimanager
zeigen, Kopie anlegen … (das ganze Projekt mit Angeboten und Bildern unter neuem Namen in einem
eigenen Ordner) und Aus der Liste entfernen. Gibt es die Datei nicht mehr (verschoben,
umbenannt), steht in der Zeile „Datei fehlt“; #menu[Pfad suchen …] wählt die Datei am neuen
Ort, #menu[Entfernen] nimmt den Eintrag aus der Liste.

== Neues Projekt

#menu[Neues Projekt …] fragt nach Name, Ordner, *Vorlage* und *OZ-Maske*. proAVA legt im Ordner
einen Unterordner mit dem Projektnamen an und darin `Name.proava`. Mit der Vorlage „leer“ bekommt
das erste LV OZ-Maske, Schrittweite und USt aus den Einstellungen (#menu[Neue Projekte], ab Werk
`11.222.PPPPI` = Los/Abschnitt 2 Stellen, Titel 3 Stellen, Position 4 Stellen und Index,
Schrittweite 10, 19 %). Das Feld *OZ-Maske* zeigt, womit das Projekt beginnt – bei „leer“ die
Maske aus den Einstellungen, bei der GA-Vorlage die der Vorlage; eine Änderung gilt nur für
dieses Projekt. Eine eigene Vorlage bringt ihre Gliederung mit. Darunter steht, was die Vorlage
mitbringt; die zuletzt gewählte ist beim nächsten Mal vorgewählt.

#bild("neues-projekt.jpg", breite: 60%)[Neues Projekt aus der GA-Vorlage]

== Projektvorlagen

Eine Vorlage ist ein normales proAVA-Projekt. Ein neues Projekt übernimmt daraus die LVs mit
OZ-Maske und Schrittweite, die Titel mit Bezeichnung und Kostengruppe, Vorbemerkungen und
Hinweise, Ausführungsbeschreibungen, die Stammdaten-Vorgaben (Planer, Vergabeart,
Vertragsbedingungen mit Abzügen) und die Briefvorlagen; die Positionen nur, wenn du
#menu[Positionen der Vorlage übernehmen] anhakst. Angebote, Auftrag und Abrechnung kommen nie
mit, Positionen beginnen wieder „offen“.

- *GA nach DIN 276 (481–489)* liefert proAVA mit: LV 480 „Gebäudeautomation“ mit den Titeln
  481 Automationseinrichtungen, 482 Schaltschränke, 483 Management- und Bedieneinrichtungen,
  484 Kabel und Leitungen, 485 Netzwerke und 489 Sonstiges (je mit Kostengruppe), einer
  Vorbemerkung, einer allgemeinen Ausführungsbeschreibung und den üblichen Abzügen (0 %).
  Die Vorlage bringt ihre OZ-Maske `11.222.PPPPI` mit: Titel `01` Gebäudeautomation (Los bzw.
  Abschnitt), darunter die Kostengruppe als Titelnummer `01.481` bis `01.489`, Positionen
  vierstellig mit Index (`01.481.0010`); die Schrittweite kommt aus den Einstellungen. Im Dialog
  lässt sich die Maske ändern: eine andere Ebene mit drei Ziffern trägt ebenso die Kostengruppe
  (`11.222.33.PPPP` → `01.481`), ohne eine solche Ebene sind die Titel 01 bis 06 nummeriert
  (der Dialog weist darauf hin).
- *Eigene Vorlagen*: #menu[Projekt][Als Vorlage speichern …] legt das offene Projekt im
  Vorlagenordner ab (#menu[Extras][Einstellungen …] ▸ Neue Projekte, ab Werk `proAVA-Vorlagen`
  im Home-Ordner). Was nur zu diesem Projekt gehört, bleibt dabei weg: Bauherr, Auftraggeber,
  Baustelle, weitere Adressen, Bezeichnung, Vergabenummer und Termine. Gibt es den Namen schon,
  fragt der Dialog, ob die Vorlage ersetzt werden soll. Eine Vorlage lässt sich wie jedes
  Projekt öffnen und ändern.

== Öffnen, Doppelklick, Kommandozeile

#menu[Projekt öffnen …] wählt eine `.proava`. #menu[Neues Projekt aus GAEB …] legt aus einer
GAEB-Datei (@gaeb-import) oder CSV-Tabelle (@csv) ein neues Projekt an. Nach der Einrichtung mit
`install_linux.sh` öffnet auch ein *Doppelklick* im Dateimanager das Projekt; für GAEB-Dateien
und CSV-Tabellen gibt es „Öffnen mit proAVA“. Dasselbe geht auf der Kommandozeile:
`proava Projekt.proava`, `proava LV.X83` bzw. `proava Zulieferung.csv`;
`proava --schreibgeschuetzt Projekt.proava` öffnet nur zum Lesen und Kopieren.

*Als AppImage* (eine Datei, etwa für einen Laptop): `proava-<Version>-x86_64.AppImage`
ausführbar machen (Dateimanager: Eigenschaften ▸ Rechte ▸ „Als Programm ausführen“) und
starten. Unten im Startfenster trägt #menu[Ins Startmenü eintragen] proAVA ins Startmenü ein;
Doppelklick auf `.proava`-Dateien öffnet dann dieses AppImage. Liegt die Datei später woanders
oder kommt eine neue Version mit anderem Namen, bietet das Startfenster den Knopf wieder an. Wird
die Datei durch eine neue ersetzt, sagt proAVA „bitte neu starten“. Der MCP-Server läuft mit
`… .AppImage mcp`.

== Zwei Fenster und schreibgeschützt öffnen

#menu[Projekt][Neues Fenster] startet ein zweites proAVA, #menu[Projekt][Projekt
schreibgeschützt öffnen …] öffnet ein Projekt dort nur zum Lesen und Kopieren – etwa ein altes
Projekt als Vorlage. Zwischen den Fenstern lassen sich Zeilen, Titel und ganze LVs kopieren
(@kopieren), auch per Maus von einem Fenster ins andere. Ist ein Projekt schon in einem anderen Fenster (oder auf einem anderen Rechner)
geöffnet, fragt proAVA vor dem Öffnen – mit Name, Rechner und Uhrzeit („von kollege auf
laptop seit 19.11.2026 08:00“): *schreibgeschützt öffnen*, *trotzdem öffnen* oder abbrechen. Die
Sperre ist die Datei `.~lock.<Projekt>.proava#` neben dem Projekt. Bleibt sie nach einem Absturz
liegen und läuft das proAVA auf diesem Rechner nicht mehr, entfernt proAVA sie beim nächsten
Öffnen ohne Rückfrage; die KI-Werkzeuge beachten sie genauso.

== Mehrere LVs in einem Projekt <mehrere-lv>

Ein Projekt kann mehrere LVs haben, etwa je Los oder Gewerk (z. B. LV 480 und LV 484). Die
LV-Tabelle zeigt *alle LVs der Ausschreibung untereinander*. Jedes LV beginnt mit seiner
*Kopfzeile*: einem dunkelblauen Balken mit Nummer und Bezeichnung, der Zahl der Positionen und der
Nettosumme. Hat das Projekt nur ein LV, gibt es keine Kopfzeile und die Tabelle sieht aus wie
immer.

#bild("mehrere-lv.jpg")[Zwei LVs untereinander: das Ende von LV 480, die Kopfzeile von LV 484 und
seine Zeilen; die Schreibmarke steht in LV 484]

*Das LV der Schreibmarke* ist das LV, in dem du gerade arbeitest. Ein Klick in eine Zeile, die
Pfeiltasten über die Grenze zweier LVs oder ein Suchtreffer machen das LV zum LV der
Schreibmarke. Seine Kopfzeile hat links einen hellen Rand. Die Statusleiste nennt es vorn
(„LV 484 – Elektro · Position 01.01.0020“), die Summenzeile unter der Tabelle zeigt seine
Summen. Alles, was für *ein* LV gilt, nimmt dieses LV:
- Drucken, GAEB-Export und #menu[LV prüfen] (#menu[Alle LVs prüfen …] nimmt alle LVs der
  Ausschreibung)
- LV-Eigenschaften, Neu nummerieren und OZ-Maske
- #taste[F9] und #taste[F10]
- die Reiter Angebote, Preisspiegel, Abrechnung und Nachträge

*Kopfzeile:* Das Dreieck klappt das ganze LV zu oder auf. Ein Doppelklick öffnet die
LV-Eigenschaften (Nummer, Bezeichnung, OZ-Maske, Schrittweite, USt, Währung). Ein Rechtsklick zeigt
die Befehle für LVs, dieselben wie im Menü #menu[LV]:
- #menu[Neues LV …] und #menu[LV duplizieren (ohne Angebote)]
- #menu[Ganzes LV kopieren] und #menu[LV aus Zwischenablage einfügen …]
- verschieben und löschen

#menu[LV][Gehe zu LV] springt zur Kopfzeile eines LV.

#bild("lv-kopf-menue.jpg", breite: 46%)[Rechtsklick auf die Kopfzeile: die Befehle für das LV]

*Zwischen den LVs kopieren und ziehen:*
- Kopieren: #taste[Strg+C] in LV 480, Klick in eine Zeile von LV 484, #taste[Strg+V]. Es kommt
  alles mit: Langtext, Bilder, Bieterangaben, Kostengruppe, STLB-Bau-Kennung und Zuschläge mit
  ihren Bezügen. Fehlt im Ziel-LV eine verknüpfte Ausführungsbeschreibung, wird sie mit angelegt.
- Ziehen: Eine Zeile oder einen Titel mit der Maus in das andere LV ziehen *verschiebt* sie, mit
  gedrückter #taste[Strg] wird *kopiert*.
- In beiden Fällen gelten OZ-Maske und Nummerierung des Ziel-LV. #taste[Strg+Z] nimmt das Einfügen
  oder Verschieben in einem Schritt zurück. Hat das Ziel-LV schon Angebote oder einen Auftrag,
  fragt proAVA wie bei jeder Änderung daran nach.

Eine Mehrfachauswahl bleibt in einem LV: #taste[Umschalt]+Klick in ein anderes LV wählt bis zum
Ende des eigenen LV.

*Über alle LVs der Tabelle* gehen Suchen (#taste[Strg+F], #taste[F3]), Suchen und Ersetzen,
die Rechtschreibprüfung (#taste[F7]) und #menu[Bearbeiten][Alle Titel zuklappen]. Ein Treffer
in einem anderen LV macht es zum LV der Schreibmarke, die Meldung nennt es („Gefunden in LV 484 –
Elektro.“). #menu[Alle ersetzen] ersetzt in allen LVs als *ein* Rückgängig-Schritt und nennt je
LV die Anzahl.

*Angebote, Preisspiegel, Abrechnung, Nachträge* gelten je LV. Bei mehreren LVs steht über diesen
Reitern eine kleine Auswahl „LV:“. Sie zeigt das LV der Schreibmarke, eine andere Wahl macht
dieses LV zum LV der Schreibmarke.

#bild("lv-auswahl-reiter.jpg", breite: 70%)[Reiter Angebote: Auswahl des LV, jeweils mit
Positionen und Angeboten]

*Kostenermittlungen* (Kostenschätzung oder Kostenberechnung als LV) stehen nie zwischen den LVs der
Ausschreibung. Über #menu[LV][Gehe zu LV] geöffnet, steht ein solches LV allein in der Tabelle; seine Kopfzeile nennt die Stufe („Kostenberechnung · 12 Positionen“).
Suchen, Ersetzen und Rechtschreibung bleiben dann in diesem LV. Die LV-Auswahl über den Angeboten
listet nur LVs der Ausschreibung.

#hinweis[Rückgängig (#taste[Strg+Z]) gilt für das ganze Projekt: Der jüngste Schritt wird
zurückgenommen, auch wenn er in einem anderen LV oder in den Stammdaten war – proAVA
zeigt dieses LV dann zuerst.]

== Projektstammdaten

#menu[Projekt][Projektstammdaten …] sammelt alles, was auf Deckblatt, Kopfzeilen und in den
GAEB-Kopf gehört, in fünf Reitern:

- *Allgemein:* Projektbezeichnung, Kurzname/Projektnummer des Auftraggebers, Baumaßnahme.
- *Adressen:* Bauherr, Auftraggeber (Vergabestelle; im GAEB-Export der Auftraggeber), Aufsteller
  (Planer), Baustelle und weitere Beteiligte mit Rolle; jede Rolle erklärt, wo sie erscheint. Jede
  Anschrift hat unter *Weitere Angaben* auch Ansprechpartner, Land, Fax, USt-IdNr., Zusatz 2 und 3
  und ILN. Die Baustelle hat Kennnummer und Bezeichnung, der Auftraggeber Debitorennummer und Staat
  (EWR oder anderer, Kfz-Kennzeichen). Benachrichtigungsort, weitere Ausführungsorte und
  Bedarfsträger aus einer GAEB-Datei stehen dort zur Ansicht und gehen mit dem Export wieder hinaus.
- *Vergabe & Termine:* Vergabeart, Vergabenummer und Datenaustauschnummer, Abgabetermin und -ort, Bindefrist,
  Ausführungsbeginn und -ende, weitere Fristen.
- *Vertragsbedingungen & Abzüge:* Sicherheiten für Vertragserfüllung und Mängelansprüche,
  Skonto mit Frist, Abzüge in % (Bauwesenversicherung, Baustrom …), Zahlungsbedingungen.
- *Kopf-/Fußzeile:* eine eigene Kopf- und Fußzeile für die Ausdrucke dieses Projekts
  (@kopffuss).

Jedes Feld wird beim Tippen geprüft (Datum als TT.MM.JJJJ, auch Kurzformen wie „1.3.“, Ende
nicht vor Beginn, 0–100 %); #menu[Übernehmen] geht erst, wenn alles stimmt.

#bild("stammdaten.jpg", breite: 75%)[Projektstammdaten, Reiter Allgemein]

= Gliederung und OZ-Maske <gliederung>

#menu[LV][Gliederung und OZ-Maske …] legt fest, wie die Ordnungszahlen aufgebaut sind –
als Liste von Stufen statt als Maskentext:

- je Stufe eine Bezeichnung (Los, Titel, Untertitel …), 1–4 Stellen, Ziffern oder Buchstaben;
  „Los als erste Stufe“ auf Wunsch; die Pfeile verschieben eine Stufe nach oben oder unten,
- dann die *Positionsstellen* (1–4), ein *Index* (keiner oder 1 Stelle) und die
  *Schrittweite* (1, 10, 100 oder 1000).

Ein Beispiel zeigt sofort, wie eine OZ aussieht; die GAEB-Grenzen (höchstens 5 Stufen, 14
Stellen) prüft der Dialog mit. Ist das LV schon gefüllt, sagt der Dialog, was mit den
Nummern passiert, und lässt dich wählen (neu nummerieren oder die OZ lassen, wie sie sind).

#bild("oz-maske.jpg", breite: 68%)[Gliederung und Ordnungszahl]

== Nummerierung

Solange ein LV fortlaufend nummeriert ist, nummeriert proAVA nach jedem Einfügen,
Verschieben oder Löschen neu – je Titel, mit der Schrittweite (0010, 0020 …). Hat das LV
*eigene* OZ (aus GAEB übernommen oder von Hand geändert), bleiben sie stehen; eine neue Zeile
bekommt dann eine freie OZ zwischen den Nachbarn (0015 zwischen 0010 und 0020).

Ist dort nichts frei (0078 und 0079 stehen direkt hintereinander), fragt proAVA beim Einfügen
– neue Position, #taste[Strg+V], Duplizieren, Übernahme aus Bibliothek oder anderem Fenster:
„Einfügen und neu nummerieren?“. Die Liste zeigt alt → neu und die neue Zeile. Mit
#menu[Einfügen] (oder #taste[Enter]) kommt die Zeile an ihre Stelle, und die Positionen dieses
Titels zählen wieder in Schritten (0070, 0080, 0090 …); die Titelnummern, die übrigen Titel und
fertige Positionen bleiben. #taste[Strg+Z] nimmt beides in einem Schritt zurück. Bekäme dabei eine
fertige Position eine neue OZ, fügt proAVA nicht ein und nennt sie.

Das ganze LV oder einen Titel nummerierst du jederzeit mit #menu[LV][Neu nummerieren …]: ganzes
LV oder nur der gewählte Titel, nur Positionen oder auch Titel, fertige Positionen behalten auf
Wunsch ihre OZ; die Vorschau zeigt alt → neu.

Die OZ einer Zeile änderst du im Eigenschaftenfenster (Feld *Ordnungszahl*), direkt in der
Tabelle (Spalte OZ, #taste[F2]) oder mit #menu[Ordnungszahl ändern …] (#taste[Strg+Umschalt+O]).
proAVA prüft gegen die Maske und meldet doppelte Nummern.

= LV schreiben <lv>

Links steht das *Eigenschaftenfenster* der gewählten Zeile (über #menu[Ansicht] links,
rechts oder ausgeblendet, #taste[Strg+E] blendet es ein und aus), rechts die *Tabelle* mit OZ,
Status, Kurztext, KG, Menge, Einheit, EP und GP. Die Spalten *KG* (Kostengruppe, Vorgabe an)
und *LB* (Leistungsbereich, Vorgabe aus) schaltest du über #menu[Ansicht] oder mit einem
Rechtsklick auf den Spaltenkopf ein und aus; proAVA merkt sich die Wahl. Ein Wert, den die
Zeile von ihrem Titel erbt, steht *grau und kursiv*, der Tooltip nennt den Titel („KG 481
Automationseinrichtungen – vom Titel 01.481“); eigene Werte stehen schwarz.
#menu[Ansicht][Anteil der Titelsummen in %] (auch im Rechtsklick-Menü des Spaltenkopfs) zeigt in
jeder Titelzeile grau in der Spalte EP, wie viel Prozent der LV-Summe netto ihre Titelsumme
ausmacht (Vorgabe aus, proAVA merkt sich die Wahl).
Titel und Untertitel sind im hellen Schema gelb hinterlegt – Titel wie „Hellgelb 3“ einer
Tabellenkalkulation, Untertitel eine Stufe heller. Im dunklen Schema behalten sie den Hintergrund
der übrigen Zeilen, OZ und Titeltext stehen in hellem Gelb (Untertitel etwas blasser). Das gilt
ebenso im Preisspiegel und in der Abrechnung (abschaltbar: #menu[Extras][Einstellungen …] ▸
Darstellung ▸ „Titelzeilen farbig hinterlegen“). Titel lassen sich mit dem kleinen Dreieck auf- und zuklappen; die Fußzeile zeigt Anzahl,
Bearbeitungsstand und Netto/USt/Brutto.

== In der Tabelle arbeiten

Die Tabelle lässt sich wie eine Tabellenkalkulation bedienen:

- #taste[↑]/#taste[↓] wählt die Zeile, #taste[←]/#taste[→] die Zelle (auf Titeln: zu- und aufklappen).
- #taste[F2] oder #taste[Enter] bearbeitet die Zelle – oder einfach lostippen.
  OZ, Kurztext, KG, LB, Menge, Einheit (mit Vorschlägen) und EP sind direkt änderbar: Zeile
  wählen, dann ein Klick in die Zelle. Genauso bearbeitest du bei einem Hinweis die Überschrift
  und bei einer Ausführungsbeschreibung die Bezeichnung direkt in der Textspalte (mit der Maus;
  #taste[Enter] auf einer Hinweiszeile führt in den Hinweistext; GAEB überträgt
  bei Kurztexten 70, bei Bezeichnungen 55 Zeichen – darüber erscheint ein Hinweis). Ein Klick auf
  Status oder GP (GP = Menge × EP) öffnet nichts, ein Abstandshalter hat keine Zelle; die OZ
  änderst du mit #taste[F2] oder #taste[Strg+Umschalt+O], nicht per Klick. Unter KG und LB klappt die Liste mit Klartext auf
  (DIN 276 bzw. die Leistungsbereiche von STLB-Bau und die eigenen Namen im LV); Tippen
  filtert, #taste[↓]/#taste[↑] wählt, #taste[Enter] oder ein Klick übernimmt, leer = keine
  eigene. Ändert der neue Wert eines Titels fertige Positionen, fragt proAVA vorher.
- *Doppelklick* auf eine Zeile oder #taste[Alt+Enter] öffnet ihre Eigenschaften im eigenen
  Fenster (@posfenster).
- Beim Bearbeiten: #taste[Tab]/#taste[Umschalt+Tab] übernimmt und springt zur nächsten/vorigen
  Zelle – gesperrte Zellen wie die Einheit einer STLB-Bau-Position überspringt es, #taste[F2] auf
  einer gesperrten Zelle öffnet die nächste freie –, #taste[Enter] in die Zeile darunter, #taste[F9] oder #taste[Strg+Enter] übernimmt und
  legt gleich die nächste Position an (ebenso die anderen Tasten für neue Zeilen, unten),
  #taste[Esc] bricht ab.
- *Menge mit Rechenansatz:* In die Menge darf eine Rechnung, z. B. `3*12,5+4`.
- Zeilen ziehen: eine oder mehrere zusammenhängende Zeilen mit der Maus verschieben (Bezüge
  bleiben, die OZ passt sich an).
- *Jede Zeile bleibt einzeilig:* Ein zu langer Kurztext endet mit „…“, der ganze Text steht im
  Tooltip und beim Bearbeiten im Eingabefeld. Mehrzeilige Kurztexte aus GAEB erscheinen in einer
  Zeile; sie behalten ihre Zeilen, solange du sie nicht änderst. Blendest du das
  Eigenschaftenfenster aus (#menu[Ansicht][Eigenschaftenfenster anzeigen]) oder ziehst es
  schmaler, bekommt die Spalte Kurztext die frei werdende Breite. In einem schmalen Fenster
  behält der Kurztext mindestens etwa 140 px: Zuerst wird die OZ-Spalte so schmal wie die
  längste OZ, dann fallen LB, KG, Einheit, EP und GP nacheinander weg; die Menge bleibt. Ebenso kürzen OZ, Einheit und
  Beträge in zu schmalen Spalten mit „…“; das gilt auch für Preisspiegel, Abrechnung, Nachträge,
  Kostenermittlung, Bibliothek und Schriftverkehr.

Die wichtigsten Befehle (alle in @tasten). Neue Zeilen und die Übernahme aus STLB-Bau und
AUSSCHREIBEN.DE liegen auf Funktionstasten (@funktionstasten); sie wirken überall in der
LV-Ansicht: in der Tabelle, beim Bearbeiten einer Zelle (die Eingabe wird vorher übernommen), im
Eigenschaftenfenster und in den Fenstern „Eigenschaften“ und „Langtext“ – dort hinter deren
Zeile, das Fenster „Eigenschaften“ zeigt danach die neue. Die bisherigen Strg-Tasten gelten
weiter.

#table(
  columns: (1fr, auto),
  table.header[Was][Taste],
  [Neue Position darunter / davor], [#taste[F9] / #taste[Umschalt+F9] (auch #taste[Strg+Enter], #taste[Einfg] / #taste[Umschalt+Einfg])],
  [Neuer Titel / Untertitel], [#taste[F12] / #taste[Umschalt+F12] (auch #taste[Strg+T] / #taste[Strg+Umschalt+T])],
  [Neuer Hinweis darunter / davor], [#taste[F10] / #taste[Strg+F10] (auch #taste[Strg+H] / #taste[Strg+Umschalt+H])],
  [Neue Ausführungsbeschreibung], [#taste[F11] (auch #taste[Strg+Umschalt+A])],
  [Neue Position aus STLB-Bau / AUSSCHREIBEN.DE], [#taste[F5] / #taste[F6]],
  [Neuer Abstandshalter darunter (Leerzeile)], [#taste[Strg+L]],
  [Titel eine Stufe tiefer / höher], [#taste[Tab] / #taste[Umschalt+Tab]],
  [Zeile verschieben], [#taste[Alt+↑] / #taste[Alt+↓]],
  [Duplizieren / Löschen], [#taste[Strg+D] / #taste[Entf]],
  [Langtext im eigenen Fenster], [#taste[Umschalt+F2]],
  [Eigenschaften im eigenen Fenster], [#taste[Alt+Enter] oder Doppelklick],
  [Kopieren, Ausschneiden, Einfügen], [#taste[Strg+C], #taste[Strg+X], #taste[Strg+V]],
)

*Wohin eine neue Position kommt:* in den Titel der Schreibmarke, hinter die gewählte Zeile.
Positionen gehören in einem LV unter einen Titel der untersten Stufe – unter einem neuen
Titel einer höheren Stufe legt proAVA deshalb mit der ersten Position die Untertitel bis dorthin
an. Stehen im LV aber schon Positionen direkt unter Titeln dieser Stufe (etwa nach einem
GAEB-Import), kommt die neue einfach dazu, ohne neue Untertitel. In der Positionsbibliothek
legt eine neue Position nie Titel an (@bibliothek). Titel entstehen sonst nur über
#menu[Neuer Titel] und #menu[Neuer Untertitel].

Der *Rechtsklick* auf eine Zeile (oder #taste[Umschalt+F10]) zeigt alle Befehle zur Zeile,
geordnet nach Einfügen, Bearbeiten, Status, Suchen, Verknüpfen und Leistungsverzeichnis – auch in
einer Zelle, die gerade bearbeitet wird; steht dort unter der Maus ein rot unterstrichenes Wort,
kommen die Rechtschreibvorschläge darüber.

#bild("kontextmenue.jpg", breite: 70%)[Kontextmenü einer Position]

== Das Eigenschaftenfenster

Für eine Position: Status, Ordnungszahl, die Zeile zur *Ausführungsbeschreibung* (siehe
@ausfuehrungsbeschreibungen), Kurztext (mit Zeichenzähler, GAEB erlaubt 70 Zeichen), Menge,
Einheit, EP und GP (Menge und EP mit Tausenderpunkten wie der GP; getippt werden darf mit oder
ohne), *Positionsart*, die Schalter *Pauschal* und *Stundenlohn*, die *Kostengruppe*, unter
„Weitere Angaben“ seltene Felder, darunter der *Langtext*. Für einen Titel: Ordnungszahl, Kurztext, Stufe
(höher/tiefer), Kostengruppe, Summe und Bearbeitungsstand mit „Alle fertig/in Bearbeitung/offen“.
Sind mehrere Positionen gewählt, ändert es Status, Einheit, Positionsart, Pauschal,
Stundenlohn und Kostengruppe für alle auf einmal.

== Eigenschaften im eigenen Fenster <posfenster>

Ein *Doppelklick* auf eine Zeile, #taste[Alt+Enter] oder im Kontextmenü #menu[Bearbeiten][Eigenschaften
im eigenen Fenster …] öffnet die Zeile in einem eigenen Programmfenster, das sich auf einen anderen
Bildschirm ziehen lässt; Größe und Lage merkt sich proAVA. Eine Position hat vier Reiter:

- *Allgemein*: Ordnungszahl, die Zeile zur Ausführungsbeschreibung, Kurztext, Menge (auch mit
  Rechenansatz), Einheit, EP, GP und der Langtext mit Formatleiste.
- *Weiteres*: Positionsart (mit Wahlgruppe und den anderen Positionen der Gruppe), Kostengruppe,
  *Leistungsbereich* (Gewerk: STLB-Bau-Nummer wie 070 Gebäudeautomation aus der Liste oder ein
  eigener Name; am Titel die Vorgabe für alles darunter wie bei der Kostengruppe, die Position
  zeigt „Gilt vom Titel …“; geht in GAEB mit), Pauschal, Stundenlohn, STLB-Bau-Kennung und
  die „Weiteren Angaben“ (Wiederholung, Zeitansatz, EP-Anteile, Textergänzungen).
- *Mengenermittlung*: Zeilen mit Ort/Bezeichnung und Rechenansatz (z. B. `2*12,5+3`), Ergebnis je
  Zeile und Summe; #menu[Summe als Menge übernehmen] setzt die Menge der Position. Weicht die Menge
  später ab, meldet es die Prüfleiste. Es sind dieselben Zeilen wie beim Aufmaß der Abrechnung;
  liegt schon ein Aufmaß vor, steht es darunter je Rechnung. Im Druckdialog druckt das Häkchen
  *Mengenermittlung* die Zeilen mit Summe unter jeder Position.
- *Grafik*: die Bilder und PDF-Ausschnitte des Langtexts – einfügen, groß ansehen, Druckbreite,
  entfernen.

Rechts steht die *Prüfleiste* mit den Befunden der Zeile (fehlende Menge, Einheit oder
Kurztext, offener STLB-Platzhalter, leere Textergänzung des Auftraggebers, Grund- ohne
Wahlposition …); ein Klick führt zum passenden Reiter. Titel, Hinweise und
Ausführungsbeschreibungen zeigen nur ihre Felder. #menu[Vorige Position]/#menu[Nächste Position]
(#taste[Bild↑]/#taste[Bild↓]) blättert durch Zeilen derselben Art, die Tabelle folgt. Änderungen
wirken sofort, #taste[Strg+Z] nimmt sie zurück, fertige Positionen sind schreibgeschützt.
#taste[Esc], #menu[Schließen] und das Kreuz der Titelleiste (auch #taste[Alt+F4]) schließen; ein
Wert, der noch im Feld steht, wird dabei übernommen wie beim Wechsel der Zeile, ein abgelehnter
(rot markierter) verworfen. Minimieren und Maximieren gehen wie bei jedem Fenster; proAVA merkt
sich Lage und Größe des normalen Fensters. Solange eine Zeile im Fenster steht, zeigt das
Eigenschaftenfenster daneben nur einen Hinweis.

*Das Fenster folgt der Tabelle:* Einmal geöffnet, zeigt es immer die gewählte Zeile – nach einem
Klick, mit den Pfeiltasten, #taste[Bild↑]/#taste[Bild↓], #taste[Pos1]/#taste[Ende], bei einem
Suchtreffer (#taste[F3]), aus der Prüfliste und wenn eine KI Zeilen zeigt; bei mehreren gewählten
Zeilen die aktive. Die Tastatur bleibt in der Tabelle, du blätterst also einfach weiter; Lage,
Größe und der gewählte Reiter bleiben (eine Zeile ohne den Reiter zeigt Allgemein, die nächste
Position wieder den gewählten). Ein Feld, in dem du gerade schreibst, wird vor dem Wechsel
übernommen; eine ungültige Eingabe (Menge „abc“) wird wie beim Blättern nicht übernommen. Mit dem
Häkchen *Festhalten* oben rechts bleibt das Fenster bei seiner Zeile, etwa um zwei Positionen zu
vergleichen: die eine im Fenster, die andere im Eigenschaftenfenster. Festhalten gilt, bis du
das Häkchen wieder nimmst oder das Fenster schließt.

*Vor dem Hauptfenster:* Die Fenster „Eigenschaften“ und „Langtext“ gehören zum Hauptfenster wie
der Dialog eines Programms: Sie bleiben vor dem Hauptfenster – ein Klick in die Tabelle verdeckt sie
nicht –, aber nicht vor anderen Programmen; klickst du eines an oder wechselst mit #taste[Alt+Tab],
kommt es wie gewohnt nach vorn. Öffnet sich im Hauptfenster ein Dialog, tritt das Fenster dafür
zurück und kommt danach wieder. Abschalten:
#menu[Extras][Einstellungen …] ▸ Darstellung ▸ *Positions- und Langtextfenster vor dem
Hauptfenster halten*.

== Positionsarten

#table(
  columns: (auto, 1fr),
  table.header[Art][Bedeutung, Summe],
  [Normalposition], [zählt in die Summe],
  [Bedarfsposition mit GB], [Eventualposition mit Gesamtbetrag, zählt in die Summe],
  [Bedarfsposition ohne GB], [nur Einheitspreis, zählt nicht],
  [Grundposition], [Grundposition einer Wahlgruppe, zählt],
  [Wahlposition], [Alternative zu einer Grundposition, zählt nicht],
  [Zulageposition], [zählt in die Summe],
)

Dazu kommen die Schalter *Pauschal* und *Stundenlohn*. STLB-Bau-Platzhalter (Positionen, die
später aus STLB-Bau gefüllt werden, siehe @stlb) kommen aus GAEB-Dateien oder anderen
Projekten; von Hand geht es gleich über #menu[Einfügen][Neue Position aus STLB-Bau …]. Ein
Platzhalter zeigt sein Kennzeichen in der Tabelle; #menu[Platzhalter-Markierung entfernen] im
Rechtsklick (nur an Platzhaltern) macht ihn zu einer normalen Position. #menu[LV prüfen …] weist
darauf hin, dass Bedarfs- und Wahlpositionen nach VOB/A § 7 nur wenn nötig vorkommen sollen.

== Zuschlagspositionen <zuschlaege>

Eine *Zuschlagsposition* rechnet einen Prozentsatz auf andere Positionen, z. B. „5 % Zuschlag
Erschwernis“. Sie hat keine Menge und keine Einheit: In der Spalte Menge steht grau die
*Bezugssumme*, im EP der *Prozentsatz* („5 %“), im GP der Zuschlag (Bezugssumme × Prozentsatz).
Titel-, LV- und Angebotssummen, Preisspiegel und Kostengruppen enthalten ihn. Zulage- und
Zuschlagsposition sind zweierlei: eine Zulage ist eine normale Position mit Menge und EP.

Anlegen: Position wählen, im Fenster *Eigenschaften* (#taste[Alt+Enter]) Reiter *Weiteres* das
Kästchen *Zuschlagsposition (%)* setzen und die Art wählen:

#table(
  columns: (auto, 1fr),
  table.header[Art][Bezugssumme],
  [auf alle Positionen der Liste], [die Positionen davor bis zum Titel bzw. bis zum vorigen
    Zuschlag (GAEB `AllInCat`)],
  [auf die gekennzeichneten], [dieselben, aber nur die mit dem Kästchen *zu bezuschlagen*
    (Reiter Weiteres, GAEB `MarkupIt`, Art `IdentAsMark`)],
  [auf genannte Positionen], [die Positionen der Liste im Reiter *Allgemein*, je mit ganzer Menge
    oder einer Teilmenge (GAEB `ListInSubQty`)],
)

Im Reiter *Allgemein* zeigt eine Zuschlagsposition statt Menge und Einheit die Bezugssumme und
„%“; das EP-Feld heißt *Prozentsatz (%)*. Darunter stehen Art, Bezugssumme und Zuschlag, bei
„auf genannte Positionen“ die Liste der Positionen mit Feld *Teilmenge* (leer = ganze Menge) und
#menu[Entfernen]. Positionen kommen auf drei Wegen dazu:

- *Position suchen:* OZ oder einen Teil des Kurztexts tippen, den Treffer anklicken.
- *Bereich von … bis …:* zwei OZ eintragen, #menu[Bereich übernehmen] nimmt alle Positionen
  dazwischen (ohne Zuschläge); ein Titel als Ende bringt seinen ganzen Block.
- *Markierte Zeilen übernehmen:* Häkchen *Festhalten* im Fenster setzen, die Positionen in der
  Tabelle markieren (#taste[Umschalt+Pfeil] oder #taste[Strg]+Klick), dann den Knopf drücken.

Jede Änderung nimmt #taste[Strg+Z] zurück. Im Druck steht die Bezugssumme in der Mengenspalte und
„Zuschlag auf 01.0010, 01.0020 (10 m3)“ unter dem Kurztext; *LV prüfen* meldet eine
Zuschlagsposition nicht als „ohne Menge/Einheit“, aber einen Zuschlag „auf genannte Positionen“
ohne Bezug. In GAEB geht sie als `MarkupItem` hinaus (in X83 ohne Prozentsatz – den trägt der
Bieter ein); der Prozentsatz eines Angebots (X84) ist der Preis des Bieters. Kopieren, Einfügen
und Übernehmen behalten den Zuschlag, seine Bezüge zeigen auf die mitkopierten Positionen.

#hinweis[GAEB-90-Angebote (D84) haben für den Zuschlagsbetrag kein Feld; manche Programme lassen
ihn deshalb aus ihrer Angebotssumme. proAVA wertet den Zuschlag und sagt beim Einlesen, dass die
Summe der Datei ihn nicht enthält.]

*Pauschalsummen:* Ein Titel kann statt der Summe seiner Positionen eine *Pauschalsumme* haben
(Bearbeitungsfeld des Titels, *Pauschalsumme netto*; GAEB `TotalLSUM`). Die Tabelle zeigt
„pauschal“, der Druck „Pauschalsumme …“, die Titel und das LV darüber rechnen mit ihr; Untertitel
behalten ihre eigenen Summen. Eine Pauschalsumme für das ganze LV steht in
#menu[Eigenschaften: LV] und ersetzt die Summe der Positionen als LV-Summe.

== Langtext, Texteditor und Bilder

Der Langtext wird so geschrieben, wie er gedruckt wird: Schriftart und -größe,
#taste[Strg+B] fett, #taste[Strg+I] kursiv, #taste[Strg+U] unterstrichen. Die Werkzeugleiste
zeigt dafür die üblichen Symbole wie in einer Textverarbeitung (der Name erscheint, wenn die
Maus darauf steht): Ausrichtung links, zentriert, rechts und Blocksatz, Aufzählung und
Nummerierung, Tabelle (Zeilen und Spalten einfügen/löschen, Kopfzeile), Bild und Ausschnitt aus
PDF. *Bilder* ziehst du in den Text, fügst sie mit #taste[Strg+V] ein oder wählst sie über das
Bildsymbol (#menu[Bild einfügen …]); sie werden in die Projektdatei übernommen und mitgedruckt.
Tabellen und Bilder verschieben die Pfeile darunter, der Papierkorb entfernt sie.

*Unterpunkte:* In einer Aufzählung oder Nummerierung rückt #taste[Tab] den Listenpunkt (oder alle
markierten) eine Ebene ein, #taste[Umschalt+Tab] wieder aus – bis zu acht Ebenen. #taste[Enter] in
einem leeren Unterpunkt und #taste[Rücktaste] am Anfang rücken ebenfalls eine Ebene aus, erst auf der
obersten Ebene endet die Liste. Aufzählungszeichen sind auf allen Ebenen „•“, Nummern zählen je Ebene
als „1.“, „a)“, „i.“; so wird auch gedruckt. Listen mit Unterpunkten aus anderen Programmen kommen so
an, wie sie geliefert wurden: aus GAEB DA XML (Liste im Listenpunkt), aus der Zwischenablage (Word,
LibreOffice, Browser) und aus GAEB 90 (über die Einrückung, soweit die Datei sie hat). Der Export
schreibt sie in GAEB DA XML verschachtelt, in GAEB 90 und im CSV-Export eingerückt (zwei Leerzeichen
je Ebene). GAEB trägt nur „nummeriert“ oder „Aufzählung“, nicht die Art der Nummer.

*Ausschnitt aus PDF:* Für Pläne wie die Systemtopologie holt das Blattsymbol
(#menu[Ausschnitt aus PDF einfügen …], auch im Rechtsklickmenü, im Fenster des Langtexts und im
Reiter _Grafik_ des Positionsfensters) einen Teil einer PDF-Seite in den Text: PDF wählen
(oder auf den Langtext ziehen), Seite wählen, auf der Seitenvorschau mit der Maus ein Rechteck
aufziehen – etwa nur die Zeichnung ohne Rahmen und Plankopf –, Links/Oben/Breite/Höhe in mm
feinstellen, die Breite im Langtext festlegen und #menu[Einfügen]. Ins Projekt kommt nur die
gewählte Seite, dieselbe PDF bei mehreren Ausschnitten nur einmal. Im PDF-Ausdruck bleibt der
Ausschnitt *Vektorgrafik* – Linien und Texte sind bei jedem Zoom scharf. GAEB kennt nur
Pixelbilder: mit „Bilder mitgeben“ geht er als PNG mit 300 dpi in die Datei, die Exportprüfung
nennt die Pixel und warnt bei sehr großen Bildern.

Große Pläne kosten beim Anzeigen Rechenzeit. proAVA rechnet die Vorschau darum nur einmal
im Hintergrund und merkt sie sich. Unter dem Ausschnitt stehen zwei Schalter: *Vorschau
einfrieren* behält das fertige Bild und speichert es mit dem Projekt, *Vorschau ausblenden* zeigt
nur einen Platzhalter mit Name und Größe. Für alle Ausschnitte auf einmal:
#menu[Extras][Einstellungen …] ▸ Darstellung ▸ „Vorschau aller PDF-Ausschnitte ausblenden“ (auch
im Rechtsklickmenü eines Ausschnitts). Der Druck enthält die Ausschnitte immer.

Für lange Texte gibt es ein eigenes Fenster: #menu[In eigenem Fenster bearbeiten …] oder
#taste[Umschalt+F2]. Änderungen wirken sofort, #taste[Strg+Z] im Hauptfenster nimmt sie
zurück, #taste[Esc] oder das Kreuz der Titelleiste schließt. Wie das Fenster „Eigenschaften“ folgt es der Auswahl in der Tabelle
und hat ein Häkchen *Festhalten*; bei einem Titel oder Abstandshalter (ohne Langtext) bleibt es
bei seiner Zeile und sagt das oben. Es bleibt ebenso vor dem Hauptfenster (@posfenster).

#bild("langtext-fenster.jpg", breite: 80%)[Langtext im eigenen Fenster, mit Tabelle]

== Bieterangaben im Langtext <bieterangaben>

Was der Bieter eintragen soll – Fabrikat, Typ, Hersteller, angebotenes Produkt –, steht als
*Bieterangabe* direkt im Langtext (GAEB: Textergänzung „vom Bieter“). Schreibmarke an die Stelle
setzen und #taste[Strg+Umschalt+B] drücken: Im Dialog wählst du den Vortext aus der
Schnellauswahl (oder schreibst ihn frei), optional einen Nachtext, und bestätigst mit
#taste[Enter]. Schneller geht es mit dem Feldsymbol der Werkzeugleiste
(#menu[Bieterangabe einfügen …]): ein Klick auf „Fabrikat:“ setzt das Feld sofort an die
Schreibmarke. Das zweite Symbol daneben (#menu[Bieterangabe unter dem Langtext …]) hängt die
Angabe als eigenen Absatz unter den Text – so, wie sie in vielen Leistungsverzeichnissen am Ende
der Position steht. Beides gibt es auch im Rechtsklickmenü, im Fenster des Langtexts und im
Positionsfenster, ebenso für Hinweise.

#bild("bieterangaben.jpg", breite: 90%)[Bieterangaben (blau) und eine Angabe des Auftraggebers (grau) im Langtext]

Im Editor ist die Angabe ein *Feld*: Es lässt sich als Ganzes markieren, kopieren und mit
#taste[Entf] oder #taste[Rücktaste] entfernen, aber nicht versehentlich halb löschen;
#taste[Strg+Z] holt es zurück. Ein *Doppelklick* öffnet es zum Bearbeiten (Vortext, Nachtext,
Kennung, Art). Kopieren und Einfügen nimmt es als Feld mit – auch in eine andere Position, ins
Fenster des Langtexts, ins Positionsfenster und in ein zweites proAVA-Fenster; in ein anderes
Programm kommt es als Text („Fabrikat: …………“). Der Zusatz „(vom Bieter)“ steht nur im Editor
und unterscheidet die Bieterangabe von der Angabe des Auftraggebers; gedruckt und in den
CSV-Export kommt er nicht. Im Druck kennzeichnet die Punktlinie die Bieterangabe, wie in anderen
AVA-Programmen (52 Punkte, etwa 5 cm); sie wird nie mitten durchgebrochen – passt sie nicht
mehr in die Zeile, beginnt sie auf der nächsten. Folgt im Text direkt ein Wort (etwa „vom Bieter
einzutragen.“), setzt der Druck ein Leerzeichen davor; vor einem Satzzeichen nicht. Im
Kurztext-LV stehen die Bieterangaben unter dem Kurztext. In der GAEB-Datei steht die
Textergänzung an genau dieser Stelle; Bilder, die du davor einfügst, verschieben sie nicht. Füllt der Bieter sie in seiner X84 aus, zeigt der
Preisspiegel seine Angabe (Tooltip an der Position), das Auftrags-LV druckt sie an der Stelle
der Lücke.

Mit #taste[Strg+Umschalt+G] (oder #menu[Angabe des Auftraggebers einfügen …]) entsteht eine
*Angabe des Auftraggebers*: ein fester Wert mit Vortext, z. B. „Nennweite DN 50“
(GAEB: Textergänzung „vom Auftraggeber“, wie die Zahlenwerte von STLB-Bau).

Die Vortexte der Schnellauswahl pflegst du unter #menu[Extras][Einstellungen …] ▸ Darstellung ▸
„Bieterangaben im Langtext“, je Zeile einen (Vorgabe für die Gebäudeautomation: Fabrikat, Typ,
Hersteller, Erzeugnis, Angebotenes Produkt, Fabrikat/Typ, Software/Version). `LV prüfen` zählt
die Bieterangaben im LV; eine Bieterangabe ohne Vortext meldet die Prüfung (auch vor dem Export),
weil der Bieter dann nicht weiß, was er eintragen soll.

== Ausführungsbeschreibungen <ausfuehrungsbeschreibungen>

Gemeinsame Texte mehrerer Positionen stehen einmal als *Ausführungsbeschreibung*
(GAEB „PerfDescr“) im LV: #menu[Einfügen][Neue Ausführungsbeschreibung] (#taste[F11] oder #taste[Strg+Umschalt+A])
legt sie an – Nummer, Bezeichnung, Kurztext, Langtext. Sie heißt `A0001`, `A0002` …
(OZ-Spalte, Kennzeichen an der Position, Fenster, Verknüpfungsfeld; im Druck steht in der
OZ-Spalte `A0001` mit „Ausführungsbeschr.“ darunter, daneben die Bezeichnung) und zählt
automatisch nach der Lage im LV, auch nach Einfügen, Verschieben und Löschen. #menu[Von Hand …]
vergibt eine eigene Nummer: „7“, „0007“, „A0007“ und „AB 0007“ ergeben `A0007`; danach zählt
proAVA erst nach #menu[Neu nummerieren …] wieder selbst. In GAEB steht die Nummer
ohne Buchstaben und Nullen (`PerfNo` 1). Positionen verweisen über die Zeile unter der
Ordnungszahl (unten) oder #menu[Verknüpfen] im Kontextmenü darauf (dort auch für mehrere gewählte
Positionen auf einmal). Das Fenster der Beschreibung zeigt, welche Positionen sie verwenden.

Ob eine Position verknüpft ist, steht gleich unter ihrer Ordnungszahl – im Eigenschaftenfenster und im
Reiter „Allgemein“ des Fensters „Eigenschaften“: „Ausführung nach A0001 – Kabeltrassen“ mit
#menu[Anzeigen] (springt zur Beschreibung), #menu[Ändern …] (andere Beschreibung oder neue anlegen)
und #menu[Lösen]; ohne Verknüpfung steht dort „keine Ausführungsbeschreibung“ mit #menu[Verknüpfen …].
#taste[Strg+Z] nimmt jede Änderung zurück. Das große Langtextfenster nennt die Verknüpfung in seinem
Kopf, die LV-Tabelle zeigt `A0001` als Kennzeichen hinter dem Kurztext.

== Hinweise

Hinweistexte (ohne OZ, ohne Menge) erklären einen Abschnitt; sie stehen kursiv in der Tabelle,
in der Spalte Status steht „Hinweis“. In GAEB gehen sie als Hinweistext. #taste[F10] (oder #taste[Strg+H])
legt einen Hinweis hinter, #taste[Strg+F10] (oder #taste[Strg+Umschalt+H]) vor der gewählten
Zeile an.

Ein Hinweis *ohne Text* (auch nur Leerzeichen oder leere Absätze) gilt als Versehen wie eine
Position ohne Menge: In der Spalte Status steht „leer“, die Zeile ist orange, #menu[Prüfen][LV prüfen …]
und die Exportprüfung warnen, #menu[Prüfen][Offene Punkte …] listet ihn. Gedruckt wird er nicht.

Im Druck steht über einem Hinweis klein die Marke „Hinweis“. *Vorbemerkungen* (GAEB: vor dem
LV) stehen dagegen ohne Marke über die ganze Breite, ab der OZ-Spalte; ihre Überschrift sagt,
was sie sind (etwa „Allgemeine Vorbemerkungen“).

Eine *Schlussbemerkung* ist ein Text nach dem LV, etwa ein „Schlussblatt“, in das der Bieter
Arbeitskräfte und Nachunternehmer einträgt. Am Hinweis ankreuzen: „Schlussbemerkung (steht in
GAEB und im Druck nach dem LV)“. Der Druck setzt sie nach den Summen auf eine eigene Seite, GAEB
DA XML hinter das LV; beim Import aus anderen Programmen bleibt sie so am Ende.

== Abstandshalter

Wer Positionen beim Bearbeiten übersichtlich gliedern will, nimmt statt eines leeren Hinweises
einen *Abstandshalter*: eine Leerzeile ohne OZ, Text und Preis, die nirgends mitzählt. Er ist
*nur für die Bearbeitung* da und erscheint weder im Druck noch in GAEB oder im CSV-Export.
#taste[Strg+L] oder #menu[Einfügen][Neuer Abstandshalter darunter (Leerzeile)] (in der Menüleiste
und im Rechtsklick gleich) legt ihn unter der gewählten Zeile an.
In der Tabelle bleibt die Zeile leer; gewählt oder unter der Maus steht „Abstandshalter – nur für
die Bearbeitung“. Verschieben, Löschen, Kopieren (auch in ein anderes Fenster) und Rückgängig
gehen wie bei jeder Zeile.

Leere Hinweise aus einem GAEB-Import werden zu Abstandshaltern über #menu[Prüfen][Leere Hinweise
in Abstandshalter umwandeln], den Knopf im Fenster LV prüfen bzw. Offene Punkte oder einzeln
über das Kontextmenü #menu[Bearbeiten][In Abstandshalter umwandeln]. Danach stehen sie nicht
mehr im Druck – wer dort eine Leerzeile braucht, behält den Hinweis. Der Import selbst ändert
nichts.

== Zeilenart umwandeln <umwandeln>

Eine Zeile kann ihre Art wechseln: *Position*, *Hinweis (Text)* oder *Ausführungsbeschreibung* –
etwa wenn eine Standardbeschreibung aus STLB-Bau als Position ins LV kam, aber nur ein Text ist.
Dazu die Zeile wählen und

- im Rechtsklick #menu[Bearbeiten][Umwandeln in] die neue Art wählen,
- oder #menu[Bearbeiten][Umwandeln in] in der Menüleiste, die Befehlssuche (#taste[Strg+K],
  „umwandeln“),
- oder im Bearbeitungsbereich bzw. im Fenster Eigenschaften das Feld *Zeilenart*.

Kurztext (beim Hinweis die Überschrift, bei der Ausführungsbeschreibung die Bezeichnung),
Langtext mit Bildern, Bieterangaben, STLB-Bau-Kennung und Textquelle bleiben. Was die neue Art
nicht tragen kann, fällt weg – Ordnungszahl, Menge und Einheit, Einheitspreis, Kostengruppe,
Positionsart, Unterbeschreibungen, die Nummer einer Ausführungsbeschreibung. Eine Rückfrage nennt
es vorher; geht nichts verloren, wandelt proAVA ohne Rückfrage um. Positionen, die auf eine
umgewandelte Ausführungsbeschreibung verweisen, verlieren den Bezug (die Rückfrage nennt sie).

Aus einem Hinweis wird eine Position mit freier Ordnungszahl, Menge 0 und ohne Einheit – beides
danach eintragen. Positionen gehören unter einen Titel der untersten Stufe oder einer Stufe, auf
der im LV schon Positionen stehen (wie in der Bibliothek); ein Hinweis davor muss erst dorthin. Eine fertige (gesperrte) Position erst auf „offen“ oder „in Bearbeitung“
setzen. Ist die Angebotsaufforderung schon versandt oder haben Angebote Preise für die Position,
sagt die Rückfrage das wie beim Neu nummerieren. #taste[Strg+Z] nimmt das Umwandeln in einem
Schritt zurück.

== Kostengruppen

Titel und Positionen tragen eine *Kostengruppe nach DIN 276* (Feld im Eigenschaftenfenster,
mit #menu[Liste …] zum Auswählen). Eine Position ohne eigene Kostengruppe erbt die ihres
Titels. Die Spalte *KG* der Tabelle (Vorgabe an) schaltest du über #menu[Ansicht] oder einen
Rechtsklick auf den Spaltenkopf ein und aus.
#menu[Prüfen][Kosten nach Kostengruppen …] zeigt die Summen je Kostengruppe nach Planerpreisen und
für jedes gewertete Angebot; #menu[Drucken …] öffnet dafür den Druckdialog, #menu[Als Text
kopieren] legt die Tabelle in die Zwischenablage. In den LV-Ausdrucken steht die Kostengruppe
rechts neben dem Kurztext („KG 481“).

== Bearbeitungsstatus und Sperre

Jede Position hat einen Status: *offen*, *in Bearbeitung* oder *fertig*. Die Spalte *Status* der
Tabelle zeigt „in Bearb.“ bzw. „fertig“; offene Positionen bleiben dort leer.
#taste[Strg+Umschalt+F] schaltet fertig/offen um, #taste[Strg+Umschalt+E] setzt „in Bearbeitung“; an
einem Titel gilt der Status für alles darunter. *Fertige Positionen sind gesperrt* – gegen
Ändern, Löschen, Verschieben und neue OZ, auch beim Suchen und Ersetzen und über den
MCP-Server. Wer ändern will, setzt sie zuerst wieder auf „in Bearbeitung“.

Über #menu[Bearbeiten][Bearbeitungsstatus] filterst du die Tabelle (nur offene, nur in Bearbeitung,
alles außer fertigen, nur Positionen ohne Menge …). Positionen *ohne Menge* sind rot markiert
(abschaltbar in den Einstellungen), #taste[F4] springt zur nächsten.

== Suchen und Ersetzen

#taste[Strg+F] sucht im LV, #taste[F3] springt weiter. #taste[Strg+R] öffnet *Suchen und
Ersetzen* über Kurztexte, Langtexte (die Formatierung bleibt), Hinweise,
Ausführungsbeschreibungen und Einheiten, auf Wunsch nur in der Auswahl; fertige Positionen werden
übersprungen und gezählt. Stehen mehrere LVs in der Tabelle, gehen beide über alle diese LVs
(@mehrere-lv).

== Rechtschreibung

Unbekannte Wörter in Langtexten, Hinweisen und Kurztexten haben eine rote Wellenlinie;
ein Rechtsklick darauf zeigt Vorschläge, *Zum Wörterbuch hinzufügen* und *Ignorieren*.
#menu[Prüfen][Rechtschreibung prüfen …] (#taste[F7]) geht alle LVs der Tabelle durch: jedes unbekannte
Wort mit Anzahl und Fundstellen, überall ersetzen (ganze Wörter, die Formatierung bleibt,
fertige Positionen nicht), ins Wörterbuch, ignorieren oder zur Fundstelle springen.
Geprüft wird mit dem deutschen Hunspell-Wörterbuch des Systems (unter Linux Paket
„hunspell-de-de“) und einer mitgelieferten Liste von GA-Fachwörtern; Wörter nur aus
Großbuchstaben oder mit Ziffern (DDC, Pt1000) bleiben ungeprüft. Ein- und ausschalten
unter #menu[Extras][Einstellungen …] ▸ Darstellung.

== Offene Punkte

#menu[Prüfen][Offene Punkte …] (#taste[Strg+Umschalt+P]) listet, was noch fehlt: Positionen ohne
Menge, ohne Kurztext oder Einheit, ohne EP (wenn das LV bepreist wird), offene
STLB-Platzhalter, Positionen „in Bearbeitung“, leere Hinweise und GP-Abweichungen aus einem
CSV-Import. Ein Klick springt zur Zeile.

== Kopieren und Übernehmen <kopieren>

#taste[Strg+C] legt die gewählten Zeilen *vollständig* in die Zwischenablage: eine Position,
mehrere (#taste[Umschalt]+Klick für einen Bereich, #taste[Strg]+Klick für einzelne Zeilen, auch
aus verschiedenen Titeln, #taste[Strg+A] für alle) oder einen ganzen Titel mit Untertiteln,
Positionen und Hinweisen. Mit kommen formatierte Langtexte, Bilder, Positionsart, Kostengruppe,
Textergänzungen, STLB-Bau-Kennung und die verknüpften Ausführungsbeschreibungen.
#taste[Strg+V] fügt sie unter der gewählten Zeile ein – im selben LV, in einem anderen LV
(@mehrere-lv) oder in einem anderen proAVA-Fenster. Dort gelten Maske und Nummerierung des Ziel-LV, der Status
ist „offen“, #taste[Strg+Z] nimmt das Einfügen in einem Schritt zurück. Excel und Word
bekommen dieselben Zeilen als Tabelle (OZ, Kurztext, Menge, Einheit, EP). Was in der
Zwischenablage liegt („3 Positionen“, „Titel 02 Lüftung mit 14 Positionen“), steht rechts in
der Statuszeile.

*Zwischen zwei Fenstern ziehen:* Zeilen lassen sich auch mit der Maus aus einem proAVA-Fenster in
ein anderes ziehen – eine Zeile, mehrere markierte oder einen Titel, ebenso Einträge aus dem
Seitenfenster „Bibliothek“ oder dem Fenster „Übernehmen“. Greif sie und zieh sie über den
Fensterrand ins andere Fenster: Dort erscheint an der Zeile unter dem Zeiger die Einfügemarke
und ein Hinweis („1 Position aus Schule Musterstadt – loslassen fügt hinter 01.482 ein“), in der
Statuszeile des eigenen Fensters steht, wohin es geht. Am oberen oder unteren Rand der Tabelle
scrollt das andere Fenster mit. Loslassen fügt dort ein wie #taste[Strg+V] – mit Langtext,
Bildern, Bieterangaben, Ausführungsbeschreibungen, Kostengruppe und STLB-Bau-Kennung;
#taste[Strg+Z] im anderen Fenster nimmt es in einem Schritt zurück. Zwischen Fenstern wird immer
*kopiert*, das eigene LV bleibt unverändert. #taste[Esc] während des Ziehens oder Loslassen
außerhalb eines proAVA-Fensters bricht ab. Im anderen Fenster gilt, was auch von Hand gilt:
schreibgeschützt oder mit offenem Dialog nimmt es nichts an, ein versandtes LV fragt nach.
Unter Wayland meldet das System nicht, wo die Fenster liegen – dann erscheint am Rand der Hinweis
„Zwischen Fenstern ziehen: Strg+C/Strg+V“.

#bild("ziehen-fenster.jpg")[Links eine Position gegriffen, rechts die Einfügemarke unter dem Titel
01.482 und der Hinweis am Zeiger]

*Ein ganzes LV* kopierst du mit #menu[LV][Ganzes LV kopieren] oder im Kontextmenü seiner
Kopfzeile. #menu[LV aus Zwischenablage einfügen …] legt es als neues LV mit der nächsten freien
Nummer an – mit Maske, USt und Texten, ohne Angebote; proAVA fragt, ob die Planerpreise
mitkommen.

*Mit Vorschau:* #menu[Einfügen][Aus anderem Projekt / GAEB übernehmen …] zeigt die Gliederung einer
Quelle (anderes Projekt, GAEB-Datei, Bibliothek) in einem eigenen Fenster: Titel oder Positionen
anhaken oder in die Tabelle ziehen, wahlweise mit Mengen und Preisen. Die Quelle wird nur
gelesen.

*Suchen:* Das Feld #menu[Suche] findet Wörter in Kurztext, Langtext, Bieterangaben, Hinweisen
und Ausführungsbeschreibungen – alle Wörter müssen vorkommen, Groß/klein und Umlaute sind egal
(„reinweiss“ findet „reinweiß“), eine Zahl wie „01.02“ findet die OZ. Die Gliederung zeigt dann
nur die Treffer mit ihren Titeln (Titel ohne eigenen Treffer grau); #menu[Treffer ankreuzen]
hakt alle an. #menu[In allen Projekten unter … suchen] fragt nach einem Ordner und durchsucht
jedes proAVA-Projekt darin und in den Unterordnern (ohne „Alte Versionen“) im Hintergrund;
die Liste nennt je Projekt die Treffer, ein Klick lädt das Projekt als Quelle und zeigt den
Treffer mit Langtext. Nicht lesbare Dateien werden übersprungen und gezählt.

== Mit anderem Stand vergleichen

#menu[LV][Mit anderem Stand vergleichen …] vergleicht das aktive LV mit einem anderen LV des
Projekts, einem anderen Projekt oder einer GAEB-Datei (#menu[Datei wählen …]): neue, entfernte,
verschobene und geänderte Zeilen mit Wortunterschied in Kurz- und Langtext, Menge, Einheit und
Preis. #menu[Filter …] schränkt die Liste ein, ein Klick zeigt die Zeile in der Tabelle, auf
Wunsch sind die geänderten Zeilen dort markiert. #menu[Als Text kopieren] gibt den Vergleich als
Text aus, #menu[Drucken …] als PDF über den Druckdialog (Stil, A4 hoch oder quer).

= GAEB-Austausch <gaeb>

== Export und Prüfung

Das Menü #menu[GAEB] schreibt GAEB DA XML 3.3 (gegen die offiziellen Schemas geprüft):

#table(
  columns: (auto, 1fr),
  table.header[Befehl][Wofür],
  [#menu[Export Angebotsaufforderung (X83) …]], [an die Bieter – ohne Preise],
  [#menu[Export Bepreistes LV (X82, GAEB: Kostenanschlag) …]], [mit den Planerpreisen – der Kostenvoranschlag nach DIN 276; GAEB nennt diese Datenart Kostenanschlag],
  [#menu[Export Leistungsbeschreibung (X81) …]], [nur Texte und Mengen],
)

Im Dateidialog wählst du, ob die *Bilder* der Langtexte mitgehen (nur GAEB DA XML; die Wahl
merkt sich proAVA). Vor jedem Export prüft proAVA das LV; dieselbe Prüfung gibt es jederzeit
über #menu[Prüfen][LV prüfen …]. Gibt es Befunde – *Fehler* (doppelte oder fehlende OZ, leere
Kurztexte, Positionen ohne Menge oder Einheit, offene STLB-Platzhalter), *Warnungen* oder
*Hinweise* –, zeigt proAVA sie zuerst; ein Klick auf eine Zeile springt ins LV,
#menu[Trotzdem exportieren] schreibt die Datei dennoch.

#bild("lv-pruefen.jpg", breite: 70%)[LV prüfen]

#menu[Prüfen][Alle LVs prüfen …] prüft alle LVs der Ausschreibung auf einmal; die Befunde stehen je
LV unter seinem Namen, ein Klick auf eine Zeile macht dieses LV zum LV der Schreibmarke und zeigt
die Zeile. Kosten-LVs prüft es nur mit, wenn gerade ein Kosten-LV das LV der Schreibmarke ist – sie
gehen an keinen Bieter.

GAEB 90 (D8x) schreibt proAVA für das Auftrags-LV (D86) und über den MCP-Server (z. B. D83);
lesen kann es alle Phasen D81–D86.

*Nach dem Versand:* Mit dem X83-Export (oder der Aufforderung im Schriftverkehr) merkt sich
proAVA, was die Bieter bekommen haben. Verschiebt danach eine Änderung Ordnungszahlen –
Duplizieren, Einfügen mit Neunummerierung, Löschen, Verschieben, Neu nummerieren –, fragt proAVA:
„Das LV wurde am … versandt. Die OZ ab … ändern sich – das LV muss danach neu versendet werden.“
#menu[Trotzdem ändern] behält die Änderung (bis zum nächsten X83-Export ohne weitere Rückfrage),
#menu[Abbrechen] nimmt sie zurück. Jede Änderung an Texten, Mengen oder Positionen zeigt in der
Statuszeile „Seit dem Versand geändert – neu versenden“; ein Klick darauf öffnet die Änderungsliste
gegen die versandte X83 (#menu[LV][Mit anderem Stand vergleichen …]). Planerpreise und
Bearbeitungsstand zählen nicht. Der Hinweis verschwindet mit dem nächsten X83-Export.

*Mit Angeboten oder Auftrag:* Hat ein LV schon eingelesene Angebote oder einen Auftrag, fragt
proAVA auch ohne Versand aus proAVA, sobald Positionen hinzukommen oder solche wegfallen, die
ein Angebot oder der Auftrag enthält – beim Einfügen (#taste[Strg+Enter], #taste[Strg+V],
Duplizieren, #menu[Übernehmen], Bibliothek), beim Ziehen und Kopieren aus einem anderen LV hinein,
beim Löschen und wenn eine KI über die Fenster-Brücke einfügt; Verschieben aus einem LV mit
Angeboten fragt ebenso. #menu[Neue Position aus STLB-Bau …] bleibt für die nächste Position offen
und nennt den Grund deshalb nur in seiner Meldung (#taste[Strg+Z] nimmt die Position zurück). Der Text nennt den Grund, z. B. „Das LV hat bereits 4 Angebote –
die neue Position fehlt dort. Trotzdem einfügen?“; war das LV auch versandt, steht der Hinweis
auf die OZ in derselben Rückfrage. #menu[Trotzdem einfügen] (bzw. löschen, verschieben) behält
die Änderung – bis neue Angebote kommen oder sich der Auftrag ändert, fragt proAVA in diesem LV
nicht wieder –, #menu[Abbrechen] nimmt sie zurück. Mit Auftrag sagt die Rückfrage, dass
zusätzliche Leistungen in einen Nachtrag gehören; #menu[Als Nachtrag anlegen …] behält die neuen
Positionen an ihrer Stelle, legt einen Nachtrag (zusätzliche Leistung, heute angekündigt) mit
ihnen als neuen Positionen an und zeigt ihn im Reiter Nachträge – dort Bezeichnung, Begründung und
Preise eintragen (@nachtraege). Ein #taste[Strg+Z] nimmt den Nachtrag zurück, ein weiteres die
Positionen. Positionen, die du im Reiter Nachträge anlegst, sind Nachtragspositionen – dafür
fragt proAVA nicht.

== GAEB importieren <gaeb-import>

#menu[GAEB][Importieren …] holt ein LV aus einer GAEB-Datei – eigene Stände, Ausschreibungen
anderer Planer oder *Zulieferungen* wie Vortexte und Herstellertexte (oft ohne Mengen und
Preise). Angenommen werden X81–X86 (DA XML 3.0 bis 3.3), D81–D86 und eine `.zip`, die eine
solche Datei enthält; die Datei wird nur gelesen.

+ Datei wählen (oder den Pfad eintragen) und #menu[Lesen]. Die Vorschau zeigt je Zeile eine
  Angabe: Datei, Art (z. B. „X83 – Angebotsaufforderung“), Format („GAEB DA XML 3.2“), erstellt
  mit, LV, OZ-Maske, die Zahl der Titel, Positionen, Vorbemerkungen, Hinweise und
  Ausführungsbeschreibungen, die Summe netto, Positionen ohne Preis oder Menge und was beim
  Lesen auffiel.
+ Ziel wählen:
  - *Als neues LV in dieses Projekt* – LV-Nr. und Bezeichnung frei, vorgeschlagen aus der
    Datei; eine Nummer, die es schon gibt, wird abgelehnt. Ein leeres einzelnes LV (frisches
    Projekt) wird durch das importierte ersetzt. #taste[Strg+Z] nimmt den Import zurück.
  - *Als neues Projekt* – Name und Ordner frei (etwa um daraus Texte herauszukopieren), auf
    Wunsch gleich öffnen.
  - *In das aktive LV übernehmen* – die Datei wird Quelle im Übernahme-Fenster (@kopieren).
  - *In die eigene Bibliothek* – alle Zeilen in eine Bibliothek, unter einen neuen oder
    vorhandenen Titel (Dialog wie bei „In Bibliothek übernehmen“, @bibliothek-uebernehmen).
+ #menu[Preise übernehmen]: bei bepreisten Dateien (X82/D82) übernimmt proAVA die
  Einheitspreise als Planerpreise; ausgeschaltet bleiben die Preise leer. #menu[Importieren]
  liest ein.

Die *Ordnungszahlen der Datei bleiben erhalten* – auch Sprünge wie 02 → 10 –, ebenso
Langtext-Formatierung, Bilder, Positionsarten, STLB-Bau-Kennungen und Textergänzungen; alle
Positionen stehen auf „offen“. Ist die Datei ein Angebot (X84/D84), weist der Dialog darauf hin
und bietet #menu[Als Angebot einlesen …] an. #menu[Neues Projekt aus GAEB …] auf der Startseite
macht dasselbe direkt als neues Projekt.

Das *Importprotokoll* steht danach als Liste mit zwei Spalten da – links die Bezeichnung,
rechts der Wert, eine Angabe je Zeile: was gelesen wurde (wie in der Vorschau), *Übernommen als*,
der *Speicherort* eines neuen Projekts, Ordnungszahlen, Preise, Bearbeitungsstatus und
*Verworfen oder umgewandelt* mit den Einzelheiten darunter und ihrem Grund (z. B. Projektangaben
der Datei, die das Projekt nicht überschreiben, Bezüge auf Positionen, die es in der Datei nicht
gibt). Unplausible Projektangaben der Datei (z. B. ein Ausführungsende vor dem Beginn) übernimmt
proAVA wie geliefert und nennt sie hier und in *LV prüfen* als Hinweis. Der
Dialog wird so breit, dass der Speicherort in einer Zeile ganz zu lesen ist; reicht das Fenster
dafür nicht, ist der Pfad in der Mitte gekürzt und der Tooltip zeigt ihn ganz. #menu[Kopieren]
legt das Protokoll als Text in die Zwischenablage (je Zeile „Bezeichnung: Wert“). Nach demselben
Muster zeigt proAVA, was beim Einlesen von Angeboten gelesen wurde und welche Hinweise ein
Export oder das Einlesen eines Auftrags oder einer Rechnung hatte.

#bild("importprotokoll.jpg")[Importprotokoll: eine Angabe je Zeile, Speicherort ganz in einer Zeile]

= Angebote einlesen und prüfen <angebote>

#bild("angebote.jpg")[Reiter Angebote: Bieterverzeichnis, Angebotsliste und das gewählte Angebot]

== Bieterverzeichnis

Oben im Reiter #menu[Angebote] steht das *Bieterverzeichnis* des LV: wer angefragt wurde
(#menu[Bieter hinzufügen] oder #menu[Aus Projekt-Adressen …]), Unterlagen angefordert am,
Eingang des Angebots, Status (angefragt, Angebot abgegeben, kein Angebot, ausgeschlossen),
Nebenangebote, Absagegrund und Bemerkungen für die Niederschrift. Ein eingelesenes Angebot wird
dem Bieter gleichen Namens zugeordnet, sonst mit #menu[Angebot verknüpfen]. Von hier erreichst
du #menu[Niederschrift …] (Druckdialog mit der Niederschrift über die Angebotsöffnung),
#menu[Schriftverkehr …] (@schriftverkehr) und die abgelegten Schreiben.

Ein Klick wählt einen Bieter (die Zeile ist markiert wie in der LV-Tabelle), #taste[↑] und
#taste[↓] wechseln, *Doppelklick* oder #taste[Enter] öffnet ihn zum Bearbeiten. Die
Werkzeugzeile über der Liste hat #menu[Bearbeiten], #menu[Entfernen …], #menu[Nach oben] und
#menu[Nach unten] – ohne gewählten Bieter ausgegraut; dieselben Befehle stehen im
*Kontextmenü* (Rechtsklick), dazu #menu[Angebot lösen] und #menu[Abgelegte Schreiben …].
#menu[Entfernen …] und #taste[Entf] fragen vorher nach und nennen, was bleibt: ein verknüpftes
Angebot bleibt erhalten und wird nur gelöst (es ist ein eingegangenes Dokument, samt Wertung und
Auftrag – entfernen lässt es sich in der Angebotsliste), abgelegte Schreiben an den Bieter bleiben
im Schriftverkehr als Nachweis. #taste[Strg+Z] holt den Eintrag zurück.

== Angebote einlesen

#menu[Angebote][Angebote einlesen (X84/D84) …] liest Angebote ein – mit #taste[Strg]+Klick
mehrere Dateien auf einmal. proAVA ordnet die Preise über die OZ zu – robust gegen
fehlende führende Nullen oder einen fehlenden Index – und prüft jedes Angebot:

- fehlende Preise, unbekannte OZ aus der Datei, geänderte Mengen oder Einheiten,
- *Rechenfehler* (genannter GP ≠ Menge × EP; gewertet wird der Einheitspreis nach § 16c VOB/A),
- abweichende Summen, ein anderes LV als das ausgeschriebene,
- *Textergänzungen* der Bieter (Fabrikat, Typ …) an ihrer Stelle im Langtext.

Fehlt der Bietername in der Datei, nimmt proAVA den Namen des Ordners. Ohne Datei geht
#menu[Angebote][Angebot von Hand anlegen …] (auch als Knopf im Reiter); die Preise trägst du
dann im Preisspiegel ein.

Im Dialog „Angebote eingelesen“ legt der Haken #menu[Ins Bieterverzeichnis aufnehmen] (Vorgabe
an) die Firmen im Bieterverzeichnis an, mit „Angebot abgegeben“ und Eingangsdatum; einen
vorhandenen Eintrag mit gleichem Namen (auch mit anderer Rechtsform oder Schreibweise) oder
gleicher Anschrift ordnet proAVA zu, statt ihn doppelt anzulegen. Das brauchen z. B. Zuschlags-
und Absageschreiben.

*Angebotsmengen:* Die Mengen des LV beim Einlesen sind die Mengen, mit denen der Bieter gerechnet
hat. Ändert sich eine Menge danach im LV, ist das kein Rechenfehler des Bieters, sondern eine
*Massenmehrung oder -minderung*: Wertung, Rang, Auftragssumme und Auftrags-LV bleiben bei den
Angebotsmengen (Auftrag = Angebot), die Differenz gehört in einen Nachtrag (@preisspiegel).

Die Liste zeigt Rang, Bieter, Quelle, Datum, Netto, Nachlass, Netto nach Nachlass, Brutto,
Abstand zum Günstigsten und die Zahl der Befunde. Darunter stehen für das gewählte Angebot die
*Bieterangaben* (änderbar), die *Wertung* mit *Nachlass* (keiner, in % oder in €),
#menu[Vom Preisspiegel ausschließen], die *Prüfbefunde* und die Befehle der Vergabe (@vergabe).
Über der Liste stehen #menu[Auftrag erteilen …], #menu[Vom Preisspiegel ausschließen] und
#menu[Entfernen …] für das gewählte Angebot, im Kontextmenü (Rechtsklick) dazu Auftrags-LV und
Preisübernahme. #taste[↑]/#taste[↓] wechseln das Angebot, #taste[Entf] entfernt es nach einer
Rückfrage (sie nennt einen verknüpften Bieter, der im Bieterverzeichnis stehen bleibt);
#taste[Strg+Z] holt es zurück.

== Abweichungen im Angebot

Bieter geben oft auf eine Fassung ab, in der OZ umnummeriert, Titel umgestellt oder Mengen
geändert sind. proAVA vergleicht deshalb jede Angebotsdatei beim Einlesen mit dem LV und meldet
*jede* Abweichung – nicht erst ab einer Schwelle:

- Positionen der Datei, die nicht im LV stehen, und LV-Positionen, die in der Datei fehlen,
- OZ, deren Inhalt nicht passt (die Menge der Datei – angegeben oder GP ÷ EP – oder ihr Text
  widerspricht der LV-Position),
- abweichende Menge, Einheit, Kurztext (verglichen ohne Leerzeichen-, Groß/Klein- und
  Zeichenunterschiede), deutlich geänderter Langtext, geänderte Positionsart (Bedarf, Wahl,
  pauschal),
- Titel, die nur auf einer Seite stehen, anders heißen oder in anderer Reihenfolge stehen,
- eine Angebotssumme der Datei, die von der gewerteten abweicht – mit Betrag und Aufschlüsselung
  (nicht zugeordnete Positionen, Rechenfehler, Mengenabweichungen, Rest).

Das *Abweichungsprotokoll* bleibt am Angebot. Im Reiter Angebote steht es farbig über den
Prüfbefunden („Strukturabweichungen: 6 umnummeriert, 2 Mengen …“ mit der Liste), im Preisspiegel-Druck
kurz unter „Hinweise zu den Angeboten“, bei vielen Einträgen als Einzelliste im Anhang. Gewertet
wird immer mit Menge und Text des LV (Menge LV × EP des Bieters); abweichende Bietermengen meldet
proAVA, übernimmt sie aber nicht.

*Zuordnen:* Die meisten Dateien enthalten nur OZ, EP und GP. Für Positionen ohne passende OZ
sucht proAVA zwischen den sicher zugeordneten Nachbarn die LV-Positionen ohne Preis und gleicht
sie der Reihe nach ab – Menge, Einheit und, wenn die Datei Texte hat, Kurz- und Langtext
bestätigen es. Stehen an einer Stelle gleich viele Positionen auf beiden Seiten und widerspricht
nichts, heißt der Vorschlag *sicher*, sonst *prüfen*. #menu[Positionen zuordnen …] im Hinweis
öffnet den Dialog *Angebotspositionen zuordnen*: Angebots-OZ mit den Angaben der Datei,
vorgeschlagene LV-Position (änderbar), Ähnlichkeit, Häkchen. #menu[Sichere übernehmen] nimmt alle
sicheren Vorschläge, #menu[Übernehmen] die angehakten; das Protokoll vermerkt jede Zuordnung,
#taste[Strg+Z] nimmt sie zurück. Über den MCP-Server liest `angebot_einlesen` mit
`zuordnung: text` die sicheren Vorschläge gleich mit ein, `zuordnungen` ordnet von Hand zu.

= Preisspiegel <preisspiegel>

#bild("preisspiegel.jpg")[Preisspiegel mit Planerpreisen und vier Bietern]

Der Preisspiegel stellt alle gewerteten Angebote je Position und Titel nebeneinander, mit
feststehendem Kopf, feststehenden linken Spalten und feststehender Summenzeile:

- Anzeige *EP + GP*, *nur EP* oder *nur GP*; *Planerpreise* als eigene Spalte,
- *Abweichung* zum Mittelwert, zum Günstigsten oder zum Planerpreis; *Ausreißer* ab einstellbarer Grenze
  (Standard 20 %) mit „hoch“ oder „tief“ in der Zelle markiert, günstigster Preis je Zeile grün;
  „RF“ kennzeichnet einen Rechenfehler im Angebot, „prüfen“ in der Summenzeile fehlende Preise
  oder Rechenfehler (Einzelheiten im Tooltip),
- *Bedarf in Wertung* (Bedarfspositionen mitrechnen), *nur Titelsummen*, *nur Ausreißer*,
  Bieter ein- und ausblenden, alle Titel auf/zu, Suche nach OZ oder Kurztext, *Bieter anonym*,
- Summen: Netto, Nachlass, Netto nach Nachlass, USt, Brutto, *Rang* und Abstand zum
  Günstigsten; fehlende Preise trägst du per Doppelklick von Hand ein.

Liegt das günstigste Angebot deutlich unter dem nächsten, meldet proAVA die
*Aufgreifschwelle* (Standard 10 %, § 16d Abs. 1 VOB/A): Das Angebot ist dann auf
Auskömmlichkeit zu prüfen.

Wurde eine LV-Menge nach dem Angebotseingang geändert, steht über der Tabelle „Achtung
Massenmehrung nach Angebotseingang: … LV-Menge 12 statt 10 St (+2)“, die Menge ist markiert, und am
Kennzeichen „Menge“ unter den Summen steht die Summe mit der aktuellen LV-Menge – nur zur
Information, der Rang bleibt bei den Angebotsmengen; der Druck zeigt beides ebenso. Mit
beauftragtem Angebot legt #menu[Als Mehr-/Mindermenge in Nachtrag übernehmen …] einen Nachtrag
(Mengenänderung, § 2 Abs. 3 VOB/B) mit der Differenz je Position zum Auftrags-EP an (@nachtraege).
„LV prüfen“ und die Offenen Punkte nennen dieselben Positionen.

#menu[Preisspiegel drucken …] öffnet den Druckdialog (@drucken); gedruckt wird, wie die Ansicht
eingestellt ist (A4 oder A3 quer je nach Bieterzahl, auf Wunsch anonym). Je Bieter stehen EP,
GP und die Spalte *%* (Abstand zum günstigsten Preis der Zeile, auch bei Titelsummen und der
Angebotssumme), dazu Mittelwert, Minimum und – mit *Planerpreise* – der Planerpreis. Ein *?*
hinter dem EP kennzeichnet einen Ausreißer, ein *!* hinter dem GP einen Rechenfehler des
Bieters. Ist in den Projektstammdaten ein Skonto vereinbart, zeigt die letzte Zeile den
Skontobetrag je Bieter – nur zur Information, er geht nicht in den Rang ein.
Die erste Seite ist ruhig gehalten: oben die Angaben in einem kleinen Kasten, links die
*Übersicht der Angebote* (Rang, Bieter, Angebotsdatum, netto – nach Nachlass, wie gewertet – und
brutto), rechts daneben die *Legende* der Kennzeichnungen, darunter beginnt die Tabelle. Der
Preisspiegel druckt ab Werk als Gitter-Tabelle (alle Zellen umrandet, Druckoption *Gitter*,
@drucken).

#bild("preisspiegel-druck.jpg")[Preisspiegel im Druck: Angaben, Übersicht der Angebote, Legende, Gitter-Tabelle]

Der Kurztext behält im Druck mindestens 45 mm Breite und steht nie unter Menge und Einheit: Bei
vielen Bietern wird die Schrift kleiner (bis 6 pt), reicht auch das nicht, verteilt proAVA die
Bieter auf mehrere Tabellen (Teil 1, Teil 2 …), Mittelwert, Minimum und Planerpreis stehen im
letzten Teil; ab vier Bietern druckt es A3. Hat das LV keine Planerpreise, entfallen die Spalte
Planerpreis in Ansicht und Druck und ihre Erklärung. Enthält ein Angebot Positionen, die nicht im
LV stehen, oder Rechenfehler, nennt der Druck sie unter den Angebotssummen („Hinweise zu den
Angeboten“) – nicht gewertet bzw. gewertet mit Menge × EP.

= Vergabe abschließen <vergabe>

Alle drei Schritte stehen im Menü #menu[Angebote] und bei jedem Angebot im Reiter Angebote.

== Auftrag erteilen

#menu[Auftrag erteilen …] vergibt den Auftrag an das gewählte Angebot mit Datum. Die
Auftragssumme ist die Angebotssumme netto nach Nachlass. Je LV gibt es einen Auftrag: Ein
Auftrag an ein anderes Angebot ersetzt den bisherigen.

#menu[Angebote][Auftrag zurücknehmen …] nimmt einen erteilten Auftrag ganz zurück; die Rückfrage
nennt Bieter, Datum und Summe. Ist der Auftrag in der Abrechnung schon festgeschrieben, wird die
Festschreibung mit zurückgenommen (die Rückfrage warnt). Stehen auf ihm schon Rechnungen oder
Nachträge, geht es nicht – erst diese löschen. Das Angebot bleibt im Preisspiegel, ein abgelegtes
Zuschlagsschreiben in der Ablage; #taste[Strg+Z] holt Auftrag und Festschreibung in einem Schritt
zurück.

#bild("auftrag.jpg", breite: 55%)[Auftrag erteilen]

== Auftrags-LV

#menu[Auftrags-LV exportieren (X86/D86/PDF) …] schreibt das LV des beauftragten Bieters mit
seinen Preisen, dem Nachlass, seinen Angaben (Textergänzungen), Auftraggeber und
Auftragnehmer, Vergabenummer und Ausführungszeit – als GAEB DA XML X86 oder GAEB 90 D86. Für
das PDF öffnet sich der Druckdialog mit der Dokumentart *Auftrags-LV*.

== Preise übernehmen

#menu[Preise als Planerpreise übernehmen …] schreibt den Kostenvoranschlag mit den Ergebnissen
der Vergabe fort: Quelle ist ein Angebot oder der *Mittelwert*, *Median* oder das *Minimum*
der gewerteten Angebote; vorhandene Planerpreise werden nur auf Wunsch überschrieben.
#taste[Strg+Z] im LV nimmt es zurück.

= Drucken <drucken>

Alle Ausdrucke entstehen als PDF im *Druckdialog* (#menu[Drucken][Drucken …], #taste[Strg+P]).
Jeder Eintrag im Menü #menu[Drucken] – und ebenso #menu[Preisspiegel drucken …],
#menu[Niederschrift …], #menu[Drucken …] im Fenster der Kostengruppen und die PDF-Knöpfe der
Abrechnung – öffnet ihn mit der passenden Dokumentart; #taste[Enter] speichert das PDF sofort.
Die Optionen merkt sich proAVA je Dokumentart.

#bild("druckdialog.jpg", breite: 85%)[Druckdialog: Dokumentart mit Kurzbeschreibung links, Stil und Inhalt rechts]

#table(
  columns: (auto, 1fr),
  table.header[Dokumentart][Wofür],
  [LV ohne Preise (Angebotsaufforderung)], [für die Bieter: EP und GP leer zum Ausfüllen, Deckblatt mit Bieterfeld],
  [LV mit Preisen (Kostenvoranschlag)], [mit deinen Planerpreisen und Summen je Titel – der Kostenvoranschlag nach DIN 276, 4.3.5],
  [Kurztext-LV], [nur Kurztexte, mit oder ohne Preise],
  [Zusammenstellung (Summenblatt)], [nur die Titel mit ihren Summen],
  [Auftrags-LV], [das LV mit den Preisen des beauftragten Bieters],
  [Preisspiegel], [alle Angebote nebeneinander, wie im Reiter eingestellt],
  [LV mit den Preisen eines Bieters], [das LV mit den Preisen eines gewählten Angebots],
  [Niederschrift über die Angebotsöffnung], [Bieter, Eingang, Summen mit Nachlass, Nebenangebote, Unterschriften],
  [Kosten nach Kostengruppen], [Planer- und Bietersummen nach DIN 276],
  [Aufmaßprüfung einer Rechnung], [Rechnung gegen geprüfte Mengen je Position (@abrechnung)],
  [Rechnungsfreigabe], [Rechenweg bis zum Freigabebetrag, mit Unterschriften (@abrechnung)],
  [Kostenschätzung / Kostenberechnung (DIN 276)], [die Datenpunktrechnung der Gebäudeautomation (@kosten)],
  [Kostenermittlung (Kosten-LV, DIN 276)], [das aktive Kosten-LV mit Zusammenstellung nach Kostengruppen (@kosten)],
  [Kostenverfolgung (DIN 276)], [die Kette von der Kostenvorgabe bis zur Kostenfeststellung je Kostengruppe, wie im Reiter eingestellt (@kostenverfolgung)],
)

Arten, für die noch etwas fehlt (Angebote, Auftrag, Rechnungen), sind ausgegraut. Für die
LV-Ausdrucke wählst du außerdem den *Inhalt* (Langtexte, Bilder, Hinweise und Vorbemerkungen,
Ausführungsbeschreibungen, Kostengruppe, Leistungsbereich, Mengenermittlung, Zusammenstellung
am Ende – auf Wunsch mit der Spalte *Anteil*, jede Titelsumme in Prozent der Summe netto, ab Werk
an –, neuer Titel auf neuer Seite, Inhaltsverzeichnis, Übertrag, Lesezeichen für Positionen), das *Deckblatt*, den
*Bereich* (ganzes LV oder die markierten Titel) und die *Seite* (A4 hoch oder quer, Schrift 9,
10 oder 11 pt). Unten: Kopf- und Fußzeile, *Blattrahmen*, *Gitter*, je eine Zusatzzeile für Kopf und Fuß
(z. B. „Vorabzug – nicht zur Ausführung“), #menu[Seiten zählen] und „nach dem Speichern
öffnen“. Langtexte erscheinen mit Formatierung, Tabellen und Bildern; Arial/Times/Courier im Text
werden als Liberation Sans/Serif/Mono gedruckt.

*Sehr große LVs:* Ein LV mit 2000 Positionen (rund 500 Seiten) braucht beim Drucken etwa 2 bis
2,5 GB Arbeitsspeicher, ohne Übertrag etwa ein Drittel weniger; Langtexte mit sehr vielen
Zeilenumbrüchen ergeben entsprechend mehr Seiten und brauchen mehr. Reicht der freie Arbeitsspeicher
voraussichtlich nicht, druckt proAVA nicht, sondern nennt in einem Hinweis, wie viel der Druck
braucht und wie viel frei ist. Dann andere Programme schließen, nur die markierten Titel drucken
(*Bereich*) oder ohne Übertrag drucken.

*Dokumente aus einem Fenster* gehen ebenfalls über den Druckdialog, dort steht dann nur ihre
Dokumentart mit ihren Optionen; #taste[Enter] speichert:

#table(
  columns: (auto, 1fr),
  table.header[Dokument (Knopf)][Optionen neben dem Stil],
  [Änderungsliste (LV-Vergleich, #menu[Drucken …])], [Seite A4 hoch oder quer, Blattrahmen, Kopf-/Fußzeile],
  [Prüfung Nachtragsangebot (#menu[Prüfung drucken …])], [Blattrahmen, Kopf-/Fußzeile],
  [Nachtragsübersicht (#menu[Übersicht drucken …])], [Blattrahmen, Kopf-/Fußzeile],
  [Schreiben (Schriftverkehr, #menu[PDF erzeugen …])], [Ein PDF je Bieter (in einen Ordner), Im Projekt ablegen, Fußzeile],
)

== Lesezeichen

Jedes PDF bringt *Lesezeichen* mit (in Acrobat, Xreader und anderen Betrachtern die Leiste
links, meist #menu[Lesezeichen] oder #menu[Inhalt]). Ein Klick springt an die Stelle; gedruckt
wird davon nichts, und beim Kopieren aus dem PDF kommt kein zusätzlicher Text mit.

- *LV-Ausdrucke:* Deckblatt, Vertragsbedingungen, Inhaltsverzeichnis, Vorbemerkungen, jeder Titel
  mit OZ und Text (Untertitel darunter eingerückt), darunter jede Position („01.481.0010
  DDC-Station“), Hinweise mit Überschrift und Ausführungsbeschreibungen („A0001 Kabel …“),
  am Ende Zusammenstellung, Schlussbemerkungen und Angaben des Bieters. Ein Titel über mehrere Seiten hat ein
  Lesezeichen auf seiner ersten Seite. Im Inhaltsverzeichnis ist jeder Eintrag ein Link zu seinem
  Titel, ebenso die Seitenzahlen in der Zusammenstellung.
- *Lesezeichen für Positionen* (Inhalt, Vorgabe an) schaltet nur die Positionen ab – bei sehr
  großen LVs wird die Liste dann kürzer, die Titel bleiben.
- *Preisspiegel:* Übersicht der Angebote, jeder Titel, Angebotssummen.
- *Kostenermittlung:* Deckblatt, Zusammenfassung, jedes Bauteil, Kostenkontrolle, Erläuterungen.
- *Prüfung Nachtragsangebot:* Nachtrag, Positionen (jede einzeln), Ergebnis der Prüfung,
  Unterschriften; *Nachtragsübersicht:* jeder Nachtrag, Summen je Status, Auftragssumme.
- *Aufmaßprüfung:* Rechnung, Titel und Positionen, Summen und Befunde; *Rechnungsfreigabe:*
  Rechnung, jeder Abschnitt, Unterschriften.
- *Niederschrift:* jeder Bieter, Unterschriften; *Änderungsliste:* jede Änderung;
  *Schreiben:* im Sammel-PDF ein Lesezeichen je Empfänger.

== Stil

Oben rechts im Druckdialog wählst du den *Stil* – für jede Dokumentart:

- *Klassisch* (Vorgabe): schwarze Schrift, Titelzeilen leicht in der Akzentfarbe hinterlegt
  (oberste Stufe etwas kräftiger), Linie unter dem Tabellenkopf in der Akzentfarbe.
- *proAVA:* Überschriften, Spaltenköpfe und Dokumentart in der Akzentfarbe, Kopf- und
  Fußzeile grau, nur die oberste Titelstufe hinterlegt.

Die Vorgabe für alle Dokumentarten und die *Akzentfarbe* (Vorgabe Petrolblau \#004F7C) stellst
du unter #menu[Extras][Einstellungen …] ▸ Drucken ein. Briefe, Niederschrift und Nachtrags-PDFs
drucken ebenfalls im Stil und in der Akzentfarbe der Einstellungen.

Das Häkchen *Blattrahmen* zieht eine Linie um Kopfzeile, Inhalt und Fußzeile jeder Seite, hoch
und quer; Kopf und Fuß stehen innen, senkrecht mittig in ihrem Feld und durch eine Linie vom
Inhalt getrennt. Das Deckblatt ist mit
gerahmt (ohne Kopfzeile), Briefe nie. Vorgabe und Strichstärke (dünn oder mittel) stehen ebenfalls
unter Einstellungen ▸ Drucken.

Das Häkchen *Gitter* druckt die Tabellen voll umrandet: Außenrahmen, alle senkrechten und
waagrechten Linien, dünn und dunkelgrau, unter dem Spaltenkopf und um Summen etwas kräftiger. Im LV
bekommt jede Position einen Rahmen, ihr Langtext bleibt ohne Linien, die Preiszeile steht in
Feldern unter Menge, Einheit, EP und GP. Die Vorgabe für alle Ausdrucke ist
#menu[Extras][Einstellungen …] ▸ Drucken ▸ *Tabellen voll umrandet (Gitter)* (ab Werk aus), der
Preisspiegel ist ab Werk umrandet; im Druckdialog merkt sich proAVA die Wahl je Dokumentart.

== Deckblatt

Jede LV-Dokumentart kann ein Deckblatt haben: *Standard*, *Briefkopf*, *Tabelle 1*,
*Tabelle 2* oder *ohne*. Das LV ohne Preise hat von Haus aus das Standard-Deckblatt.

- *Standard:* Dokumentart groß, LV, Projekt und Baumaßnahme, darunter Bauherr, Auftraggeber/
  Vergabestelle und Planer, die Kenndaten (Vergabeart, -nummer, Abgabetermin und -ort,
  Bindefrist, Ausführungsfrist, Stand) und beim LV ohne Preise ein gerahmtes Feld
  *Vom Bieter auszufüllen* (Bieter, Anschrift, Angebotssumme netto/USt/brutto, Ort, Datum,
  Unterschrift und Stempel).
- *Briefkopf:* Vergabestelle oben links wie im Brief, die Kenndaten im Kasten rechts, die
  Bieterangaben als Formularzeilen.
- *Tabelle 1:* Projektbezeichnung fett über der Dokumentart, darunter Adressen, Kenndaten und
  Bieterfeld in voll umrandeten Tabellen mit hinterlegten Beschriftungen; je Beteiligtem eine
  Spalte mit dem ganzen Adressblock.
- *Tabelle 2:* wie Tabelle 1, die Adressen Feld für Feld: je Beteiligtem eine Spalte, je Feld
  (Name, Zusatz, Straße, PLZ Ort, Telefon, E-Mail) eine Zeile. Felder, die bei allen
  Beteiligten leer sind, entfallen.

Die Baumaßnahme aus den Projektstammdaten steht in allen Formen als *Baumaßnahme: …* unter
dem LV.

Leere Angaben lassen ihre Zeile weg. Sicherheiten, Skonto, Abzüge, Zahlungsbedingungen und
weitere Beteiligte stehen auf der Folgeseite *Vertragsbedingungen*; das LV beginnt danach.
Welche Blöcke das Deckblatt zeigt (Logo, Adressen, Kenndaten, Bieterfeld), stellst du unter
Einstellungen ▸ Drucken ein.

== Kopf- und Fußzeile <kopffuss>

Kopf- und Fußzeile jeder Seite haben je drei Felder (links, Mitte, rechts) mit einer oder zwei
Zeilen aus Text und Platzhaltern, z. B. `{projekt}`, `{titel}` (Titel, in dem die Seite
beginnt), `{vergabenummer}`, `{waehrung}` (Währung des LV, z. B. EUR), `{ersteller}`
(Bearbeiter aus den Einstellungen, sonst der Planer), `Seite {seite} von {seiten}`; die Liste
steht im Dialog unter *Platzhalter*. Unter jedem Feld setzt *Baustein …* fertige Inhalte ein
(Seite x von y, Seite x, Ersteller, Datum, Projekt …): ein leeres Feld bekommt den Baustein,
sonst wird er die zweite Zeile. Dazu: Linie unter dem Kopf und über dem Fuß, Logo links oder
rechts im Seitenkopf, Schriftgröße. Die Vorgabe stellst du unter #menu[Extras][Einstellungen …] ▸
Kopf-/Fußzeile ein, ein Projekt kann unter #menu[Projekt][Projektstammdaten …] ▸ Kopf-/Fußzeile
eine eigene haben (auch über den Knopf #menu[Kopf-/Fußzeile …] im Druckdialog). Eine Vorschau
zeigt das Ergebnis. Die Zusatzzeilen des Druckdialogs erscheinen über `{kopftext}` und
`{fusstext}`. Zu lange Texte werden mit „…“ gekürzt; Briefe nutzen nur die Fußzeile.

= Abrechnung <abrechnung>

Der Reiter #menu[Abrechnung] (#taste[Strg+4]) prüft je LV die Rechnungen der beauftragten Firma:
Mengen, Aufmaß und Leistungsfortschritt je Position, daraus der Rechenweg bis zum
Freigabebetrag. Grundlage ist der *festgeschriebene Auftrag* – Auftragsmengen, Auftrags-EP,
Nachlass und Vertragsbedingungen zum Zeitpunkt der Beauftragung. Spätere Änderungen am LV
ändern den Auftrag nicht. Alle Eingaben lassen sich mit #taste[Strg+Z] zurücknehmen.

#bild("abrechnung.jpg")[Reiter Abrechnung: Auftrag, Rechnungen, Positionen der Schlussrechnung, Rechenweg und Befunde]

== Auftrag festschreiben

Ist im Reiter Angebote ein Angebot beauftragt, übernimmt #menu[Auftrag übernehmen] es als
Auftrag. Ohne Vergabe in proAVA liest #menu[Auftrag aus GAEB …] ein Auftrags-LV (X86, D86);
es gelten dann die Mengen und Preise der Datei, Abweichungen zum LV nennt proAVA.
Positionen mit „entfällt“ gehören nicht zum Auftrag. Beauftragte Nachträge kommen als eigene
Positionen dazu. #menu[Auftrag neu übernehmen …] schreibt den Auftrag neu fest; Rechnungen,
Aufmaß und Nachträge bleiben.

== Rechnungen erfassen

#menu[Neue Rechnung] legt eine Rechnung an; Mengen- und Freigabeart übernimmt sie von der
letzten. #menu[Rechnung aus GAEB (X89/X31) …] liest eine Rechnung der Firma (X89, X89B) mit
Kopf, Mengen, Preisen, Prüfvermerken und Aufmaß als neue Rechnung ein. Eine
Mengenermittlung (X31) kommt als Aufmaß zur gewählten Rechnung.

Im *Rechnungskopf* stehen Nummer, Art, Daten (Rechnung, Eingang, Zahlungsziel,
Leistungszeitraum), Forderung netto, Umsatzsteuer (19 %, 7 % oder § 13b), Sicherheitseinbehalt,
angerechnete Freigaben, Pauschalbeträge, Korrektur mit Begründung und der Prüfvermerk.

- *Arten:* Abschlags- oder Teilrechnung, Schlussrechnung (immer kumuliert, alle bisherigen
  Freigaben werden angerechnet), Einzelrechnung außerhalb der Folge (z. B. Regie) und Freigabe
  des Sicherheitseinbehalts.
- *Mengen der Firma:* kumuliert (Gesamtstand) oder Zuwachs (nur diese Rechnung).
- *Freigabe:* kumuliert (Gesamtleistung abzüglich der bisherigen Freigaben) oder nur diese
  Rechnung.

Die Liste zeigt je Rechnung Forderung, geprüfte Leistung, Freigabe brutto und den Stand
(„in Prüfung · 2 Befunde“, „geprüft am …“).

== Positionen prüfen

Die Tabelle gliedert sich wie das LV und zeigt je Position Auftragsmenge, bisher geprüft,
Menge der Rechnung, geprüfte Menge, Leistungsfortschritt, Quelle, EP, Betrag und Vermerk.
Bearbeitet wird in der gewählten Zeile: Pfeiltasten wählen die Position, #taste[Enter] oder
#taste[F2] bearbeitet „geprüft“, #taste[Tab] wechselt das Feld, Pfeil auf/ab übernimmt und bleibt
in der Spalte.

Welche Angabe zählt, steht in der Spalte *Quelle*. Automatisch gilt: Leistungsfortschritt vor
geprüfter Menge vor Aufmaß vor Rechnungsmenge; in der Zeile lässt sich die Quelle fest wählen.
Der *Leistungsfortschritt* (in % oder als Faktor, Klick auf den Spaltenkopf schaltet um) braucht
einen Vermerk, worauf er beruht. Bei Stundenlohnpositionen hakt man ab, ob der Nachweis vorliegt.

Befunde färben die Zeile – orange Warnung, rot Fehler – und stehen rechts unten in der
Befundliste; ein Klick springt zur Position. Geprüft wird unter anderem: Position nicht im
Auftrag, Nachtrag noch nicht beauftragt, EP abweichend vom Auftrag (es gilt der Auftrags-EP),
Rechenfehler, Mehrmenge über der Auftragsmenge (über 10 % mit Hinweis auf § 2 Abs. 3 VOB/B),
Stundennachweis fehlt, Forderung passt nicht zur Summe, Auftragssumme überschritten.

== Aufmaß

#taste[Strg+M], ein Doppelklick oder #menu[Aufmaß der Position …] öffnet das Aufmaß der
Position: je Zeile Ort oder Bezeichnung und ein Rechenansatz wie `2*(64+38,5)`. Jede Zeile
gehört zu einer Rechnung und zählt dort als Zuwachs; nicht zugeordnete Zeilen zählen nirgends.

#bild("abrechnung-aufmass.jpg", breite: 80%)[Aufmaß einer Position, Zeilen der 2. Abschlagsrechnung]

Aus GAEB (X89 oder X31) übernimmt proAVA je Position die Aufmaßmenge der Datei. Die
Aufmaßzeilen nach REB 23.003 stehen als Text dabei; proAVA rechnet ihre Formeln nicht nach.
Bei kumulierten Rechnungen zählt nur der Zuwachs gegenüber dem bisher Geprüften.

== Rechenweg der Freigabe

Links unten steht der Rechenweg, so wie er auch gedruckt wird:

+ geprüfte Leistung netto (kumuliert oder Zuwachs),
+ abzüglich Nachlass und Abzüge laut Vertrag (Bauwesenversicherung, Baustrom …),
+ abzüglich Sicherheitseinbehalt (Vertragserfüllung, bei der Schlussrechnung Mängelansprüche),
+ abzüglich anrechenbarer Freigaben, Rundung oder Korrektur,
+ Freigabe netto, Umsatzsteuer, Freigabe brutto; Skonto zur Information.

Darunter: Freigaben gesamt, Einbehalt, Stand gegen den Auftrag, Prognose laut Aufmaß und die
bisherigen Freigaben.

== Drucken und Anschreiben

#menu[PDF Aufmaßprüfung …] und #menu[PDF Rechnungsfreigabe …] öffnen den Druckdialog mit der
gewählten Rechnung. Die *Aufmaßprüfung* (A4 quer) zeigt je Position Auftrag, bisher, Rechnung und
geprüft mit Beträgen, Differenz, Stand, Quelle, Vermerke und Befunde, darunter die Summen. Die
*Rechnungsfreigabe* zeigt den Rechenweg mit Auftrag und Stand, bisherigen Freigaben, dem Hinweis
zur Bauabzugsteuer und Unterschriftsfeldern für Planer und Bauherr.

#bild("rechnungsfreigabe.jpg", breite: 62%)[Rechnungsfreigabe (Stil „Klassisch“)]

#menu[Anschreiben an den Bauherrn …] öffnet den Schriftverkehr mit der Vorlage *Freigabe an den
Bauherrn*. Sie geht an den Auftraggeber (sonst den Bauherrn) und kennt die Platzhalter
`{rechnung.nummer}`, `{rechnung.art}`, `{rechnung.datum}`, `{rechnung.forderung}`,
`{freigabe.netto}`, `{freigabe.brutto}`, `{freigaben.gesamt}`, `{einbehalt}`, `{auftragnehmer}`
und `{auftrag.nummer}`. Den Text kannst du wie die übrigen Vorlagen anpassen. Mit *Im Projekt
ablegen* kommt das Anschreiben in die Ablage (@schriftverkehr).

= Nachträge <nachtraege>

Der Reiter #menu[Nachträge] (#taste[Strg+5]) führt die Nachträge eines LV:
von der Ankündigung über das Nachtragsangebot der Firma und die Prüfung bis zur Beauftragung.
Grundlage ist der Auftrag (#menu[Auftrag erteilen …]); fehlt er noch, bietet der Reiter an, ihn aus
dem beauftragten Angebot zu übernehmen. Beauftragte Nachträge erhöhen die Auftragssumme und sind in
der Abrechnung abrechenbar.

#bild("nachtraege-liste.jpg")[Nachtragsliste mit Summen je Status und Auftragssumme]

== Nachtragsliste und Übersicht

Die *Nachtragsliste* zeigt je Nachtrag Nummer (N01, N02 … = GAEB-Nachtragsnummer), Bezeichnung,
Anlass, Status mit Datum, angebotene, geprüfte und beauftragte Summe (nach dem Nachlass des Auftrags,
wo er gilt), die Abweichung geprüft gegen angeboten und ob er in die Prognose eingeht. Darüber filtern
Status (alle, offen, je Status, abgelehnt/zurückgezogen) und Suche. Darunter stehen die
Summen je Status und die Auftragssumme: alt, plus beauftragte Nachträge, neu, offene Nachträge und
*Prognose* (mit den offenen Nachträgen, die eingehen sollen). Die *Nachtragsübersicht* fasst alles wie
eine Nachtragsaufschlüsselung zusammen; #menu[Übersicht drucken …] druckt sie (über den Druckdialog).

== Nachtrag bearbeiten

#menu[Neuer Nachtrag] legt den nächsten an (Status angekündigt). Ein Klick auf Nummer oder
Bezeichnung öffnet ihn. Im *Kopf* stehen Anlass nach VOB/B (geänderte oder zusätzliche Leistung,
Mengenänderung über 10 %, Leistung ohne Auftrag, Behinderung), Begründung, Firma (leer = Auftragnehmer),
Fristen, Angebotsnummer und -datum, Prüfvermerk, Empfehlung an den Bauherrn, ob der Nachlass gilt und
ob der Nachtrag offen in die Prognose eingeht.

#bild("nachtrag-bearbeiten.jpg")[Nachtrag mit Nachtrags-LV, Prüfhilfe und Langtext]

Das *Nachtrags-LV* darunter:

- #menu[Neue Position] schreibt eine Position in den *Nachtragsbereich* am Ende des LV: einen Titel
  „Nachträge“ und darunter je Nachtrag einen Titel „N01 …“. Die OZ ist vorgeschlagen und änderbar;
  die OZ der Auftragspositionen ändern sich dabei nie.
- #menu[Zuordnen] nimmt Positionen oder ganze Titel aus dem LV (die schon im LV-Editor geschrieben
  wurden) oder eine Auftragsposition als *Mehr- oder Mindermenge* (Menge = Differenz, negativ = Minderung;
  ohne EP gilt der Auftrags-EP).
- Je Position: angebotene Menge und EP, geprüfte Menge und EP (leer = wie angeboten), Abweichung in
  Prozent, ein abweichender Status (z. B. eine abgelehnte Position in einem beauftragten Nachtrag – sie
  zählt nicht und steht in Klammern) und der Prüfvermerk („EK 102 €, Zeitansatz in Ordnung“).
- Die *Prüfhilfe* zur gewählten Position nennt Vergleichspreise mit der Abweichung des angebotenen EP:
  EP des Auftrags bzw. der ähnlichsten Auftragsposition, Planerpreis und die Bieterpreise aus dem
  Preisspiegel; #menu[als geprüften EP] übernimmt einen davon. Kurztext, Einheit und Langtext neuer
  Positionen lassen sich hier direkt bearbeiten.

#menu[Status ändern …] setzt angekündigt, angeboten, in Prüfung, geprüft, beauftragt, abgelehnt oder
zurückgezogen mit Datum und Vermerk; der Wechsel steht im *Statusverlauf*. #taste[Strg+Z] nimmt
Änderungen zurück, auch angelegte Positionen.

== Nachtragsangebot, GAEB und PDF

Im Menü #menu[GAEB] des Nachtrags:

- *Nachtrags-LV als X83* schickt die neuen Positionen als Nachtragsangebotsaufforderung an die
  Firma, *als X86* den Nachtragsauftrag mit den geprüften Preisen (abgelehnte Positionen
  stehen mit ihrem Status als „entfällt“ darin und zählen nicht) – jeweils mit Nachtragsnummer und
  Status. Mehr- und Mindermengen kennt GAEB nicht; sie stehen im PDF.
- #menu[Nachtragsangebot einlesen (X84, X86, X82, D8x) …] übernimmt die Datei der Firma:
  Positionen mit gleicher OZ bekommen Menge und EP (geprüfte Werte bleiben), unbekannte Positionen
  kommen mit Text in den Nachtragsbereich. Ein angekündigter Nachtrag wird „angeboten“.

#menu[Prüfung drucken …] erzeugt über den Druckdialog die *Prüfung Nachtragsangebot*: Positionen angeboten und geprüft
mit Vermerken, Summen mit Nachlass, Prüfvermerk, Empfehlung, „fachtechnisch und rechnerisch geprüft am …
von …“ und Unterschriftsfelder für Planer und Auftraggeber.

= Kosten nach DIN 276 <kosten>

Eine Kostenschätzung (DIN 276, 2. Ebene der Kostengliederung) oder Kostenberechnung (3. Ebene)
entsteht in proAVA auf zwei Wegen:

- *Nach Positionen als Kosten-LV* – der Normalfall für alle Gewerke: ein Kurztext-LV mit Menge ×
  Kostenkennwert je Bezugseinheit (m Rohrleitung, St. Ventil, kW Heizleistung …), gegliedert
  nach Kostengruppen. Es wird im Reiter #menu[LV] bearbeitet wie jedes LV (Abschnitt
  @kosten-lv).
- *Nach Datenpunkten* – der Sonderfall Gebäudeautomation (KG 480): Datenpunkte je Bauteil ×
  Kennwert bzw. EP je DP-Art, im Reiter #menu[Kosten] (die folgenden Abschnitte).

Beides lässt sich verbinden: ein Kosten-LV, in dem die KG 480 nach Datenpunkten gerechnet wird
(GU-Fall, Abschnitt @gu-fall).

== Kosten-LV (Kostenermittlung als Kurztext-LV) <kosten-lv>

#menu[Kosten][Neue Kostenschätzung als Kurztext-LV …] bzw. #menu[Neue Kostenberechnung als
Kurztext-LV …] – oder die gleichnamigen Knöpfe im Reiter #menu[Kosten] – öffnen den Dialog
#menu[Neues LV] mit der *Art* Kostenschätzung bzw. Kostenberechnung; dieselbe Wahl hat
#menu[LV][Neues LV …]. Für die Kostengruppen 410 bis 490, die du anhakst, legt proAVA je einen
Titel an, mit der KG als Ordnungszahl (420 Wärmeversorgungsanlagen). Die Kostenberechnung bekommt
darunter je KG der 3. Ebene einen Untertitel (420.421 Wärmeerzeugungsanlagen …); leere Untertitel
lassen sich löschen. Eine Vorbemerkung oben nimmt auf, was DIN 276 zu jeder Kostenermittlung
verlangt: Kostenstand, Grundlagen, Verfahren und Quellen der Kostenkennwerte, Umsatzsteuer.

#bild("kosten-lv.jpg")[Kosten-LV im Reiter LV: Titel nach Kostengruppen, darüber die Hinweiszeile]

Die Positionen trägst du ein wie in jedem LV: Kurztext, Menge, Einheit und als EP den
Kostenkennwert – aus der Bibliothek mit ihren Erfahrungspreisen, aus anderen Projekten oder aus
Angeboten. Die Kostengruppe kommt vom Titel oder steht an der Position (Spalte KG). Die Zeile über
der Tabelle zeigt, dass es eine Kostenermittlung ist. Ein Kosten-LV steht in der LV-Tabelle für
sich, nie zwischen den LVs der Ausschreibung; seine Kopfzeile nennt die Stufe.

Ein Kosten-LV geht nie an Bieter: Bieter, Angebote, Preisspiegel, Auftrag, Nachträge und
Abrechnung gibt es dafür nicht – die Menüeinträge sind grau, die Reiter zeigen einen Hinweis.
Ein LV, das schon Bieter, Angebote, einen Auftrag, Nachträge oder Rechnungen hat, bleibt ein
Ausschreibungs-LV (#menu[LV-Eigenschaften …] lehnt die Art dann ab).

#menu[Prüfen][LV prüfen …] meldet im Kosten-LV Positionen ohne Kostengruppe und solche, deren KG
gröber ist als die Stufe verlangt (Kostenberechnung: KG 420 statt 421 …).
#menu[Kosten][Kostenberechnung aus Kostenschätzung (Kurztext-LV)] kopiert das aktive (sonst das
letzte) Kosten-LV einer Kostenschätzung als Kostenberechnung; danach verfeinerst du die
Kostengruppen bis zur 3. Ebene – #menu[LV prüfen …] zeigt, wo.

#menu[Drucken][Kostenermittlung (Kosten-LV, DIN 276) …] – bei einem Kosten-LV auch #taste[Strg+P] –
druckt die Angaben nach DIN 276, die Zusammenstellung nach Kostengruppen in den Ebenen der Stufe
(Kostenschätzung bis zur 2., Kostenberechnung bis zur 3. Ebene) mit Summe netto, Umsatzsteuer und
brutto und danach die Kurztext-Liste mit Menge, Einheit, Kennwert und Betrag; auf Wunsch mit den
Langtexten (z. B. der Quelle eines Kennwerts). Die Kurztext-Liste lässt sich abwählen.

#bild("kosten-lv-drucken.jpg", breite: 55%)[Druckdialog mit dem Kosten-LV: Angaben, Kurztext-Liste, Langtexte]

#bild("kosten-lv-pdf.jpg")[Kostenberechnung als Kosten-LV im PDF: Zusammenstellung nach Kostengruppen]

== GU: KG 400 komplett in einer Kostenermittlung <gu-fall>

Braucht ein Generalunternehmer die ganze KG 400 als eine Kostenermittlung, stehen die
Kurztext-Positionen der übrigen Kostengruppen (410 bis 470) und die Datenpunktrechnung der
Gebäudeautomation (480) in einem Kosten-LV. Die KG 480 ist dort ein *Datenpunkt-Abschnitt*: ein
Titel, der statt Positionen die Anlagenliste trägt.

#menu[Einfügen][Datenpunkt-Abschnitt einfügen (KG 480)] – nur in einem Kosten-LV, auch im
Rechtsklick – macht den leeren Titel 480 der Vorlage zum Datenpunkt-Abschnitt (seine leeren
Untertitel fallen weg) oder legt einen neuen Titel 480 vor 490 an. Ein Doppelklick auf den Titel
öffnet die Anlagenliste in einem Fenster über dem LV: DP-Arten mit Preisen, die Bauteile mit
ihren Anlagen und rechts das Ergebnis mit der Aufteilung auf die KG 481–489 – dieselbe Ansicht
wie im Reiter #menu[Kosten], bedient genauso. Bezeichnung, Stand und Bearbeiter sind die des
Kosten-LV. Jede Änderung ist ein Schritt im Rückgängig des LV (#taste[Strg+Z]); #taste[Esc]
schließt das Fenster.

#bild("gu-anlagenliste.jpg")[Kosten-LV mit Datenpunkt-Abschnitt 480: die Anlagenliste im Fenster über dem LV]

In der LV-Tabelle trägt der Titel den Zusatz „nach Datenpunkten“ mit der Zahl der Datenpunkte und
seine Summe; im Eigenschaftenbereich stehen Bauteile, Datenpunkte, die Beträge je Kostengruppe
und #menu[Anlagenliste öffnen …]. Die Stufe folgt dem LV: In einer Kostenschätzung ist der
Abschnitt die KG 480 (Kennwert je DP oder je DP-Art), in einer Kostenberechnung rechnet er mit EP
je DP-Art und teilt auf 481–489 auf; #menu[Kostenberechnung aus Kostenschätzung (Kurztext-LV)]
nimmt ihn mit. Kopieren und Einfügen in ein anderes Kosten-LV bringt die Anlagenliste mit, in
einem Ausschreibungs-LV bleibt nur der Titel.

Der Betrag des Abschnitts zählt im LV, in #menu[Kosten nach Kostengruppen], in der
Kostenverfolgung und im Druck genau einmal – mit dem Kosten-LV. #menu[Prüfen][LV prüfen …] meldet
Kostengruppen, die doppelt erfasst sind: eine Kurztext-Position mit einer KG, die schon der
Abschnitt abdeckt (Kostenschätzung: jede KG 48x, Kostenberechnung: die KG seiner Aufteilung),
und einen zweiten Datenpunkt-Abschnitt; dazu die Hinweise der Datenpunktrechnung (Aufteilung
nicht 100 %, DP-Art ohne Preis …).

Eine eigenständige Datenpunktrechnung im Reiter #menu[Kosten] wird mit dem Knopf
#menu[In Kosten-LV übernehmen] zu einem neuen Kosten-LV ihrer Stufe mit genau diesem Abschnitt
(Titel 480, Bezeichnung und Stand der Datenpunktrechnung in der Vorbemerkung); sie zählt danach
nur noch dort. Die Kurztext-Positionen der übrigen Kostengruppen kommen dann dazu.

#menu[Drucken][Kostenermittlung (Kosten-LV, DIN 276) …] zeigt in der Zusammenstellung beide
Teile. In der Kurztext-Liste steht der Abschnitt als eine Zeile mit Datenpunkten und Betrag;
danach folgt je Abschnitt ein Anhang „Datenpunkte“ (A4 quer, mit Lesezeichen): Zusammenfassung je
Bauteil, DP-Arten mit EP, Aufteilung mit Prüfsumme und Anlagenliste.

#bild("gu-pdf.jpg")[Anhang „Datenpunkte“ im PDF des Kosten-LV: ein Bauteil mit Aufteilung und Anlagenliste]

== GA-Kostenberechnung als Kurztext-LV <ga-kurztext>

Die Gebäudeautomation lässt sich wie jedes andere Gewerk als Kurztext rechnen – dann ist die
Darstellung im ganzen Kosten-LV einheitlich. Die Datenpunkte stehen als Positionen da (1.1.1
Analoge Eingabe (AI) … 1.2.2 Binärwert (BV), Menge × EP je DP-Art), darunter verteilen
*Aufteilungszeilen* ihre Summe auf die Kostengruppen 481 bis 489.

#menu[Einfügen][GA nach Datenpunkten (Kurztext-Vorlage) einfügen] legt den ganzen Block an: einen
Titel „Gebäudeautomation nach Datenpunkten“ (KG 480; ein Untertitel, wenn der Titel an der
Schreibmarke eine Ebene darunter hat), die sechs Datenpunkte mit den EP der Datenpunktrechnung
(400 € bzw. 200 €) und darunter die Aufteilung 481 40 %, 482 12 %, 483 10 %, 484 30 %, 485 5 %,
489 3 % – die Vorgaben der Kostenberechnung nach Datenpunkten. Du trägst nur noch die Mengen ein;
für Anlagen- und Raumautomation fügst du den Block zweimal ein und benennst die Titel um.

#menu[Einfügen][Aufteilung nach KG einfügen] legt nur die sechs Aufteilungszeilen an, hinter der
Schreibmarke (auf einem Titel am Ende seiner Positionen). Ihr Bezug sind die Positionen davor im
Titel – wie bei einem Zuschlag „auf alle Positionen davor“; sind mehrere Positionen markiert,
genau diese. Beide Befehle gibt es nur im Kosten-LV, auch im Rechtsklick und in der Befehlssuche.
Ist zwischen den Nachbarn keine Ordnungszahl frei, bekommen die Positionen des Titels neue
Nummern; die Titel behalten ihre.

#bild("ga-kurztext.jpg")[GA als Kurztext: Datenpunkte als Rechengrundlage, darunter die Aufteilung auf KG 481–489 mit „Σ 100 %“]

Eine Aufteilungszeile ist eine Zuschlagsposition mit dem Kennzeichen #menu[Aufteilung nach KG]:
Im EP steht ihr Anteil in Prozent, in der Spalte Menge die Bezugssumme, der GP ist Bezugssumme ×
Anteil; die Kostengruppe steht an der Zeile. Prozent und KG änderst du direkt in der Tabelle,
#taste[Strg+Z] nimmt jede Änderung zurück. Die bezogenen Positionen sind nur noch
*Rechengrundlage*: Ihr GP steht in Klammern, sie tragen das Kennzeichen „Rechengrundlage“ und
zählen weder im Titel noch im LV, in den Kostengruppen oder in der Kostenverfolgung – das Geld
tragen die Aufteilungszeilen, so wird nichts doppelt gezählt.

An der letzten Zeile eines Blocks zeigt proAVA die Summe der Anteile („Σ 100 %“, sonst rot mit
der Abweichung). Ergeben die Anteile 100 %, geht der Rundungsrest von wenigen Cent auf die Zeile
mit dem größten Anteil, damit der Block genau die Bezugssumme ergibt. #menu[Prüfen][LV prüfen …]
meldet Blöcke, die nicht 100 % ergeben (mit dem nicht verteilten Betrag), und Aufteilungszeilen
ohne Rechengrundlage. Eine vorhandene Zuschlagsposition wird im Eigenschaftenbereich mit
#menu[Aufteilung nach KG] zur Aufteilungszeile.

Im Druck #menu[Kostenermittlung (Kosten-LV, DIN 276) …] stehen die Aufteilungszeilen mit
„40 % von 118.200,00 €“ und ihrem Betrag, die Rechengrundlage mit GP in Klammern und dem Vermerk
„Rechengrundlage, aufgeteilt“; die Zusammenstellung nach Kostengruppen rechnet mit den
Aufteilungszeilen.

#bild("ga-kurztext-pdf.jpg", breite: 80%)[Kurztext-Liste im PDF: Rechengrundlage in Klammern, Aufteilungszeilen mit Anteil und Bezugssumme]

Ein Datenpunkt-Abschnitt (Abschnitt @gu-fall) lässt sich in diese Form bringen:
#menu[Bearbeiten][Umwandeln in][Datenpunkt-Abschnitt in Kurztext-Positionen umwandeln] – auch im
Rechtsklick und als Knopf im Eigenschaftenbereich des Titels – setzt je Bauteil die Datenpunkte
mit der Summe ihrer Anlagenliste ein und darunter die Aufteilung des Bauteils (mehrere Bauteile
als Untertitel). Die Summen je Kostengruppe bleiben gleich; die einzelnen Zeilen der
Anlagenliste (Ort, Anlage, Bemerkung) entfallen, #taste[Strg+Z] holt sie zurück.

== Datenpunktrechnung der Gebäudeautomation

Der Reiter #menu[Kosten] (#taste[Strg+6]) rechnet die Kosten der Gebäudeautomation
(Kostengruppe 480) aus den Datenpunkten – vom ersten Überschlag bis zur Kostenberechnung, im
selben Projekt wie später das LV. Oben im Reiter stehen die Kosten-LVs des Projekts (ein Klick
öffnet eines im Reiter LV) mit Knöpfen für neue. Ein Projekt kann mehrere Kostenermittlungen haben, z. B. die
Kostenschätzung der LPH 2 und die Kostenberechnung der LPH 3; jede hat Bezeichnung, Stand,
Bearbeiter und Bemerkung. Alle Eingaben lassen sich mit #taste[Strg+Z] zurücknehmen. In der
Kopfleiste steht der Reiter, sobald das Projekt eine Kostenermittlung hat; #menu[Ansicht][Ansicht Kosten]
und #taste[Strg+6] erreichen ihn immer, das Menü #menu[Kosten] legt neue Kostenermittlungen an. Das PDF
druckt #menu[Drucken][Kostenschätzung / Kostenberechnung (DIN 276) …].

#bild("kosten.jpg")[Reiter Kosten: Anlagenliste je Bauteil, rechts Ergebnis, Aufteilung und Prüfsumme]

Die rechte Spalte mit Ergebnis, Aufteilung und Zusammenfassung beginnt direkt unter der Knopfzeile
und rollt für sich; in schmalen Fenstern steht sie unter der Anlagenliste.

- *Kostenschätzung* (DIN 276, 2. Ebene): alle Datenpunkte × Kostenkennwert ergeben die
  KG 480. Der Kennwert gilt für alle Datenpunkte (Vorgabe 450 € je DP) oder je DP-Art
  (#menu[Kennwert je DP-Art]); mit #menu[Spanne von … bis] zeigt proAVA zusätzlich die
  Bandbreite.
- *Kostenberechnung* (DIN 276, 3. Ebene): je DP-Art Anzahl × EP (Vorgabe Hardware AI, BI, AO,
  BO 400 €, virtuell AV, BV 200 €) ergibt die Gesamtsumme je Bauteil; sie wird prozentual auf
  die KG 481–489 aufgeteilt (Vorgabe 481 40 %, 482 12 %, 483 10 %, 484 30 %, 485 5 %,
  489 3 %).

=== Anlegen

#menu[Kosten][Neue Kostenschätzung nach Datenpunkten (GA)] bzw. #menu[Neue Kostenberechnung nach
Datenpunkten (GA)] legt eine Kostenermittlung mit den Vorgaben an, ebenso die Knöpfe im Reiter.
#menu[Kostenberechnung aus Kostenschätzung (Datenpunkte)] (Knopf #menu[Kostenberechnung daraus]) übernimmt
Bauteile und Anlagenliste der gezeigten Kostenschätzung. Für ein neues Projekt gibt es die
Vorlagen „Kostenschätzung DIN 276 (2. Ebene)“ und „Kostenberechnung DIN 276 (3. Ebene)“ im
Dialog #menu[Neues Projekt]: die GA-Gliederung 481–489 mit einer Kostenermittlung. Ein Projekt,
das noch keine Positionen hat, öffnet gleich im Reiter Kosten.

=== DP-Arten und Preise

Die Zeile #menu[DP-Arten und Preise] klappt die Liste auf: Kürzel, Bezeichnung und der Preis je
Datenpunkt. DP-Arten lassen sich hinzufügen, verschieben und entfernen (mit ihren Anzahlen); eine
neue DP-Art bekommt eine eigene Spalte in der Anlagenliste. AV, BV und MV gelten als virtuelle
Datenpunkte, alle übrigen als physikalische Ein- und Ausgänge – das folgt aus dem Kürzel. Im
Ausdruck stehen die physikalischen Datenpunkte als 1.1.1, 1.1.2 … und die virtuellen als 1.2.1 …

=== Anlagenliste

Je Bauteil (Bestand, Neubau, Haus A …) stehen die Anlagen mit Ort, Gewerk, Anlage, der Anzahl
je DP-Art und einer Bemerkung; Summenzeilen je Bauteil und die Gesamtsumme rechnet proAVA. Die
Tabelle arbeitet wie eine Tabellenkalkulation: Klick in eine Zelle und tippen,
#taste[Tab]/#taste[Umschalt+Tab] zur Nachbarzelle, #taste[Enter]/#taste[Umschalt+Enter] eine
Zeile tiefer/höher, #taste[Strg+Enter] neue Zeile darunter, #taste[Esc] bricht ab. Anzahlen
sind ganze Zahlen (leer = 0). Der Rechtsklick fügt Zeilen ein, löscht und verschiebt sie –
auch in ein anderes Bauteil; auf der Bauteil-Zeile benennt, verschiebt und löscht er das
Bauteil. Die Knöpfe #menu[Zeile] und #menu[Bauteil] (mit Plus-Symbol) über der Tabelle legen
neue Zeilen und Bauteile an.

=== Aufteilung, Prüfsumme und Kostenkontrolle

Rechts steht das Ergebnis. Bei der Kostenberechnung folgt die *Aufteilung* auf die
Kostengruppen: oben die Vorgabe für alle Bauteile, darunter je Bauteil die Beträge; mit
#menu[eigene Aufteilung] bekommt ein Bauteil eigene Prozentsätze. Die Anteile müssen 100 %
ergeben – sonst zeigt die *Prüfsumme* (Summe der Kostengruppen minus Gesamtsumme) die Lücke, und
unter #menu[Hinweise] steht, welches Bauteil betroffen ist. Jeder Betrag wird auf Cent gerundet;
der Rundungsrest von wenigen Cent geht an die Kostengruppe mit dem größten Anteil, damit die
Summe der Kostengruppen genau die Gesamtsumme ergibt.

Die *Kostenkontrolle* vergleicht je Kostengruppe die Kostenermittlung mit den Planerpreisen
aller LVs des Projekts (Positionen ohne eigene KG zählen zu der ihres Titels) und, sobald ein
Auftrag erteilt ist, mit den beauftragten Angeboten (netto vor Nachlass). Eine
Kostenschätzung vergleicht die KG 480 mit der Summe der KG 481–489.

=== Drucken

#menu[Drucken …] im Reiter (oder #taste[Strg+P]) öffnet den Druckdialog mit der Dokumentart
„Kostenschätzung / Kostenberechnung (DIN 276)“ und der gezeigten Kostenermittlung. Das PDF
(Vorgabe A4 quer) beginnt mit den Kennzahlen und der Zusammenfassung aller Bauteile je
Kostengruppe, auf Wunsch mit Deckblatt und einem Balkendiagramm der Anteile; je Bauteil folgen
Datenpunkte mit EP und GP, die Aufteilung mit Prüfsumme und die Anlagenliste, am Ende die
Kostenkontrolle und die Erläuterungen mit Kennwerten, Prozentsätzen und der Rundungsregel.
Druckstil, Blattrahmen, Kopf- und Fußzeile gelten wie bei allen Ausdrucken.

#bild("kosten-drucken.jpg", breite: 55%)[Druckdialog mit der Kostenermittlung]

#bild("kosten-pdf.jpg")[Kostenberechnung als PDF: Zusammenfassung und erstes Bauteil]

= Kostenverfolgung <kostenverfolgung>

Der Reiter #menu[Kostenverfolgung] (#taste[Strg+7], #menu[Ansicht][Ansicht Kostenverfolgung], Menü
#menu[Kosten]) verfolgt die Kosten des ganzen Projekts nach DIN 276 (4.4 Kostenkontrolle, 4.5
Kostensteuerung, 4.6 Kostenvorgabe): über alle LVs aller Gewerke und alle Kostenermittlungen, je
Kostengruppe von der Kostenvorgabe bis zur Kostenfeststellung. Er rechnet bei jeder Änderung neu; alle
Eingaben lassen sich mit #taste[Strg+Z] zurücknehmen. In der Kopfleiste steht er, sobald das
Projekt etwas zu verfolgen hat – eine Kostenermittlung, ein Kosten-LV, eine Kostenvorgabe oder einen Auftrag.

#bild("kostenverfolgung.jpg")[Reiter Kostenverfolgung: Kette je Kostengruppe, rechts die gewählte Zeile]

Die Spalten (nur Stufen mit Werten):

- *Kostenvorgabe* – Budget bzw. Kostenvorgabe des Bauherrn (DIN 276, 4.6) je KG; eine Gruppe ohne
  eigene Kostenvorgabe summiert ihre Untergruppen.
- *Kostenschätzung*, *Kostenberechnung* – die geltenden Kostenermittlungen (Reiter Kosten nach
  Datenpunkten oder Kosten-LVs der Gewerke).
- *Kostenvoranschlag* – die Planerpreise der Leistungsverzeichnisse (DIN 276, 4.3.5).
- *Angebot* – das beauftragte Angebot, sonst das für die Kostenverfolgung gewählte, sonst der
  Mindestbieter; netto nach Nachlass, anteilig auf die KG.
- *Auftrag* – die Auftragssumme anteilig je KG und die beauftragten Nachträge.
- *Prognose* – je LV die Kostenfeststellung, sonst Auftrag mit den offenen Nachträgen, die in die
  Prognose gehen, und abgerechneter Mehrleistung, sonst Angebot oder Kostenvoranschlag; noch nicht
  vergebene Kostengruppen aus der Kostenberechnung. Die Seitenleiste nennt die Annahmen je LV.
- *Abrechnung* – die geprüfte Leistung (kumuliert, nach Nachlass); die Kennzahlen nennen die
  Freigaben.
- *Kostenfeststellung* – die geprüften Schlussrechnungen (DIN 276, 4.3.7).

Oben wählst du die Ansicht: #menu[nach Kostengruppen] mit den Summen der 1. und 2. Ebene oder
#menu[nach Vergabeeinheiten] (je LV mit seinen Kostengruppen), die Ebene 1–3, #menu[netto] oder
#menu[brutto] (je LV mit dessen Umsatzsteuer) und die Abweichung unter den Werten: #menu[zur Kostenvorgabe]
oder #menu[zur Vorstufe], jeweils in € und %. Die Vorstufe gilt je Vergabeeinheit – ein Auftrag
steht gegen das Angebot desselben LV, nicht gegen die Angebote aller LVs; der Kostenvoranschlag
steht gegen die Kostenberechnung. Liegt ein Wert über der Kostenvorgabe, ist die Zelle rot hinterlegt;
liegt er mehr als die Schwelle (Vorgabe 10 %) über der Vorstufe, gelb. Ein Klick auf eine Zeile
zeigt rechts alle Stufen mit beiden Abweichungen, die Erläuterung und bei einem LV Angebot und
Prognose; das Dreieck klappt Gruppen und LVs zu.

Brutto rechnet jedes LV – Ausschreibungs- wie Kosten-LV – mit seiner eigenen Umsatzsteuer,
Datenpunktrechnungen (ohne eigenen Satz) mit dem Satz, den die meisten LVs des Projekts tragen
(19 % bei Gleichstand). Die Kostenvorgabe je Kostengruppe bekommt den Satz ihrer Kostengruppe.
Gibt es verschiedene Sätze, nennt der Tooltip von #menu[brutto] sie je LV und Kostenermittlung,
das PDF unter „Grundlagen und Annahmen“.

== Kostenvorgabe (Budget)

#menu[Kostenvorgabe (Budget) …] trägt die Kostenvorgabe – das Budget des Bauherrn, DIN 276, 4.6 – je
Kostengruppe netto ein – als *Obergrenze* oder *Zielgröße*,
mit Stand und Grundlage (z. B. „Kostenrahmen des Bauherrn“) – oder übernimmt es mit
#menu[übernehmen] aus einer Kostenermittlung (je KG ihre Summen). Dort stellst du auch die
Schwelle der gelben Markierung ein. Ist die Kostenvorgabe einer Gruppe kleiner als die ihrer
Untergruppen, sagt der Dialog es.

== Kostenermittlungen und Angebote

Von mehreren Kostenermittlungen derselben Stufe zählen die Kosten-LVs der Gewerke zusammen und
von den Datenpunktrechnungen die neueste. #menu[Kostenermittlungen …] ändert die Auswahl; erfassen
zwei geltende Kostenermittlungen dieselbe Kostengruppe, steht ein Hinweis über der Tabelle. Für ein
LV ohne Auftrag wählst du in der Seitenleiste (Ansicht nach Vergabeeinheiten, Klick auf das LV) das
Angebot, das zählen soll, mit Begründung; #menu[Mindestbieter (automatisch)] nimmt die Wahl zurück.

== Erläuterungen, Kostenstand, Positionen ohne Kostengruppe

Je Kostengruppe gehört eine *Erläuterung / Maßnahme* in die Seitenleiste (DIN 276, 4.4.3 und
4.5) – etwa „Angebote über der Kostenberechnung; Maßnahme: Variante prüfen“. Sie steht in der
Tabelle, im PDF und in der CSV.

#menu[Kostenstand festhalten …] speichert die heutige Kette mit Datum und Anlass. Unter
#menu[Vergleich] wählst du einen festgehaltenen Stand: Die Abweichung springt auf #menu[zum Stand], und
jede Stufe zeigt in der zweiten Zeile ihre Änderung seit diesem Stand – eine Stufe, die damals einen
Wert hatte und heute keinen, steht als „–“ mit ihrer Änderung da. Die Seitenleiste nennt je Stufe den
Wert des Stands; Druck und CSV folgen der Wahl (die CSV mit einer Spalte je Stufe des Stands). Im
Dialog #menu[Kostenstand festhalten …] lassen sich alte Stände löschen.

Positionen ohne Kostengruppe zählen in der Zeile „ohne Kostengruppe“; die Seitenleiste listet sie,
#menu[zeigen] springt zur Position im LV.

== CSV und Drucken

#menu[CSV …] schreibt die Tabelle wie gezeigt (Ansicht, Ebene, netto/brutto) mit den Abweichungen
der Prognose und den Erläuterungen – UTF-8 mit BOM oder Windows-1252 wie der LV-Export.
#menu[Drucken …] (auch #menu[Drucken][Kostenverfolgung (DIN 276) …]) druckt das PDF in A4 quer mit
der Ansicht des Reiters: auf Wunsch ein Deckblatt, die Kennzahlen, die Tabelle mit denselben
Hinterlegungen, ein Diagramm der Stufen je Kostengruppe (Kostenvorgabe als Umriss), die Erläuterungen und
Maßnahmen, die Grundlagen und Annahmen und die Positionen ohne Kostengruppe. Jeder Abschnitt ist ein
Lesezeichen; Druckstil, Blattrahmen, Kopf- und Fußzeile gelten wie bei allen Ausdrucken.

#bild("kostenverfolgung-drucken.jpg", breite: 55%)[Druckdialog mit der Kostenverfolgung: Deckblatt, Diagramm, die Ansicht des Reiters]

#bild("kostenverfolgung-pdf.jpg")[Kostenverfolgung als PDF (Stil Klassisch): Kennzahlen und Tabelle]

KI-Assistenten lesen die Kostenverfolgung mit `kostenverfolgung_lesen` und setzen Kostenvorgabe,
Erläuterungen, Auswahl und Kostenstände mit `kostenvorgabe_setzen` und `kostenverfolgung_aendern`
(@mcp).

= Schriftverkehr und Ablage im Projekt <schriftverkehr>

#menu[Angebote][Schriftverkehr (Anschreiben an Bieter) …] (auch im Bieterverzeichnis) schreibt
Briefe an die angefragten Bieter des LV aus Vorlagen mit Platzhaltern in geschweiften Klammern:
*Aufforderung zur Angebotsabgabe*, *Zuschlagsschreiben* und *Absage / Information nach § 134 GWB*
(mit dem Absagegrund aus dem Bieterverzeichnis). Die Empfänger sind je Art vorgeschlagen (alle, der
beauftragte Bieter, die übrigen; #menu[Alle], #menu[Keiner], #menu[Vorschlag]), die Vorschau zeigt den
Brief eines Bieters, #menu[Standardtext wiederherstellen] setzt die Vorlage zurück. #menu[PDF erzeugen …]
öffnet den Druckdialog: Stil, dazu „Ein PDF je Bieter (in einen Ordner)“ statt eines Sammel-PDF und
„Im Projekt ablegen“; #taste[Enter] speichert. Nach der Aufforderung steht das Versanddatum im
Bieterverzeichnis.

*Im Projekt ablegen* (im Druckdialog, Vorgabe an) legt jedes Schreiben zusätzlich als PDF in die Projektdatei –
*je Empfänger ein Eintrag* mit genau dem Brief, den dieser Empfänger bekommen hat, auch wenn du ein
Sammel-PDF druckst. So beantwortet die Ablage „was hat Müller wann bekommen“, lässt sich nach Bieter
filtern und einzeln entfernen. Das Anschreiben „Freigabe an den Bauherrn“ (Abrechnung) hat denselben
Schalter. Ein #taste[Strg+Z] nimmt Ablage und Versanddatum zusammen zurück.

#menu[Angebote][Schriftverkehr im Projekt (abgelegte Schreiben) …] – oder #menu[Abgelegt (n) …] im
Bieterverzeichnis und im Schriftverkehr – zeigt die Ablage des ganzen Projekts, neueste zuerst, mit
Datum, Art, Empfänger, Betreff, LV und Größe; oben Filter nach Art und Empfänger:

- *Doppelklick* oder #menu[Öffnen] zeigt das PDF in der PDF-Anzeige des Systems (aus einer Kopie im
  Temp-Ordner – das Projekt bleibt unberührt; die Kopien löscht proAVA beim Beenden, nach einem Absturz
  beim nächsten Start),
- #menu[Speichern unter …] legt das PDF als Datei ab, z. B. zum Versenden,
- *Bemerkung* je Schreiben (z. B. „per Mail am …“, „Einschreiben“),
- #menu[Entfernen …] (oder #taste[Entf]) löscht das Schreiben mit seinem PDF nach einer Rückfrage,
- #menu[PDF hinzufügen …] legt eigene Schreiben ab (Protokoll, Mail als PDF) als *Eigenes Schreiben*.

#taste[↑]/#taste[↓] wählen, #taste[Enter] öffnet, der Rechtsklick bietet dieselben Befehle; ohne
gewähltes Schreiben sind die Knöpfe ausgegraut. #taste[Strg+Z] und #taste[Strg+Y] wirken in der
Liste auf die Ablage. Die PDFs stecken in der
`.proava`-Datei und damit in jeder Sicherung. Ab 25 MB weist proAVA darauf hin, dass
Projektdatei und Sicherungen damit wachsen.

= Positionsbibliothek <bibliothek>

Die Bibliothek sammelt bewährte Titel und Positionen zum Wiederverwenden. Sie ist ein normales
proAVA-Projekt (`proAVA-Bibliothek.proava` im Home-Ordner, Ort unter #menu[Extras][Einstellungen …]
▸ Neue Projekte). Dort lassen sich auch *weitere Bibliotheken* eintragen – etwa die des Büros auf
dem Netzlaufwerk, schreibgeschützt.

== Seitenfenster „Bibliothek“

#taste[F8] (oder #menu[Ansicht][Bibliothek anzeigen]) blendet neben der LV-Tabelle das
Bibliotheksfenster ein; die Suche hat gleich die Tastatur.

- *Auswahl oben:* eine Bibliothek oder „Alle Bibliotheken“ gemeinsam.
- *Suche:* über Kurztext, Langtext, Unterbeschreibungen, Stichwörter und OZ; mehrere Wörter
  müssen alle vorkommen, Umlaute sind egal („fuehler“ findet „Fühler“). Treffer stehen mit
  ihrem Titelpfad.
- *Filter:* Kostengruppe (auch vom Titel geerbt; „48“ findet 481 bis 489), Positionsart,
  Stichwort.
- *Vorschau:* der Langtext mit Formatierung, Tabellen, Bildern und Textergänzungen, dazu
  Erfahrungspreis mit Datum, Stand und Stichwörter.
- *Einfügen:* Doppelklick, #taste[Enter] (mit #taste[↑]/#taste[↓] aus der Suche heraus) oder
  in die Tabelle ziehen; mehrere Einträge mit #taste[Strg]+Klick. Eingefügt wird hinter der
  Auswahl mit freier OZ und Status „offen“, ein #taste[Strg+Z] nimmt alles zurück. „Mengen
  übernehmen“ ist aus (die Menge kommt aus dem Projekt), „Erfahrungspreise als Planerpreise“ an.

Jede eingefügte Position merkt sich ihren Eintrag (Bibliothek und Stand); das
Eigenschaftenfenster nennt ihn.

== In Bibliothek übernehmen <bibliothek-uebernehmen>

Zeilen im LV wählen – eine Position, mehrere (#taste[Umschalt]/#taste[Strg]+Klick), einen Titel
mit Inhalt, Hinweise oder Ausführungsbeschreibungen –, Rechtsklick ▸
#menu[In Bibliothek übernehmen …]. Derselbe Befehl steht unter #menu[LV][Bibliothek] und im
Seitenfenster unter #menu[Bibliothek …].

#bild("bibliothek-rechtsklick.jpg", breite: 62%)[Drei Positionen gewählt, Rechtsklick: „In
Bibliothek übernehmen …“ steht direkt im Menü]

Der Dialog fragt:

- *In welche Bibliothek?* Die eigene und die weiteren Bibliotheken aus den Einstellungen.
  Grau und nicht wählbar sind schreibgeschützte Bibliotheken, nicht gefundene Dateien und
  Bibliotheken, die gerade in einem anderen proAVA-Fenster offen sind – deren nächstes
  Speichern würde die neuen Einträge wieder überschreiben.
- *Unter welchen Titel?* Ein vorhandener Titel („Ordner“) der Bibliothek oder ein neuer.
  Vorgewählt ist der Titel, der so heißt wie der oberste Titel der Zeilen im LV; gibt es ihn
  noch nicht, steht sein Name als neuer Titel bereit.
- *Stichwörter* (freiwillig, mit Komma getrennt) kommen zu jeder abgelegten Position dazu und
  helfen später bei der Suche im Seitenfenster.

#bild("bibliothek-uebernehmen.jpg", breite: 70%)[In Bibliothek übernehmen: eigene Bibliothek,
schreibgeschützte Bürobibliothek grau, neuer Titel wie im LV, Stichwörter]

*Neue Bibliothek anlegen …:* Gibt es noch keine eigene Bibliothek, ist dieser Punkt vorgewählt –
mit Name und Ort aus den Einstellungen. Name und Ordner lassen sich ändern; #menu[Übernehmen]
legt die Datei an und trägt sie als *eigene Bibliothek* ein. Gibt es schon eine eigene, kommt
die neue zu den *weiteren Bibliotheken* (beschreibbar).

#bild("bibliothek-neu.jpg", breite: 70%)[Noch keine Bibliothek: Name und Ort der neuen]

Das Projekt ändert sich dabei nicht; geschrieben wird nur die Bibliotheksdatei, der Stand der
Einträge ist heute. Die Meldung unten nennt Bibliothek, Anzahl und Titel („In „Bibliothek GA“
abgelegt: 3 Zeilen unter „Automationsstationen“.“). Ihr Knopf #menu[Rückgängig] setzt die
Bibliotheksdatei auf den Stand davor zurück, solange sie nicht inzwischen anders geändert wurde.

== Bibliothek pflegen

Die Befehle stehen unter #menu[LV][Bibliothek] (das Seitenfenster selbst blendet
#menu[Ansicht][Bibliothek anzeigen] ein):

- *GAEB hinein:* #menu[GAEB-Datei in die Bibliothek …] liest Herstellertexte oder Vortexte
  (X81–X86, D81–D86, auch in einer Zip) mit ihrer Gliederung ein.
- *Bearbeiten:* #menu[Bibliothek bearbeiten …] öffnet die Bibliothek wie ein LV: Titel
  („Ordner“) anlegen, umbenennen, verschieben, Texte überarbeiten. Das Eigenschaftenfenster zeigt
  dort zusätzlich *Stichwörter*, *Stand* und *Preis vom* (Datum des Erfahrungspreises).
  Schreibgeschützte Bibliotheken werden nicht geöffnet.
- *Neue Position:* #taste[F9], #taste[Strg+Enter], Rechtsklick oder #menu[Einfügen] legen sie in
  den Titel der Schreibmarke, auf dessen Stufe – ohne neue Untertitel. Die Bibliothek hat vier
  Titelstufen als Platz für Gliederungen aus vielen LVs; nicht genutzte Stufen bleiben in der
  Ordnungszahl 0 (01.001.00.00.0010) und werden im GAEB-Export ausgelassen.
- *GAEB hinaus:* im Seitenfenster unter #menu[Bibliothek …] die ganze Bibliothek oder die
  Auswahl als GAEB X81 ausgeben.

*Eigenes Fenster:* #menu[Bibliothek bearbeiten …] öffnet die Bibliothek in einem *neuen
proAVA-Fenster*. Das Projekt bleibt im ersten Fenster offen, auch mit ungespeicherten
Änderungen. Ist die Bibliothek schon in einem anderen Fenster offen, sagt proAVA das, statt ein
zweites zu öffnen. Unter #menu[Extras][Einstellungen …] ▸ Neue Projekte ▸ Positionsbibliothek lässt
sich das ausschalten; dann speichert proAVA das Projekt und öffnet die Bibliothek im selben
Fenster.

*Woran man die Bibliothek erkennt:* Über der Kopfleiste steht ein grünes Band mit dem Namen der
Bibliothek („Bibliothek: Bibliothek GA“), auch der Fenstertitel nennt sie („proAVA – Bibliothek:
Bibliothek GA“). Rechts im Band führt #menu[Zurück zu …] mit dem Projektnamen zum zuletzt
geöffneten Projekt: Die Bibliothek wird gespeichert
wie beim Schließen, dann öffnet sich das Projekt wieder. Ist das Projekt noch im anderen Fenster
offen, schließt sich stattdessen das Bibliotheksfenster. Das gilt auch, wenn die Bibliothek über
die Startseite geöffnet wurde.

#bild("bibliothek-band.jpg")[Bibliothek in Bearbeitung: grünes Band mit Name und „Zurück zu Demo
Preisspiegel GA“; darunter die Einträge mit Stichwörtern, Stand und Preis vom]

== Mit Bibliothek abgleichen

#menu[LV][Bibliothek][Mit Bibliothek abgleichen …] vergleicht die Positionen des LV, die aus
einer Bibliothek stammen, mit ihrem Eintrag dort – wie der LV-Vergleich: Kurz- und Langtext
(Wortunterschied), Einheit, Positionsart, Textergänzungen, auf Wunsch auch der Einheitspreis.
#menu[Bibliotheksstand übernehmen] bringt die gewählten Positionen auf den Stand der Bibliothek;
Menge, OZ und Status bleiben, fertige Positionen bleiben gesperrt. Ein #taste[Strg+Z] nimmt es
zurück.

= STLB-Bau <stlb>

STLB-Bau-Positionen kommen über eine *GAEB-Datei* ins LV – den offenen, dokumentierten Weg.
Den direkten Aufruf aus der AVA gibt es nur über eine Partnerschnittstelle von DBD/DIN
Bauportal; proAVA hat sie nicht.

+ Die Zeile wählen oder rechts anklicken, unter der die Positionen eingefügt werden sollen, und
  #menu[Einfügen][Neue Position aus STLB-Bau …] öffnen (#taste[F5], Menüleiste oder Rechtsklick). Ein
  Platzhalter ist dafür nicht nötig; STLB-Bau-Platzhalter aus Projekten oder GAEB-Dateien füllt
  der Dialog, wenn einer gewählt ist.
+ Im Dialog #menu[STLB-Bau Online öffnen] zeigt STLB-Bau
  Online im Browserfenster von proAVA (wie bei AUSSCHREIBEN.DE, Kapitel „Herstellertexte von
  AUSSCHREIBEN.DE“). Oben auf der Seite „Login“: Die Anmeldeseite des DIN-Bauportals öffnet sich im
  selben Fenster. Mit „Angemeldet bleiben“ merkt sich das Fenster die Anmeldung auch über das
  Schließen hinaus.
+ In STLB-Bau die Positionen bilden und als GAEB-Datei speichern (STLB-Bau Online:
  Leistungsverzeichnis → Export GAEB). Die Datei aus dem Browserfenster – oder eine neue Datei im
  Download-Ordner – übernimmt der Dialog von selbst („Neue Dateien werden sofort übernommen“ über dem
  Knopf); sonst #menu[GAEB-Datei wählen …].
+ Positionen auswählen, #menu[Übernehmen]. Die Positionen kommen unter die gewählte Zeile; ein
  gewählter Platzhalter wird gefüllt (seine Menge bleibt). Der Dialog bleibt für die nächste Position offen.

Jede Datei wird *einmal* übernommen – von selbst oder per Knopf. Danach meldet der Dialog
„3 Positionen übernommen – Strg+Z nimmt es zurück“, die Liste zeigt „bereits übernommen“, und der Knopf
heißt grau *Übernommen*, bis eine neue Datei kommt oder weitere Einträge gewählt sind. #taste[Strg+Z]
nimmt die Übernahme auch bei offenem Dialog zurück. Kommen dieselben Positionen noch einmal (dieselbe
Datei erneut gewählt oder heruntergeladen), fragt proAVA „Diese Positionen wurden bereits übernommen –
noch einmal?“; sind auch neue dabei, übernimmt #menu[Nur neue übernehmen] nur diese. Wer dieselbe Position
bewusst zweimal braucht, nimmt den kleinen Knopf #menu[Noch einmal übernehmen] neben *Übernommen*. Das gilt
genauso für AUSSCHREIBEN.DE und eine auf das LV gezogene GAEB-Datei.

Die Spalte *Übernehmen als* bestimmt je Eintrag die Zeilenart. Eine *Standardbeschreibung* liefert
STLB-Bau Online als Position ohne Einheit und Menge; proAVA schlägt dafür „Hinweis (Text)“ vor.
Hinweistexte und Ausführungsbeschreibungen mit STLB-Bau-Kennung aus der Datei kommen als solche.
Ein Platzhalter wird dann zu dieser Zeilenart. Später umstellen geht mit „Zeilenart umwandeln“
(@umwandeln).

#bild("stlb.jpg", breite: 75%)[Aus STLB-Bau übernehmen]

Die *STLB-Bau-Kennung* (Katalog, Version, Leistungsbereich, Merkmale) und die Textergänzungen
bleiben an jeder Position zeichengleich erhalten und gehen beim GAEB-Export wieder hinaus.
Textergänzungen sind im Langtext als markierte Stellen zu sehen.

*Prüfen und aktualisieren:* #menu[Prüfen][STLB-Bau-Positionen zum Prüfen ausgeben (X81) …] schreibt
alle Positionen mit Kennung als X81 für STLB-Bau. Was STLB-Bau daraus aktualisiert zurückgibt,
liest #menu[Prüfen][Aktualisierte STLB-Bau-Positionen einlesen …]: Die Liste zeigt je Position die
Unterschiede (Zuordnung über die OZ, sonst über die Leistung); übernommen werden Kurz- und
Langtext, Einheit, Textergänzungen und Kennung, OZ, Menge und Preise bleiben, fertige Positionen
nicht. #menu[Prüfen][LV prüfen …] warnt vor dem Export bei offenen Platzhaltern, leeren Textergänzungen
und fehlenden Zahlenwerten.

== Schreibschutz und freie Position <stlb-sperre>

Eine Position mit STLB-Bau-Kennung ist ein Text aus dem Katalog: *Langtext* (Text, Formatierung,
Bilder, neue Angaben) und *Einheit* sind schreibgeschützt. Frei bleiben Kurztext, Menge,
Mengenermittlung, Einheitspreis, Kostengruppe, Leistungsbereich, Status, Positionsart, der Bezug auf
eine Ausführungsbeschreibung und die *Werte der Textergänzungen* – sie stehen im Bearbeitungsbereich
und im Fenster Eigenschaften über dem Langtext unter „Werte der Textergänzungen (frei)“.

Bearbeitungsbereich und Fenster Eigenschaften zeigen über dem Langtext die Kennungszeile, etwa
„STLB-Bau 10/2026 · LB 070 · Leistung 3578 · KG 482 · 8 Merkmale“; in der LV-Tabelle zeigt die
Markierung *STLB-Bau* sie als Tooltip. Gesperrte Felder sind grau; ein Klick darauf erklärt in der
Statuszeile, wie es weitergeht. Ersetzen, Rechtschreibkorrektur und das Einfügen von Bildern lassen
Langtext und Einheit dieser Positionen aus und nennen sie in der Meldung.

Den Text ändern geht auf zwei Wegen:

- *In STLB-Bau:* über „Prüfen und aktualisieren“ (oben) – die Kennung bleibt.
- *Als freie Position:* #menu[In freie Position umwandeln …] über dem Langtext, im Rechtsklick bzw.
  in der Menüleiste #menu[Bearbeiten][Umwandeln in][Freie Position (ohne STLB-Bau-Kennung) …] oder
  über die Befehlssuche. Nach der Rückfrage „Die STLB-Bau-Kennung wird entfernt, danach ist der
  Langtext frei änderbar.“ ist die Position ein freier Text; die Textquelle vermerkt „in freie
  Position umgewandelt am …“ mit der alten Kennung. Mehrere gewählte Positionen gehen in einem
  Schritt, #taste[Strg+Z] nimmt es zurück.

Weicht der Langtext einer Position trotzdem von STLB-Bau ab (Markierung *STLB-Bau\**, etwa aus
einer anderen Software), meldet #menu[Prüfen][LV prüfen …] „Langtext weicht von STLB-Bau ab –
umwandeln oder aus STLB-Bau aktualisieren“ und bietet „STLB-Bau\* in freie Positionen umwandeln“ an.
Der GAEB-Export schreibt solche Positionen als freien Text ohne STLB-Bau-Kennung und nennt sie im
Protokoll. Ein geänderter Kurztext allein behält die Kennung; STLB-Bau Check führt ihn in der Spalte
„Kurztext-Änderung“.

STLB-Bau-Platzhalter sind nicht gesperrt. Kopieren, Einfügen, Bibliothek und Übernehmen aus anderen
Projekten nehmen Kennung und Schreibschutz mit.

= Herstellertexte von AUSSCHREIBEN.DE <ausschreiben>

AUSSCHREIBEN.DE ist ein kostenloses Portal für Herstellertexte (ohne Anmeldung). Die Texte dürfen
für Ausschreibungen übernommen und geändert werden; weitergeben außerhalb von Ausschreibungen
nicht. Der Weg ist derselbe wie bei STLB-Bau: eine *GAEB-Datei* aus dem Browser – ab Werk aus dem
*Browserfenster von proAVA* (siehe unten), sonst aus dem System-Browser.

+ Eine Zeile wählen oder rechts anklicken, unter der eingefügt werden soll (oder einen
  Platzhalter, der gefüllt werden soll), dann #menu[Einfügen][Neue Position aus AUSSCHREIBEN.DE …]
  (#taste[F6]) – in der Menüleiste, im Rechtsklick unter #menu[Einfügen] und in der Befehlssuche.
+ *Suchen:* Hersteller und Begriffe eingeben, z. B. „Belimo Drehantrieb 20 Nm“. proAVA liest
  dazu die Katalogverzeichnisse, die AUSSCHREIBEN.DE veröffentlicht, und zeigt Titel,
  Artikelnummer und Ordner. Ohne Hersteller im Suchbegriff gibt es keine Treffer – eine
  Volltextsuche über alle Kataloge bietet nur die Website. Die Verzeichnisse werden 7 Tage
  zwischengespeichert.
+ #menu[Öffnen] am Treffer zeigt den Text im Browserfenster von proAVA – oder
  #menu[AUSSCHREIBEN.DE öffnen] und dort selbst suchen. Auf der Seite des Texts als *GAEB*
  exportieren (mehrere Texte: auf die Merkliste setzen und gesammelt exportieren).
+ Die exportierte Datei kommt direkt in den Dialog und wird von selbst übernommen; weitere
  Dateien folgen hinter der letzten Übernahme. Mit #menu[Ganze Datei in die Bibliothek …]
  kommen alle Texte der Datei in die eigene Bibliothek.

*Das Browserfenster:* ein eigenes Fenster von proAVA, das die Website unverändert zeigt – mit
einer Leiste darüber: Zurück, Vor, Neu laden, Startseite, die Adresse, #menu[Im System-Browser
öffnen], #menu[Abmelden] und rechts das *Ziel im LV* (wohin die nächste Datei kommt) sowie die
letzte Übernahme. Ein Klick auf einen Link zu einer anderen Website (etwa die Seite des
Herstellers) öffnet den System-Browser; die Anmeldeseite des Anbieters bleibt im Fenster.
Anmeldung, Cookie-Auswahl und Merkliste merkt sich das Fenster in einem eigenen Profil, das nur
der eigene Benutzer lesen kann; Zugangsdaten speichert proAVA nicht. #menu[Abmelden] löscht nach
einer Rückfrage in der Leiste Anmeldung, Cookies und gespeicherte Daten dieser Website im Fenster
(auch eine Merkliste ohne Konto). Andere Downloads (Datenblätter, PDF) landen im
Download-Ordner. Das Fenster schließt sich mit dem Dialog; es läuft höchstens einmal. proAVA klickt, sucht oder exportiert dort
nichts selbst.

Wer lieber den gewohnten Browser nimmt: #menu[Extras][Einstellungen …] ▸ Neue Projekte ▸
Textquellen ▸ „Im System-Browser“. Dann sammelt man Texte mit Häkchen und
#menu[Gewählte im Browser öffnen]; der Dialog beobachtet den Download-Ordner und übernimmt eine
neue GAEB-Datei ebenso von selbst. Ohne installiertes Browserfenster (`proava-web` neben
`proava`) gilt immer der System-Browser.

Der Text bleibt, wie ihn der Hersteller liefert – Herstellername und „oder gleichwertig“
inklusive. Die *Textquelle* (Quelle, Hersteller, Datum, Datei, Artikelnummer) steht im Fenster
der Position unter „Weitere Angaben“; sie geht nicht in GAEB oder PDF. #menu[Prüfen][LV prüfen …]
erinnert bei Herstellertexten daran, die Produktneutralität zu prüfen (VOB/A § 7 Abs. 2).

*GAEB-Dateien ziehen:* Eine GAEB-Datei oder ZIP aus dem Dateimanager oder der Download-Liste des
Browsers auf die LV-Tabelle ziehen öffnet denselben Dialog; eingefügt wird hinter der Zeile, auf
der die Datei abgelegt wurde. Texte oder Links aus der Webseite selbst kann proAVA nicht
annehmen – dafür den Export-Knopf auf der Seite nutzen.

= CSV importieren und exportieren <csv>

#menu[GAEB][CSV importieren …] liest eine Tabelle, wie Excel oder LibreOffice sie als CSV
speichern – etwa eine Zulieferung oder eine selbst gepflegte Liste. Das Format:

```
OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:
01.481.01.0010;Raumregler;St.;1;10;10;481;Position
;Langtext erster Absatz;;;;;;
;Langtext zweiter Absatz;;;;;;

01.481.01.0010;Kabeltrassen;;;;;481;Ausführungsbeschreibung
;Langtext;;;;;;
01.481.01.0010;Vorbemerkung;;;;;481;Text
;Langtext;;;;;;
```

- Eine Zeile mit OZ beginnt einen Eintrag; die Zeilen darunter, die nur einen Positionstext
  haben, sind sein *Langtext* (je Zeile ein Absatz). Leerzeilen zwischen den Einträgen sind
  erlaubt, aber nicht nötig – proAVA erkennt die Einträge selbst.
- Die Kopfzeile ist freiwillig (mit oder ohne Doppelpunkt); proAVA ordnet die Spalten nach
  ihren Namen zu, die Reihenfolge ist frei, fehlende Spalten bleiben leer. Ohne Kopfzeile gilt die
  Reihenfolge oben. Trenner Semikolon, Komma oder Tabulator, Zeichensatz UTF-8 oder Windows-1252
  und deutsche Zahlen („1.234,56“) erkennt proAVA selbst.
- *Positionsart*: Position (auch „Bedarfsposition mit GB“/„ohne GB“, Grund-, Wahl-, Zulageposition,
  „Pauschal“, „Stundenlohn“, mehrere mit Komma), Ausführungsbeschreibung, Text oder Hinweis,
  Vorbemerkung, Titel. Ohne Positionsart werden OZ mit weniger Stufen als die Positionen zu Titeln.
  Titel, die nur in den OZ vorkommen (01, 01.481, 01.481.01), legt proAVA an – die Bezeichnung
  danach ergänzen. Die OZ-Maske folgt den OZ der Datei.
- *GP-Prüfung*: Passt der GP nicht zu Menge × EP (auf den Cent), gilt der errechnete GP. Die
  Zeile ist dann *orange hinterlegt*, in der Spalte Status steht „GP prüfen“, der Tooltip nennt
  „GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €“. Vorschau und Importprotokoll
  listen jede Abweichung und die Summe aller Abweichungen. Die Marke bleibt (auch nach dem
  Speichern), bis Menge oder EP geändert werden oder #menu[GP-Abweichung bestätigen] sie
  wegnimmt (Kontextmenü #menu[Status], #menu[Bearbeiten][Bearbeitungsstatus], Eigenschaftenfenster oder
  Befehlssuche; für einen Titel gilt es für alle Positionen darunter). Die Fußzeile zählt „n GP
  prüfen“, die Offenen Punkte führen sie auf.
- *Währung*: im Dialog Euro, US-Dollar oder Schweizer Franken (Vorgabe: die in der Datei
  genannte, sonst Euro). Sie wird die Währung des neuen LV – GAEB-Export, Druck und Summen
  folgen ihr.

Ziele und Importprotokoll sind dieselben wie beim GAEB-Import (neues LV, neues Projekt, ins
aktive LV, Bibliothek). Die Vorschau zeigt die erkannten Spalten, Titel, Positionen,
Ausführungsbeschreibungen und Hinweise, die Summe und die Abweichungen.

#bild("csv-import.jpg", breite: 62%)[CSV importieren: Vorschau mit Spalten, GP-Prüfung und Währung]

#bild("csv-pruefmarke.jpg")[Positionen mit abweichendem GP: orange, Status „GP prüfen“, Tooltip mit der Abweichung]

#menu[GAEB][Als CSV exportieren …] schreibt das aktive LV im selben Format: Titel,
Positionen, Ausführungsbeschreibungen und Hinweise je mit einer Zeile, darunter der Langtext je
Absatz eine Zeile, eine Leerzeile zwischen den Einträgen, die Kostengruppe, wie sie gilt.
Vorgabe ist UTF-8 mit BOM – so öffnen Excel und LibreOffice die Umlaute richtig; das Kästchen
*Windows-1252* ist für ältere Programme; proAVA merkt sich die Wahl (auch unter
#menu[Extras][Einstellungen …] ▸ Drucken, Abschnitt GAEB-Export). Formatierung, Bilder und
Unterbeschreibungen fallen weg, proAVA nennt nach dem Export, was weggefallen ist. Export und
erneuter Import ergeben dasselbe LV (bis auf diese Dinge).

#hinweis(titel: "Tabellenprogramme und OZ")[LibreOffice und Excel halten „01“ oder „01.481“
beim Öffnen gern für Zahlen und machen daraus 1 bzw. 1481. Beim Öffnen die Spalte *OZ* als
*Text* einstellen (LibreOffice: Spaltentyp im Importdialog; Excel: #menu[Daten][Aus Text/CSV]),
dann bleiben die Ordnungszahlen erhalten. Als Trenner Semikolon wählen. Fehlende führende
Nullen („1“ statt „01“) ergänzt der CSV-Import selbst und nennt es im Protokoll; aus „01.481“
gewordenes „1481“ lässt sich nicht zurückholen.]

= Sichern und Wiederherstellen <sicherung>

- *Speichern:* #taste[Strg+S]. proAVA schreibt erst in eine Zwischendatei und ersetzt dann
  das Projekt – ein Absturz beim Speichern zerstört nichts.
- *Schließen mit ungespeicherten Änderungen* (#menu[Start], #menu[Projekt][Projekt schließen],
  Fenster schließen – auch aus der Fensterliste, wenn proAVA minimiert ist): proAVA fragt.
  *Speichern und zurück* bzw. *Speichern und beenden* speichert und schließt, *Verwerfen*
  schließt ohne die Änderungen, *Abbrechen* (oder #taste[Esc]) bleibt im Projekt. Lässt sich
  nicht speichern, bleibt die Frage offen und nennt den Grund.
- *Automatisch sichern:* Bei ungespeicherten Änderungen schreibt proAVA alle 5 Minuten
  (einstellbar 1–120) eine Wiederherstellungsdatei `.Name.proava.autosave` neben das
  Projekt. Ist sie beim nächsten Öffnen neuer als das Projekt (proAVA wurde nicht regulär
  beendet), fragt proAVA: #menu[Wiederherstellen] oder #menu[Verwerfen].
- *Sicherung anlegen:* #menu[Projekt][Sicherung anlegen] (oder auf der Startseite) speichert
  vorher und legt `Name_Sicherung_<Datum>_<Uhrzeit>.zip` in den Projektordner. Auf Wunsch
  auch bei jedem Schließen (Einstellungen).
- *Sicherung wiederherstellen:* auf der Startseite; das Projekt kommt in einen Ordner deiner
  Wahl, die Sicherung bleibt unverändert.
- Die *Ablage des Schriftverkehrs* (@schriftverkehr) steckt in der Projektdatei und damit in jeder
  Sicherung.

= Einstellungen <einstellungen>

#menu[Extras][Einstellungen …] (auch das Zahnrad oben rechts und auf der Startseite) gilt für
alle Projekte:

#table(
  columns: (auto, 1fr),
  table.header[Reiter][Inhalt],
  [Darstellung], [Hell/Dunkel, Schriftgröße der Oberfläche (70–200 %, auch #taste[Strg]+Plus/Minus), Zeilen- und Absatzabstand der Texte (gilt für Texteditor und Druck), Eigenschaftenfenster links/rechts und sichtbar, Positions- und Langtextfenster vor dem Hauptfenster, Titelzeilen farbig hinterlegen, Positionen ohne Menge markieren, Rechtschreibung (ein/aus, Wörterbuch-Ordner), Vorschau der PDF-Ausschnitte, KI-Assistenten (geöffnete Fenster bedienen, @ki-fenster)],
  [Neue Projekte], [Standardordner, OZ-Maske (ab Werk `11.222.PPPPI`), Schrittweite, USt neuer LVs, eigene Positionsbibliothek und weitere Bibliotheken (Name, Pfad, schreibgeschützt), „Bibliothek bearbeiten …“ im eigenen Fenster, Ordner der Projektvorlagen, Textquellen (AUSSCHREIBEN.DE und STLB-Bau Online im Browserfenster von proAVA oder im System-Browser)],
  [Speichern], [automatisch sichern und Abstand, Sicherung beim Schließen],
  [Preisspiegel], [Ausreißergrenze, Bezugsgröße der Abweichung, Aufgreifschwelle],
  [Drucken], [Aufsteller (Planer) für neue Projekte, Bearbeiter, Logo, Druckstil und Akzentfarbe, Blattrahmen, Tabellen voll umrandet (Gitter), Deckblatt auch beim LV mit Preisen, Blöcke des Deckblatts; GAEB-Export: Bilder mitgeben, CSV in Windows-1252],
  [Kopf-/Fußzeile], [drei Felder je Seite mit Platzhaltern, Linien, Logo-Platz, Schriftgröße, Vorschau],
)

*Zeilen- und Absatzabstand der Texte* (Reiter Darstellung): Zeilenabstand einfach, 1,15 oder 1,5 und
Abstand nach Absatz kein, 3 pt, 6 pt oder halbe Zeile. Die Einstellung gilt für den Texteditor und für
alle Ausdrucke gemeinsam, damit beide gleich aussehen. Vorgabe ist „einfach“ ohne Abstand nach Absatz,
wie in AVA-Ausdrucken üblich: Zeilen eines Absatzes und aufeinanderfolgende kurze Absätze (etwa
„Art: …“, „Tiefe: …“) stehen im selben Abstand, eine Leerzeile ist ein leerer Absatz im Text. Ein
Absatzformat je Absatz gibt es nicht, GAEB kennt keines.

#bild("einstellungen.jpg", breite: 62%)[Einstellungen, Reiter Darstellung]

= MCP-Server für KIs <mcp>

`proava-mcp` ist ein MCP-Server (Model Context Protocol) über stdin/stdout. Damit lesen und
schreiben KI-Assistenten in einem MCP-fähigen Programm Leistungsverzeichnisse direkt in
`.proava`-Dateien – mit denselben Regeln wie das Programm: OZ-Maske, Summen, Positionsarten,
Status-Sperre, Preisspiegel.

- *Lesen:* Projekte suchen, Projekt und LVs lesen, LV und Positionen (mit Langtext),
  LV prüfen, Rechtschreibung prüfen, Angebote und Preisspiegel, Schriftverkehr.
- *Positionen aus anderen LVs holen:* `lv_suchen` findet Positionen in einem Projekt, in allen
  Projekten unter einem Ordner oder in einer GAEB-Datei (Kurz- und Langtext, Filter
  Kostengruppe und Einheit); `positionen_uebernehmen` fügt die gefundenen OZ oder Zeilen ein –
  so geht „Hole die Positionen für die Raumbediengeräte aus dem LV von … und füge sie hier ein“.
- *Schreiben:* Projekt und LV anlegen, Stammdaten setzen, Titel/Positionen/Hinweise einfügen,
  ändern, löschen, verschieben, neu nummerieren, STLB-Bau übernehmen, Angebote einlesen,
  Nachlass, GAEB-Export (auch GAEB 90), CSV-Import und -Export, PDF wie im Druckdialog
  (`pdf_drucken` mit `lv_ohne_preise`, `lv_mit_preisen` …), Auftrag erteilen, Auftrags-LV,
  Preise übernehmen, Anschreiben.
- *Abrechnung:* Auftrag festschreiben, Rechnungen anlegen (auch aus X89), Positionen und
  Aufmaß setzen, Freigabe berechnen, Aufmaßprüfung, Rechnungsfreigabe und Anschreiben drucken.
- *Nachträge:* lesen (mit Prüfhilfe), anlegen, ändern, Positionen setzen, Status, PDF,
  Nachtrags-LV als X83/X86, Nachtragsangebot einlesen.
- *Kosten:* Kostenermittlungen lesen, anlegen und ändern; Kostenverfolgung lesen (Kette je
  Kostengruppe oder Vergabeeinheit), Kostenvorgabe setzen, Erläuterungen, Auswahl der Kostenermittlungen
  und Angebote, Kostenstand festhalten; PDF mit `pdf_drucken` `kostenverfolgung`.
- *Langtexte* schreibt die KI in einer einfachen Auszeichnung (`**fett**`, `- Aufzählung`,
  Unterpunkte mit zwei Leerzeichen mehr je Ebene, `| Tabelle |` …); Bilder fügt nur das
  Programm ein.

Zur Sicherheit: Schreibende Werkzeuge sind als freigabepflichtig beschrieben – stell sie im
MCP-Programm auf Rückfrage. Das erste
Schreiben je Projekt legt vorher eine Zip-Sicherung neben das Projekt.
`PROAVA_MCP_ORDNER` begrenzt die Zugriffe auf bestimmte Ordner. Hat sich die Datei
zwischendurch geändert, speichert der Server nicht.

#hinweis(titel: "Wichtig")[Ist ein Projekt in proAVA offen, arbeitet die KI am besten *im
Fenster* (@ki-fenster). Die Werkzeuge, die direkt in die Datei schreiben, lehnen ab, solange
das Projekt in einem Fenster ungespeicherte Änderungen hat – proAVA würde sie sonst beim
nächsten Speichern überschreiben.]

== KI-Assistenten in geöffneten Fenstern <ki-fenster>

Statt in der Datei kann die KI in den geöffneten proAVA-Fenstern arbeiten. Sie sucht, zeigt
und fügt ein über dieselbe Bearbeitung wie von Hand: Was sie einfügt, nimmt #taste[Strg]+#taste[Z]
in einem Schritt zurück, gespeichert wird erst, wenn du speicherst, und ungespeicherte
Änderungen im Fenster bleiben unberührt. So geht zum Beispiel:

- „Im rechten Fenster brauche ich die Position für den Schaltschrank – zeig sie mir und kopiere
  sie links an die Stelle, wo die Schreibmarke steht.“ Die KI sucht rechts, wählt und markiert
  die gemeinte Position, wartet auf dein Ja und fügt sie links hinter der gewählten Zeile ein –
  mit Langtext, Bildern, Bieterangaben, Ausführungsbeschreibung, Kostengruppe und STLB-Bau-Kennung.
- „Ich brauche das LV aus dem Projekt Schule Musterstadt, such nach Schaltschrank.“ Die KI
  öffnet das Projekt in einem neuen Fenster (auf Wunsch schreibgeschützt), sucht dort und zeigt
  die Treffer; „die markierte und die nächsten drei bitte rüberkopieren“ fügt sie im Hauptfenster
  ein.

Die Fenster heißen für die KI *links* und *rechts* (nach ihrer Lage auf dem Bildschirm),
*aktiv* (das zuletzt benutzte) oder nach dem Projektnamen.

Auch alles andere, was die KI an einem geöffneten Projekt ändert – Positionen anlegen oder ändern,
Bilder und Planausschnitte einfügen, Stammdaten, Kosten, Angebote, Nachträge, Abrechnung –, landet
im Fenster und nicht in der Datei: als *ein* Schritt, den #taste[Strg]+#taste[Z] zurücknimmt, mit
markierten Zeilen und ungespeichert wie nach deiner eigenen Arbeit. Hast du in der Zwischenzeit
selbst etwas geändert, übernimmt das Fenster nichts, und die KI fragt neu nach. Speichern tust du
(#taste[Strg]+#taste[S]) – oder die KI auf deine Bitte; vor dem ersten Speichern legt sie eine
Sicherung neben die Datei.

#bild("ki-fenster.jpg")[Rechts gesucht und gezeigt, links an der Schreibmarke eingefügt: die
Zeilen, an denen die KI arbeitet, sind violett umrandet, die Statusleiste sagt „KI: …“]

Was du siehst:

- *KI-Markierung:* Die Zeilen, an denen die KI gerade arbeitet, sind violett umrandet; die
  Statusleiste nennt in derselben Farbe, was sie tut („KI: 6 Treffer für „Schaltschrank““, „KI:
  hat 1 Position aus Schule Musterstadt eingefügt …“). Die Markierung verschwindet nach einer
  Weile von selbst oder sobald die Zeilen nicht mehr da sind (#taste[Strg]+#taste[Z]).
- *KI verbunden* steht rechts in der Statusleiste, solange eine KI in den letzten Minuten in
  diesem Fenster gefragt hat.
- *#taste[Esc]* bricht ab: Markierung und Statuszeile verschwinden, und für einige Sekunden lehnt
  das Fenster die KI ab – sie muss dich fragen, bevor sie weitermacht.

Was die KI nicht anders darf als du: In einem schreibgeschützt geöffneten Fenster fügt sie
nichts ein. Ist ein Dialog offen, wartet sie. Ist das LV schon versandt, kommt die übliche
Rückfrage („LV bereits versandt“); ebenso, wenn das LV schon Angebote oder einen Auftrag hat
(„LV hat bereits Angebote“) – deine Antwort gilt; die KI erfährt, ob du es behalten oder
zurückgenommen hast. Fertige Positionen bleiben gesperrt.

Abschalten: #menu[Extras][Einstellungen …] ▸ Darstellung ▸ *KI darf geöffnete Fenster bedienen*
(ab Werk an). Aus: das Fenster antwortet der KI nur mit einem Hinweis, und an einem dort geöffneten
Projekt ändert die KI nichts. Die Verbindung ist nur
für Programme deines Benutzers auf diesem Rechner erreichbar (unter Linux ein Kanal im
persönlichen Laufzeitordner, unter Windows eine Named Pipe nur für dich), nicht über das Netz.
Unter Windows 11 nur kurz erprobt (Start, Projekt laden); nicht umfassend getestet.

= Tastenkürzel <tasten>

Diese Liste entsteht aus denselben Befehlsdefinitionen wie Menüs, Befehlssuche und die Hilfe
im Programm (#taste[F1]) – sie stimmt also mit dem Programm überein. Keine Taste ist doppelt
belegt.

== Funktionstasten <funktionstasten>

Die Funktionstasten stören nie beim Tippen und liegen in Gruppen zu je vier Tasten:

#table(
  columns: (auto, auto, 1fr),
  table.header[Gruppe][Taste][Befehl],
  table.cell(rowspan: 4)[F1–F4 \ Hilfe und Bearbeiten],
  [#taste[F1]], [Hilfe zur aktuellen Ansicht],
  [#taste[F2]], [Zelle bearbeiten (#taste[Umschalt+F2]: Langtext im eigenen Fenster)],
  [#taste[F3]], [Weitersuchen],
  [#taste[F4]], [Leere Mengen finden],
  table.cell(rowspan: 4)[F5–F8 \ Texte holen und prüfen],
  [#taste[F5]], [Neue Position aus STLB-Bau …],
  [#taste[F6]], [Neue Position aus AUSSCHREIBEN.DE …],
  [#taste[F7]], [Rechtschreibung prüfen …],
  [#taste[F8]], [Bibliothek ein-/ausblenden],
  table.cell(rowspan: 4)[F9–F12 \ Neue Zeilen],
  [#taste[F9]], [Neue Position darunter; #taste[Umschalt+F9] davor],
  [#taste[F10]], [Neuer Hinweis darunter; #taste[Strg+F10] davor (#taste[Umschalt+F10] öffnet wie üblich das Kontextmenü)],
  [#taste[F11]], [Neue Ausführungsbeschreibung],
  [#taste[F12]], [Neuer Titel; #taste[Umschalt+F12] neuer Untertitel],
)

#bild("hilfe-f1-lv.jpg", breite: 70%)[#taste[F1] in der LV-Ansicht: Tastenkürzel mit Suche]

#let kuerzel = csv("tastenkuerzel.csv", delimiter: ";").slice(1)
#for bereich in kuerzel.map(z => z.at(0)).dedup() [
  == #bereich
  #table(
    columns: (1fr, auto),
    table.header[Befehl][Taste],
    ..kuerzel
      .filter(z => z.at(0) == bereich)
      .map(z => (z.at(1), z.at(2).split(" oder ").map(taste).join[ oder ]))
      .flatten()
  )
]

= Lizenz, Haftung und Namen Dritter <lizenz>

*Lizenz.* © 2026 Oliver Hagmann. proAVA ist quelloffen und kostenlos unter der *Business
Source License 1.1* (BSL 1.1). Die BSL ist keine von der OSI anerkannte Open-Source-Lizenz; proAVA ist darum
quelloffen, aber nicht „Open Source“. Verbindlich ist allein der englische Lizenztext unten
(auch als Datei `LICENSE` neben dem Programm, unter Windows `LICENSE.txt`, und unter
#menu[Hilfe][Über proAVA …], dort #menu[Lizenz anzeigen]). Kurz zusammengefasst:

- *Erlaubt:* proAVA für eigene geschäftliche Zwecke nutzen, auch kommerziell – in
  Planungsbüros, bei Bauherren und öffentlichen Auftraggebern, bei Bietern und Firmen für ihre
  Angebote. Dienstleistungen rund um proAVA anbieten (Schulung, Einrichtung, Beratung,
  Anpassung). proAVA als Quelltext oder Programm weitergeben.
- *Nicht von der Erlaubnis umfasst:* proAVA oder ein davon abgeleitetes Werk als
  eigenständiges Produkt oder als Hauptbestandteil eines kostenpflichtigen Softwarepakets
  verkaufen; proAVA oder ein davon abgeleitetes Werk als gehosteten oder verwalteten Dienst
  (SaaS) anbieten.
- *Abwandlungen* bleiben unter dieser Lizenz (nach dem Change Date unter der Change License).
- *Später:* Ab dem 20.11.2030 gilt für diese Version die GNU Affero General Public License
  v3.0 oder später.

Die Lizenzen der Bestandteile Dritter (Bibliotheken, Schriften) stehen in `THIRD-PARTY-NOTICES`
und unter #menu[Hilfe][Über proAVA …], dort #menu[Alle Lizenzhinweise anzeigen].

*Haftung.*

#gewaehrleistung

*Unter Windows 11 nur kurz erprobt (Start, Projekt laden); nicht umfassend getestet.*

*Namen und Normen Dritter.* STLB-Bau, GAEB, AUSSCHREIBEN.DE und DIN 276 sind Namen bzw. Normen
Dritter. proAVA ist mit diesen Stellen nicht verbunden und von ihnen nicht autorisiert.

== Lizenztext (Business Source License 1.1)

#block(width: 100%)[
  #set text(size: 7.5pt)
  #set par(justify: false)
  #raw(read("/LICENSE"), block: true)
]
