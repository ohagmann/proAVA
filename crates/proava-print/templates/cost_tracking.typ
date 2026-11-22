// Kostenverfolgung nach DIN 276:2018-12 (4.4 Kostenkontrolle, 4.5 Kostensteuerung,
// 4.6 Kostenvorgabe): die Kette Kostenvorgabe bis Kostenfeststellung je Kostengruppe.
// All texts and amounts come formatted from /data.json.
#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header
#let n = data.columns.len()
#let show-dev = data.deviation != ""

#show: setup.with(hd, landscape: true)

#let head-rule = 1pt + accent-rule
// Mehr als die Schwelle über der Vorstufe: hell bernsteinfarben hinterlegt.
#let warn-fill = rgb("#fdf0d5")

#let head(body, ..args) = table.cell(..args, fill: title-fill, text(weight: "bold", fill: heading, hyphenate: false, body))
#let small(body) = text(size: 0.85em, fill: muted, body)

#let band(title, right-side: none) = block(
  width: 100%,
  above: 5mm,
  below: 3mm,
  fill: title-fill,
  stroke: if classic { none } else { (left: 3pt + accent) },
  inset: (x: 7pt, y: 5pt),
  breakable: false,
  {
    bookmark-at(1, title)
    grid(
      columns: (1fr, auto),
      align: (left + horizon, right + horizon),
      text(size: 10.5pt, weight: "bold", fill: heading, title),
      if right-side != none { text(size: 10.5pt, weight: "bold", fill: heading, right-side) },
    )
  },
)

#let key-figure(f) = rect(
  width: 100%,
  height: 17mm,
  stroke: thin,
  inset: (x: 7pt, y: 6pt),
  radius: 2pt,
  {
    text(size: 8pt, fill: muted, f.label)
    v(2pt)
    text(size: 13pt, weight: "bold", fill: heading, f.value)
    if f.note != "" {
      v(1pt)
      text(size: 7.5pt, fill: muted, f.note)
    }
  },
)

#let figures = if data.figures.len() > 0 {
  grid(columns: (1fr,) * data.figures.len(), column-gutter: 3mm, ..data.figures.map(key-figure))
}

// ---------- Deckblatt (optional) ----------
#if data.cover {
  [#metadata("cover") <oa-cover>]
  bookmark-at(1, "Deckblatt")
  v(18mm)
  text(size: 11pt, fill: muted, "DIN 276:2018-12, 4.4 bis 4.6 · Kostenkontrolle und Kostensteuerung")
  v(2mm)
  text(size: 28pt, weight: "bold", fill: heading, "Kostenverfolgung")
  v(6mm)
  line(length: 100%, stroke: head-rule)
  v(6mm)
  grid(
    columns: (42mm, 1fr),
    row-gutter: 6pt,
    ..(
      ("Projekt", hd.project),
      ("Projektbezeichnung", hd.project_title),
      ("Auftraggeber", hd.client),
      ("Kostenstand", hd.date),
      ("Bearbeiter", hd.author),
      ("Beträge", data.amounts),
    ).filter(((_, v)) => v != "").map(((k, v)) => (text(fill: muted, k), text(size: 11pt, v))).flatten(),
  )
  v(12mm)
  figures
  pagebreak()
} else {
  title-block(hd, extra: (("Beträge", data.amounts),))
  figures
}

// ---------- Tabelle ----------
#let total-row = data.rows.find(r => r.kind == "total")
#let forecast-index = data.columns.position(c => c == "Prognose")
#band(
  if data.first_column == "Kostengruppe" { "Kostenverfolgung je Kostengruppe" } else { "Kostenverfolgung je Vergabeeinheit" },
  right-side: if total-row != none and forecast-index != none and total-row.cells.at(forecast-index).v != none {
    "Prognose " + total-row.cells.at(forecast-index).v + " " + data.cur
  },
)

#{
  // At most 23 mm; with the comparison columns narrower (the first column keeps about 47 mm).
  let value-width = calc.min(23, 220 / n) * 1mm
  let row-fill(r) = if r.kind == "total" { title-fill } else if r.kind in ("group", "lv", "nolv") { sum-fill } else { none }
  let bold(r) = r.kind != "leaf" and r.kind != "without"
  // Stage names break into two lines (Kosten-schätzung) instead of overlapping.
  let stage-head(c) = table.cell(fill: title-fill, align: right + bottom,
    text(weight: "bold", fill: heading, hyphenate: true, size: 6.8pt, c))
  let value-cell(c, r) = {
    let fill = if c.m == "high" { high-fill } else if c.m == "warn" { warn-fill } else { row-fill(r) }
    // A stage gone since the recorded state: „–“ with its change.
    table.cell(fill: fill, align: right, {
      if c.v != none or (show-dev and c.d != none) {
        let v = if c.v != none { c.v } else { "–" }
        if bold(r) { strong(v) } else { v }
        if show-dev and c.d != none {
          linebreak()
          text(size: 6pt, fill: muted, c.d)
        }
      }
    })
  }
  set text(size: 7.4pt)
  table(
    columns: (1fr,) + range(n).map(_ => value-width),
    stroke: table-stroke(),
    inset: (x: 2pt, y: 2.8pt),
    align: (left,) + range(n).map(_ => right),
    table.header(
      head(data.first_column),
      ..data.columns.map(c => stage-head(c)),
      table.hline(stroke: head-rule),
    ),
    ..data.rows.map(r => (
      table.cell(fill: row-fill(r), pad(left: r.depth * 3mm, if bold(r) { strong(r.title) } else { r.title })),
      ..r.cells.map(c => value-cell(c, r)),
      table.hline(stroke: thin),
    )).flatten(),
  )
  v(1.5mm)
  set text(size: 7.5pt)
  let swatch(fill) = box(width: 5mm, height: 2.8mm, fill: fill, stroke: thin, baseline: 0.5pt)
  grid(
    columns: (auto, auto, auto, auto, 1fr),
    column-gutter: 2mm,
    align: horizon,
    swatch(high-fill), [über der Kostenvorgabe],
    swatch(warn-fill), [mehr als #data.threshold % über der Vorstufe],
    align(right, small(
      "Beträge " + data.amounts + " in " + data.cur + " · " + data.view
        + if show-dev { " · zweite Zeile: Abweichung " + data.deviation + " in " + data.cur + " (gerundet) und %" } else { "" },
    )),
  )
}

