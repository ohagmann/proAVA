// Datenpunkte je Bauteil – gemeinsam für die Kostenermittlung nach
// Datenpunkten (cost_estimate.typ) und den Anhang „Datenpunkte“ eines
// Kosten-LV (cost_lv.typ, GU-Fall): Zusammenfassung je Bauteil, DP-Arten mit
// Anzahl und Preis, Aufteilung auf die KG 481–489 bzw. Kennwert, Anlagenliste.
// `d` = die Daten einer Datenpunktrechnung (kinds, sections, rate …, wie
// cost_estimate_json sie schreibt), `band(title, right)` = der Abschnittskopf
// des Ausdrucks. All texts and amounts come formatted.
#import "/common.typ": *

// Kräftige Linie unter Tabellenköpfen (Klassisch schwarz, proAVA Akzentfarbe).
#let head-rule = 1pt + accent-rule
#let head(body, ..args) = table.cell(..args, fill: title-fill, text(weight: "bold", fill: heading, hyphenate: false, body))
#let small(body) = text(size: 0.85em, fill: muted, body)
#let strong-cell(body, ..args) = table.cell(..args, fill: sum-fill, strong(body))
#let total-cell(body, ..args) = table.cell(..args, fill: title-fill, strong(body))

// ---------- Zusammenfassung je Bauteil ----------
#let point-summary(d) = {
  if d.calculation {
    let g = d.groups.len()
    set text(size: if g > 6 { 7.5pt } else { 8.5pt })
    table(
      columns: (1fr, 14mm) + range(g).map(_ => 25mm) + (28mm, 15mm),
      stroke: table-stroke(),
      inset: (x: 3pt, y: 3.5pt),
      align: (left, right) + range(g + 2).map(_ => right),
      table.header(
        head("Bauteil"),
        head("DP", align: right),
        ..d.groups.map(gr => table.cell(fill: title-fill, align: right, {
          text(weight: "bold", fill: heading, "KG " + gr.code)
          linebreak()
          text(size: 0.78em, fill: muted, hyphenate: true, gr.label)
        })),
        head("Summe", align: right),
        head("Anteil", align: right),
        table.hline(stroke: head-rule),
      ),
      ..d.sections.map(s => (
        s.name, s.points, ..s.by_group.map(v => if v == "" { "–" } else { v }), strong(s.total), text(fill: muted, s.share_of_total),
        table.hline(stroke: thin),
      )).flatten(),
      total-cell("Gesamt"), total-cell(d.points, align: right),
      ..d.groups.map(gr => total-cell(gr.amount, align: right)),
      total-cell(d.total, align: right), total-cell("100 %", align: right),
      [], [], ..d.groups.map(gr => text(fill: muted, gr.percent)), [], [],
    )
  } else {
    set text(size: 8.5pt)
    let span = d.low != none and d.high != none
    table(
      columns: (1fr, 20mm, 34mm) + if span { (32mm, 32mm) } else { () } + (36mm, 16mm),
      stroke: table-stroke(),
      inset: (x: 3pt, y: 3.5pt),
      align: (left,) + range(if span { 6 } else { 4 }).map(_ => right),
      table.header(
        head("Bauteil"),
        head("DP", align: right),
        head("Kennwert", align: right),
        ..if span { (head("von", align: right), head("bis", align: right)) } else { () },
        head("KG 480", align: right),
        head("Anteil", align: right),
        table.hline(stroke: head-rule),
      ),
      ..d.sections.map(s => (
        s.name, s.points, if d.rate != none { d.rate + " / DP" } else { "je DP-Art" },
        ..if span { (s.low, s.high) } else { () },
        strong(s.total), text(fill: muted, s.share_of_total),
        table.hline(stroke: thin),
      )).flatten(),
      total-cell("Gesamt"), total-cell(d.points, align: right), total-cell(""),
      ..if span { (total-cell(d.low, align: right), total-cell(d.high, align: right)) } else { () },
      total-cell(d.total, align: right), total-cell("100 %", align: right),
    )
  }
}

// ---------- Bauteile ----------
#let dp-table(d, s) = {
  table(
    columns: (13mm, 1fr, 16mm, 24mm, 28mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.2pt),
    align: (left, left, right, right, right),
    table.header(
      head("Nr."), head("DP-Art"), head("Anzahl", align: right),
      head(if d.calculation { "EP" } else { "Kennwert" }, align: right),
      head(if d.calculation { "GP" } else { "Betrag" }, align: right),
      table.hline(stroke: head-rule),
    ),
    ..d.kinds.zip(s.kinds).map(((k, v)) => (
      text(fill: muted, k.number),
      k.caption + if k.virtual { text(fill: muted, " · virtuell") },
      if v.used { v.count } else { text(fill: muted, "–") },
      v.price,
      if v.used { v.amount } else { text(fill: muted, "–") },
      table.hline(stroke: thin),
    )).flatten(),
    strong-cell(""), strong-cell(if d.calculation { "Gesamtsumme" } else { "Summe KG 480" }),
    strong-cell(s.points, align: right), strong-cell(""), strong-cell(s.total, align: right),
  )
}

