#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header
#let n = data.bidders.len()

#show: setup.with(hd, landscape: data.landscape)

#title-block(hd)

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, body))
#let num(v, bold: false) = table.cell(align: right, if bold { strong(v) } else { v })

#bookmark-at(1, "Kosten nach Kostengruppen")
#text(size: 11pt, weight: "bold", fill: heading, "Kosten nach Kostengruppen")
#h(6pt)
#text(fill: muted, "Katalog " + data.catalog + " · Positionen ohne eigene Kostengruppe zählen zu der ihres Titels · Angebote netto vor Nachlass · Beträge in " + hd.at("cur", default: "€"))
#v(4mm)

#set text(size: if n > 4 { 7.5pt } else { 8.5pt })
#table(
  columns: (14mm, 1fr, 12mm, 26mm, 16mm) + range(n).map(_ => 26mm),
  stroke: table-stroke(),
  inset: (x: 3pt, y: 3.5pt),
  table.header(
    head("KG"),
    head("Bezeichnung"),
    head("Pos.", align: right),
    head("Planer", align: right),
    head("Anteil", align: right),
    // Long firm names break with a hyphen instead of running into the next column.
    ..data.bidders.map(b => head(text(hyphenate: true, b), align: right)),
    table.hline(stroke: 1pt + accent-rule),
  ),
  ..data.lines.map(l => {
    let fill = if l.subtotal { sum-fill } else { none }
    let cells = (
      table.cell(fill: fill, if l.code == "" { "–" } else if l.subtotal { strong(l.code) } else { l.code }),
      table.cell(fill: fill, if l.subtotal { strong("Summe " + l.label) } else { l.label }),
      table.cell(fill: fill, align: right, str(l.positions)),
      table.cell(fill: fill, align: right, if l.subtotal { strong(l.planner) } else { l.planner }),
      table.cell(fill: fill, align: right, text(fill: muted, l.share)),
      ..l.bidders.map(v => table.cell(fill: fill, align: right, if l.subtotal { strong(v) } else { v })),
    )
    cells + (table.hline(stroke: thin),)
  }).flatten(),
  table.hline(stroke: 1pt + accent-rule),
  strong("Gesamt"), [],
  num(str(data.lines.filter(l => not l.subtotal).map(l => l.positions).sum(default: 0)), bold: true),
  num(data.planner_total, bold: true), [],
  ..data.bidder_totals.map(v => num(v, bold: true)),
)