// ---------- Diagramm ----------
#if data.chart_on {
  pagebreak(weak: true)
  band("Diagramm der Stufen je " + data.chart_of)
  let stage-fill(stage, over) = if stage == "Kostenvorgabe" { none } else if stage == "Prognose" {
    if over { rgb("#d46a6a") } else { accent }
  } else if stage in ("Kostenschätzung", "Kostenberechnung", "Kostenvoranschlag") { tint(22%) } else if stage in ("Angebot", "Auftrag") { tint(45%) } else { tint(65%) }
  let bar(ratio, stage, over) = box(width: 100%, height: 2.6mm, {
    if ratio > 0 {
      box(
        width: ratio * 100%,
        height: 2.6mm,
        fill: stage-fill(stage, over),
        stroke: if stage == "Kostenvorgabe" { 0.8pt + accent-rule } else { none },
        radius: 0.6pt,
      )
    }
  })
  let item(it) = block(breakable: false, width: 100%, {
    text(size: 8.5pt, weight: "bold", fill: heading, it.label)
    v(1mm)
    set text(size: 7.2pt)
    grid(
      columns: (30mm, 1fr, 26mm),
      column-gutter: 2mm,
      row-gutter: 1.3mm,
      align: (left + horizon, left + horizon, right + horizon),
      ..it.bars.filter(b => b.amount != none).map(b => (
        text(fill: muted, b.stage),
        bar(b.ratio, b.stage, it.over),
        b.amount,
      )).flatten(),
    )
  })
  grid(columns: (1fr, 1fr), column-gutter: 10mm, row-gutter: 5mm, ..data.chart.map(item))
  v(2mm)
  set text(size: 7.5pt)
  small("Alle Balken im selben Maßstab; Kostenvorgabe als Umriss, Prognose über der Kostenvorgabe rot hinterlegt.")
}

// ---------- Erläuterungen ----------
#if data.notes.len() > 0 {
  band("Erläuterungen und Maßnahmen")
  set text(size: 8.5pt)
  table(
    columns: (14mm, 62mm, 1fr),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.5pt),
    table.header(
      head("KG"), head("Kostengruppe"), head("Erläuterung / Maßnahme (DIN 276, 4.4.3 und 4.5)"),
      table.hline(stroke: head-rule),
    ),
    ..data.notes.map(note => (note.code, note.label, note.text, table.hline(stroke: thin))).flatten(),
  )
}

// ---------- Grundlagen ----------
#block({
  band("Grundlagen und Annahmen")
  set text(size: 8.5pt)
  // Heading and list stay together (a heading never ends a page).
  let items-list(title, items) = if items.len() > 0 {
    block(breakable: items.len() > 12, {
      text(weight: "bold", fill: heading, title)
      v(1mm)
      for item in items {
        grid(columns: (4mm, 1fr), [–], item)
        v(0.8mm)
      }
    })
    v(2mm)
  }
  items-list("Kostenvorgabe (Budget des Bauherrn, DIN 276, 4.6)", data.budget)
  items-list("Kostenermittlungen", data.sources)
  items-list("Prognose (DIN 276, 4.2.13)", data.assumptions)
  items-list("Umsatzsteuer je LV und Kostenermittlung", data.vat)
  items-list("Stufen", (
    "Kostenvoranschlag: Planerpreise der Leistungsverzeichnisse (DIN 276, 4.3.5).",
    "Kostenanschlag (4.3.6): Angebot (beauftragt, gewählt oder günstigstes, nach Nachlass), Auftrag mit beauftragten Nachträgen, Abrechnung (geprüfte Leistung kumuliert).",
    "Kostenfeststellung (4.3.7): geprüfte Schlussrechnungen.",
    "Vorstufe: je Vergabeeinheit die letzte frühere Stufe dieses LV; Kostenermittlungen ohne LV je Kostengruppe; ohne frühere Stufe im LV die Kostenberechnung der Zeile.",
  ))
  if data.warnings.len() > 0 {
    text(weight: "bold", fill: high-text, "Hinweise")
    v(1mm)
    for w in data.warnings {
      grid(columns: (4mm, 1fr), text(fill: high-text, "!"), text(fill: high-text, w))
      v(0.8mm)
    }
    v(2mm)
  }
})

#if data.without.len() > 0 {
  band("Positionen ohne Kostengruppe")
  set text(size: 8.5pt)
  table(
    columns: (50mm, 28mm, 1fr, 30mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.2pt),
    align: (left, left, left, right),
    table.header(
      head("Vergabeeinheit"), head("OZ"), head("Kurztext"), head("Betrag", align: right),
      table.hline(stroke: head-rule),
    ),
    ..data.without.map(w => (w.lv, w.oz, w.text, w.amount, table.hline(stroke: thin))).flatten(),
  )
}
