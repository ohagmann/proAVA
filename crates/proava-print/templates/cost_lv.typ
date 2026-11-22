// Kostenermittlung als Kurztext-LV (Kosten-LV) nach DIN 276:
// Angaben, Zusammenstellung nach Kostengruppen, Kurztext-Liste und je
// Datenpunkt-Abschnitt (GU-Fall) ein Anhang „Datenpunkte“.
// All texts and amounts come formatted from /data.json.
#import "/common.typ": *
#import "/points.typ": point-summary, point-sections

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd, landscape: data.landscape)

#title-block(hd, extra: data.facts.map(f => (f.at(0), f.at(1))))

// Kräftige Linie unter Tabellenköpfen (Klassisch schwarz, proAVA Akzentfarbe).
#let head-rule = 1pt + accent-rule
#let head(body, ..args) = table.cell(..args, fill: title-fill, text(weight: "bold", fill: heading, hyphenate: false, body))

// Section heading: tinted band, PDF-Lesezeichen.
#let band(title) = block(
  width: 100%,
  above: 5mm,
  below: 3mm,
  fill: title-fill,
  stroke: if classic { none } else { (left: 3pt + accent) },
  inset: (x: 7pt, y: 5pt),
  breakable: false,
  {
    bookmark-at(1, title)
    text(size: 10.5pt, weight: "bold", fill: heading, title)
  },
)

// ---------- Angaben und Hinweise ----------
#for note in data.notes {
  block(breakable: false, below: 3mm, {
    if note.heading != "" {
      text(weight: "bold", fill: heading, note.heading)
      v(1pt)
    }
    for p in note.paragraphs {
      par(p)
    }
  })
}

// ---------- Zusammenstellung nach Kostengruppen ----------
#band("Zusammenstellung nach Kostengruppen (DIN 276)")
#set text(size: 9pt)
#table(
  columns: (16mm, 1fr, 30mm, 16mm),
  stroke: table-stroke(),
  inset: (x: 3pt, y: 3.5pt),
  table.header(
    head("KG"),
    head("Bezeichnung"),
    head("Betrag", align: right),
    head("Anteil", align: right),
    table.hline(stroke: head-rule),
  ),
  ..data.groups.map(g => {
    let fill = if g.level == 1 { sum-fill } else { none }
    let indent = if g.level >= 2 { (g.level - 1) * 4mm } else { 0mm }
    let body(v) = if g.level == 1 { strong(v) } else { v }
    (
      table.cell(fill: fill, body(g.code)),
      table.cell(fill: fill, pad(left: indent, body(g.label))),
      table.cell(fill: fill, align: right, body(g.amount)),
      table.cell(fill: fill, align: right, text(fill: muted, g.share)),
      table.hline(stroke: thin),
    )
  }).flatten(),
  table.hline(stroke: head-rule),
  [], strong("Summe netto"), table.cell(align: right, strong(data.net)), [],
  [], data.vat_label, table.cell(align: right, data.vat), [],
  [], strong("Summe brutto"), table.cell(align: right, strong(data.gross)), [],
)

// ---------- Kurztext-Liste ----------
#if data.list and data.items.len() > 0 {
  band("Kurztext-Liste")
  set text(size: 8.5pt)
  table(
    columns: (24mm, 1fr, 11mm, 16mm, 11mm, 20mm, 24mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3pt),
    table.header(
      head("OZ"),
      head("Kurztext"),
      head("KG"),
      head("Menge", align: right),
      head("Einheit"),
      head("Kennwert", align: right),
      head("Betrag", align: right),
      table.hline(stroke: head-rule),
    ),
    ..data.items.map(it => {
      if it.t == "title" {
        let fill = title-fill-of(it.level + 1)
        (
          table.cell(fill: fill, strong(it.oz)),
          table.cell(fill: fill, colspan: 5, strong(it.text)),
          table.cell(fill: fill, align: right, strong(it.total)),
        )
      } else if it.t == "points" {
        // Datenpunkt-Abschnitt: one line, the details in the appendix.
        (
          strong(it.oz),
          [#strong(it.text) #block(above: 2pt, text(size: 0.9em, fill: muted, "berechnet über Datenpunkte – Einzelheiten im Anhang"))],
          it.kg,
          table.cell(align: right, it.qty),
          "DP",
          [],
          table.cell(align: right, strong(it.gp)),
          table.hline(stroke: thin),
        )
      } else {
        // Zuschlag/Aufteilung („40 % von …“) or Rechengrundlage under the text.
        let text-cell = if it.long.len() > 0 or it.note != "" {
          [#it.text #if it.note != "" { block(above: 2pt, text(size: 0.9em, fill: muted, it.note)) } #for p in it.long { block(above: 2pt, text(size: 0.9em, fill: muted, p)) }]
        } else { it.text }
        (
          it.oz,
          text-cell,
          it.kg,
          table.cell(align: right, it.qty),
          it.unit,
          table.cell(align: right, it.ep),
          table.cell(align: right, it.gp),
          table.hline(stroke: thin),
        )
      }
    }).flatten(),
    table.hline(stroke: head-rule),
    [], table.cell(colspan: 5, strong("Summe netto")), table.cell(align: right, strong(data.net)),
  )
}

// ---------- Anhang: Datenpunkte (je Datenpunkt-Abschnitt) ----------
// Section heading one level down with the amount on the right.
#let sub-band(title, total) = block(
  width: 100%,
  above: 4mm,
  below: 2.5mm,
  fill: title-fill-of(2),
  inset: (x: 7pt, y: 4pt),
  breakable: false,
  {
    bookmark-at(2, title)
    grid(
      columns: (1fr, auto),
      align: (left + horizon, right + horizon),
      text(size: 9.5pt, weight: "bold", fill: heading, title),
      text(size: 9.5pt, weight: "bold", fill: heading, total),
    )
  },
)

// The tables of the Datenpunktrechnung need the width of A4 quer (a new page
// only when an appendix follows).
#set page(flipped: data.landscape or data.points.len() > 0)
#for p in data.points {
  pagebreak(weak: true)
  band("Anhang: Datenpunkte " + p.name)
  set text(size: 9pt)
  {
    let d = p.data
    text(fill: muted, d.stage + " · " + d.level + " · " + d.points + " Datenpunkte · " + d.total)
    v(2mm)
    point-summary(d)
    point-sections(d, sub-band, plants: true)
  }
}
