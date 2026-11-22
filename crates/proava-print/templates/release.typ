// Rechnungsfreigabe (Zahlungsfreigabe): release calculation of one invoice.
#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd)

#title-block(hd)

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, hyphenate: false, body))
#let section(title) = {
  v(3mm)
  bookmark-at(1, title)
  text(size: 10pt, weight: "bold", fill: heading, title)
  v(0.5mm)
}

#bookmark(data.at("bm", default: none))
#text(size: 11pt, weight: "bold", fill: heading, data.invoice)
#v(2mm)
#grid(
  columns: (1fr, 62mm),
  column-gutter: 8mm,
  grid(
    columns: (34mm, 1fr),
    row-gutter: 2mm,
    ..data.facts.map(f => (text(fill: muted, f.label), f.text)).flatten(),
  ),
  {
    for (i, a) in data.addresses.enumerate() {
      if i > 0 { v(3mm) }
      text(size: 8pt, fill: muted, a.caption)
      linebreak()
      a.lines.join(linebreak())
    }
  },
)

#section("Leistung")
#table(
  columns: (1fr, 36mm, 36mm),
  stroke: table-stroke(),
  inset: (x: 3pt, y: 2.3pt),
  align: (left, right, right),
  table.header(
    head(""), head("beauftragt", align: right), head("abgerechnet / geprüft", align: right),
    table.hline(stroke: 1pt + accent-rule),
  ),
  ..data.overview.map(o => (
    o.label,
    if o.ordered == none { [] } else { o.ordered },
    if o.performed == none { [] } else { o.performed },
    table.hline(stroke: thin),
  )).flatten(),
  text(fill: muted, "Stand der geprüften Leistung"), [], text(fill: muted, data.performed_percent),
)

#section("Rechenweg der Freigabe")
#table(
  columns: (1fr, 24mm, 36mm),
  stroke: table-stroke(),
  inset: (x: 3pt, y: 2.3pt),
  align: (left, right, right),
  ..data.calculation.map(l => {
    let cells = (
      if l.strong { strong(l.label) } else { l.label },
      text(fill: muted, l.extra),
      if l.amount == none { [] } else if l.strong { strong(l.amount) } else { l.amount },
    )
    if l.strong { (table.hline(stroke: 0.6pt + accent-rule),) + cells } else { cells }
  }).flatten(),
)

#if data.earlier.len() > 0 {
  section("Bisherige Freigaben")
  table(
    columns: (1fr, 22mm, 30mm, 30mm, 30mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 2.3pt),
    align: (left, left, right, right, right),
    table.header(
      head("Rechnung"), head("Datum"), head("Leistung netto", align: right),
      head("Freigabe netto", align: right), head("Freigabe brutto", align: right),
      table.hline(stroke: 1pt + accent-rule),
    ),
    ..data.earlier.map(e => (e.label, e.date, e.performance, e.release_net, e.release_gross, table.hline(stroke: thin))).flatten(),
    strong("Summe"), [], [], strong(data.earlier_total), strong(data.earlier_total_gross),
  )
}

#if data.notes.len() > 0 {
  v(3mm)
  for n in data.notes {
    par(n)
  }
}
#if data.remarks.len() > 0 {
  v(2mm)
  for r in data.remarks {
    par(r)
  }
}

#v(1fr)
#block(breakable: false, {
  bookmark-at(1, "Unterschriften")
  v(12mm)
  grid(
    columns: data.signatures.map(_ => 1fr),
    column-gutter: 12mm,
    ..data.signatures.map(s => {
      line(length: 100%, stroke: 0.6pt + muted)
      text(size: 8pt, fill: muted, "Ort, Datum, Unterschrift")
      linebreak()
      text(size: 8pt, s)
    }),
  )
})
