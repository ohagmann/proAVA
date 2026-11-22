// Kostenschätzung (DIN 276, 2. Ebene) / Kostenberechnung (3. Ebene) für KG 480.
// All texts and amounts come formatted from /data.json.
#import "/common.typ": *
#import "/points.typ": *

#let data = json("/data.json")
#let hd = data.header
#let n = data.kinds.len()
#let calc-mode = data.calculation

#show: setup.with(hd, landscape: data.landscape)

// ---------- Bausteine (Tabellenköpfe und Bauteile aus /points.typ) ----------

// Section heading: tinted band with an accent stripe, value on the right.
#let band(title, right-side: none) = block(
  width: 100%,
  above: 5mm,
  below: 3mm,
  fill: title-fill,
  stroke: if classic { none } else { (left: 3pt + accent) },
  inset: (x: 7pt, y: 5pt),
  breakable: false,
  {
    // PDF-Lesezeichen: every band is a section of the outline.
    bookmark-at(1, title)
    grid(
      columns: (1fr, auto),
      align: (left + horizon, right + horizon),
      text(size: 10.5pt, weight: "bold", fill: heading, title),
      if right-side != none { text(size: 10.5pt, weight: "bold", fill: heading, right-side) },
    )
  },
)

// Key figure: label above, large value below.
#let key-figure(label, value, note: none) = rect(
  width: 100%,
  height: 19mm,
  stroke: thin,
  inset: (x: 8pt, y: 7pt),
  radius: 2pt,
  {
    text(size: 8pt, fill: muted, label)
    v(2pt)
    text(size: 15pt, weight: "bold", fill: heading, value)
    if note != none {
      v(1pt)
      text(size: 7.5pt, fill: muted, note)
    }
  },
)

// Horizontal bar 0…1 with a light track.
#let bar(ratio, height: 3.2mm) = box(width: 100%, height: height, fill: tint(8%), radius: 1pt, {
  if ratio > 0 { box(width: ratio * 100%, height: height, fill: accent, radius: 1pt) }
})

#let figures = {
  let items = (
    key-figure(
      if calc-mode { "Gesamtkosten KG 481–489 (netto)" } else { "Kosten KG 480 (netto)" },
      data.total,
      note: if data.low != none and data.high != none { "Spanne " + data.low + " bis " + data.high } else { none },
    ),
    key-figure("Datenpunkte", data.points, note: str(n) + " DP-Arten"),
    key-figure("Bauteile", str(data.sections.len())),
  )
  if not calc-mode and data.rate != none {
    items.push(key-figure("Kostenkennwert je DP", data.rate,
      note: if data.rate_low != none and data.rate_high != none { data.rate_low + " bis " + data.rate_high } else { "statistisch" }))
  } else if calc-mode {
    items.push(key-figure("Prüfsumme", data.check, note: if data.check_ok { "Aufteilung vollständig" } else { "Aufteilung prüfen" }))
  }
  grid(columns: (1fr,) * items.len(), column-gutter: 4mm, ..items)
}

// ---------- Deckblatt (optional) ----------
#if data.cover {
  [#metadata("cover") <oa-cover>]
  bookmark-at(1, "Deckblatt")
  v(18mm)
  text(size: 11pt, fill: muted, data.level + " · Kostengruppe 480 Gebäude- und Anlagenautomation")
  v(2mm)
  text(size: 28pt, weight: "bold", fill: heading, data.stage)
  if data.title != "" and data.title != data.stage {
    v(1mm)
    text(size: 14pt, fill: heading, data.title)
  }
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
      ("Stand", hd.date),
      ("Bearbeiter", hd.author),
    ).filter(((_, v)) => v != "").map(((k, v)) => (text(fill: muted, k), text(size: 11pt, v))).flatten(),
  )
  v(12mm)
  figures
  if data.remark != "" {
    v(6mm)
    text(fill: muted, data.remark)
  }
  pagebreak()
} else {
  title-block(hd, extra: (("Stufe", data.level),))
  if data.title != "" and data.title != data.stage {
    text(size: 12pt, weight: "bold", fill: heading, data.title)
    v(2mm)
  }
  figures
}

// ---------- Zusammenfassung ----------
#band("Zusammenfassung", right-side: data.total)

#point-summary(data)

#if data.chart {
  v(3mm)
  let items = if calc-mode {
    data.groups.map(gr => ("KG " + gr.code + " " + gr.label, gr.ratio, gr.percent, gr.amount))
  } else {
    data.sections.map(s => (s.name, s.ratio, s.share_of_total, s.total))
  }
  block(breakable: false, {
    text(size: 9pt, weight: "bold", fill: heading, if calc-mode { "Anteile der Kostengruppen" } else { "Anteile der Bauteile" })
    v(2mm)
    set text(size: 8.5pt)
    grid(
      columns: (78mm, 1fr, 16mm, 30mm),
      column-gutter: 4mm,
      row-gutter: 2.4mm,
      align: (left + horizon, left + horizon, right + horizon, right + horizon),
      ..items.map(((label, ratio, pct, amount)) => (label, bar(ratio), text(fill: muted, pct), amount)).flatten(),
    )
  })
}