#let split-table(s) = {
  table(
    columns: (13mm, 1fr, 16mm, 28mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.2pt),
    align: (left, left, right, right),
    table.header(
      head("KG"), head("Kostengruppe"), head("Anteil", align: right), head("Betrag", align: right),
      table.hline(stroke: head-rule),
    ),
    ..s.shares.map(sh => (
      sh.code, sh.label, sh.percent, sh.amount,
      table.hline(stroke: thin),
    )).flatten(),
    strong-cell(""), strong-cell("Summe"),
    table.cell(fill: if s.split_ok { sum-fill } else { high-fill }, align: right, strong(s.percent_sum)),
    strong-cell(s.total, align: right),
  )
  v(1.5mm)
  align(right, box(
    fill: if s.split_ok { low-fill } else { high-fill },
    inset: (x: 6pt, y: 3pt),
    radius: 2pt,
    text(fill: if s.split_ok { low-text } else { high-text }, [Prüfsumme #h(6pt) #strong(s.check)]),
  ))
  if s.own_shares {
    v(1mm)
    align(right, small("eigene Aufteilung für dieses Bauteil"))
  }
}

#let plant-table(d, s) = {
  let k = d.kinds.len()
  set text(size: if k > 8 { 7pt } else { 7.8pt })
  table(
    columns: (26mm, 16mm, 1fr) + range(k).map(_ => 10.5mm) + (12mm, 48mm),
    stroke: table-stroke(),
    inset: (x: 2.5pt, y: 2.6pt),
    align: (left, left, left) + range(k + 1).map(_ => right) + (left,),
    table.header(
      head("Ort"), head("Gewerk"), head("Anlage"),
      ..d.kinds.map(kd => head(kd.key, align: right)),
      head("DP", align: right), head("Bemerkung"),
      table.hline(stroke: head-rule),
    ),
    ..s.rows.map(r => (
      r.location, r.trade, r.plant, ..r.counts, strong(r.points), text(fill: muted, r.remark),
      table.hline(stroke: thin),
    )).flatten(),
    strong-cell(""), strong-cell(""), strong-cell("Summe " + s.name),
    ..s.kinds.map(v => strong-cell(if v.used { v.count } else { "" }, align: right)),
    strong-cell(s.points, align: right), strong-cell(""),
  )
}

#let point-sections(d, band, plants: true) = {
  for (i, s) in d.sections.enumerate() {
    pagebreak(weak: true)
    band("Bauteil: " + s.name, s.total)
    if d.calculation {
      grid(
        columns: (1.15fr, 1fr),
        column-gutter: 8mm,
        {
          text(size: 9pt, weight: "bold", fill: heading, "Datenpunkte")
          v(1.5mm)
          set text(size: 8.5pt)
          dp-table(d, s)
        },
        {
          text(size: 9pt, weight: "bold", fill: heading, "Kostenaufteilung nach Titelsummen")
          v(1.5mm)
          set text(size: 8.5pt)
          split-table(s)
        },
      )
    } else {
      grid(
        columns: (1.15fr, 1fr),
        column-gutter: 8mm,
        {
          text(size: 9pt, weight: "bold", fill: heading, "Datenpunkte")
          v(1.5mm)
          set text(size: 8.5pt)
          dp-table(d, s)
        },
        {
          text(size: 9pt, weight: "bold", fill: heading, "Kostenschätzung")
          v(1.5mm)
          rect(width: 100%, stroke: thin, inset: 8pt, radius: 2pt, {
            text(fill: muted, "KG 480 Gebäude- und Anlagenautomation")
            v(2pt)
            text(size: 15pt, weight: "bold", fill: heading, s.total)
            v(2pt)
            if d.rate != none { text(size: 8.5pt, s.points + " DP × " + d.rate) } else { text(size: 8.5pt, s.points + " DP, Kennwert je DP-Art") }
            if s.low != none and s.high != none {
              v(2pt)
              text(size: 8.5pt, fill: muted, "Spanne " + s.low + " bis " + s.high)
            }
          })
        },
      )
    }
    if plants and s.rows.len() > 0 {
      v(4mm)
      text(size: 9pt, weight: "bold", fill: heading, "Anlagenliste")
      v(1.5mm)
      plant-table(d, s)
    }
  }
}