// ---------- Bauteile ----------
#point-sections(data, (title, total) => band(title, right-side: total), plants: data.plants)

// ---------- Kostenkontrolle ----------
#if data.control != none {
  let c = data.control
  let diff(d) = if d.sign == "" { text(fill: muted, "–") } else {
    text(fill: if d.sign == "up" { high-text } else { low-text }, d.amount + if d.percent != "" { " (" + d.percent + ")" })
  }
  pagebreak(weak: true)
  band("Kostenkontrolle je Kostengruppe")
  set text(size: 8.5pt)
  table(
    columns: (13mm, 1fr, 30mm, 34mm, 38mm) + if c.has_award { (30mm, 38mm) } else { () },
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.2pt),
    align: (left, left) + range(if c.has_award { 5 } else { 3 }).map(_ => right),
    table.header(
      head("KG"), head("Kostengruppe"), head(data.stage, align: right), head("Kostenvoranschlag", align: right),
      head("Abweichung", align: right),
      ..if c.has_award { (head("Auftrag", align: right), head("Abweichung", align: right)) } else { () },
      table.hline(stroke: head-rule),
    ),
    ..c.lines.map(l => (
      if l.code == "" { "–" } else { l.code }, l.label, l.estimate, l.planner, diff(l.planner_diff),
      ..if c.has_award { (l.awarded, diff(l.awarded_diff)) } else { () },
      table.hline(stroke: thin),
    )).flatten(),
    total-cell(""), total-cell("Summe"), total-cell(c.estimate, align: right), total-cell(c.planner, align: right),
    total-cell(diff(c.planner_diff), align: right),
    ..if c.has_award { (total-cell(c.awarded, align: right), total-cell(diff(c.awarded_diff), align: right)) } else { () },
  )
  v(1mm)
  small("Kostenvoranschlag: Planerpreise der Ausschreibungs-LVs des Projekts je Kostengruppe (Positionen ohne eigene KG zählen zu der ihres Titels). "
    + if c.has_award { "Auftrag: beauftragte Angebote netto vor Nachlass." } else { "Noch kein Auftrag erteilt." })
}

// ---------- Erläuterungen ----------
#block(breakable: false, {
  band("Erläuterungen und Annahmen")
  set text(size: 8.5pt)
  let points = ()
  points.push("Grundlage: DIN 276:2018-12, " + data.stage + " in der " + data.level.replace("DIN 276, ", "")
    + " für die Kostengruppe 480 Gebäude- und Anlagenautomation. Alle Beträge netto ohne Umsatzsteuer in " + data.cur + ".")
  if calc-mode {
    points.push("Einheitspreise je Datenpunkt: " + data.kinds.zip(data.kind_sums).map(((k, v)) => k.key + " " + v.price).join(", ") + ".")
    points.push("Aufteilung der Gesamtsumme auf die Titelsummen (Erfahrungswerte): "
      + data.shares.map(sh => "KG " + sh.code + " " + sh.percent).join(", ") + "."
      + if data.own_split.len() > 0 { " Eigene Aufteilung: " + data.own_split.join(", ") + "." } else { "" })
    points.push("Beträge je Kostengruppe auf Cent gerundet; ein Rundungsrest von wenigen Cent geht an die Kostengruppe mit dem größten Anteil, damit die Summe der Kostengruppen der Gesamtsumme entspricht.")
  } else if data.rate != none {
    points.push("Kostenkennwert: " + data.rate + " je Datenpunkt (statistischer Mittelwert über alle DP-Arten)"
      + if data.rate_low != none and data.rate_high != none { ", Spanne " + data.rate_low + " bis " + data.rate_high } else { "" } + ".")
  } else {
    points.push("Kostenkennwerte je DP-Art: " + data.kinds.zip(data.kind_sums).map(((k, v)) => k.key + " " + v.price).join(", ") + ".")
  }
  points.push("Datenpunkte: Hardware-Datenpunkte (1.1) und virtuelle Datenpunkte (1.2) nach Anlagenliste.")
  for p in points {
    grid(columns: (4mm, 1fr), [–], p)
    v(1.2mm)
  }
  if data.remark != "" and not data.cover {
    v(1mm)
    text(fill: muted, data.remark)
  }
  if data.warnings.len() > 0 {
    v(2mm)
    text(weight: "bold", fill: high-text, "Hinweise")
    v(1mm)
    for w in data.warnings { grid(columns: (4mm, 1fr), text(fill: high-text, "!"), text(fill: high-text, w)); v(1mm) }
  }
})
