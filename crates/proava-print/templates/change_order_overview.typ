// Nachtragsübersicht: alle Nachträge eines LV mit Anlass, Status und
// Beträgen, Summen je Status und Wirkung auf die Auftragssumme (Prognose).
#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd, landscape: true)

#grid(
  columns: (1fr, auto),
  gutter: 8mm,
  text(size: 16pt, weight: "bold", fill: heading, hd.kind),
  if hd.logo != none and hd.logo.t == "img" { fitted-image(hd.logo.path, 55mm, 16mm) },
)
#v(3mm)
#let fact-grid(rows) = grid(
  columns: (34mm, 1fr),
  row-gutter: 2mm,
  ..rows.filter(((_, v)) => v != "").map(((k, v)) => (text(fill: muted, k), v)).flatten(),
)
#grid(
  columns: (1fr, 1fr),
  column-gutter: 10mm,
  fact-grid((
    ("Projekt", hd.project),
    ("Projektbezeichnung", hd.project_title),
    ("Auftraggeber", hd.client),
    ("Leistungsverzeichnis", hd.lv),
    ("Datum", hd.date),
  )),
  fact-grid(data.facts.map(f => (f.label, f.text))),
)
#v(2mm)
#line(length: 100%, stroke: 1pt + accent-rule)
#v(3mm)

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, hyphenate: false, body))

#bookmark-at(1, "Nachträge")
#if data.lines.len() == 0 {
  text(fill: muted, style: "italic", "Noch keine Nachträge.")
} else {
  table(
    columns: (10mm, 1fr, 30mm, 1fr, 20mm, 18mm, 24mm, 24mm, 24mm, 22mm, 17mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.5pt),
    align: (x, _) => if x in (6, 7, 8, 9) { right + top } else { left + top },
    table.header(
      head("Nr."),
      head("Nachtrag"),
      head("Anlass"),
      head("Begründung"),
      head("Status"),
      head("Datum"),
      head("angeboten", align: right),
      head("geprüft", align: right),
      head("beauftragt", align: right),
      head("Gutschrift", align: right),
      head("Prognose"),
      table.hline(stroke: 1pt + accent-rule),
    ),
    ..data.lines.map(l => (
      {
        bookmark(l.at("bm", default: none))
        l.label
      },
      l.title,
      l.basis,
      text(size: 8pt, l.reason),
      l.status,
      l.date,
      l.offered,
      l.checked,
      l.approved,
      l.credit,
      l.forecast,
      table.hline(stroke: thin),
    )).flatten(),
    table.hline(stroke: 1pt + accent-rule),
    [],
    strong("Summe"),
    [],
    [],
    [],
    [],
    strong(data.total.at(0)),
    strong(data.total.at(1)),
    strong(data.total.at(2)),
    strong(data.total.at(3)),
    [],
  )
}

#v(6mm)
#block(breakable: false, grid(
  columns: (1fr, 1fr),
  column-gutter: 12mm,
  {
    bookmark-at(1, "Summen je Status")
    text(weight: "bold", fill: heading, "Summen je Status")
    v(2mm)
    table(
      columns: (1fr, 14mm, 26mm, 26mm, 26mm),
      stroke: table-stroke(),
      inset: (x: 3pt, y: 3pt),
      align: (x, _) => if x >= 1 { right } else { left },
      table.header(
        head("Status"),
        head("Anzahl", align: right),
        head("angeboten", align: right),
        head("geprüft", align: right),
        head("beauftragt", align: right),
        table.hline(stroke: 0.8pt + accent-rule),
      ),
      ..data.by_status.map(r => (..r, table.hline(stroke: thin))).flatten(),
    )
  },
  {
    bookmark-at(1, "Auftragssumme")
    text(weight: "bold", fill: heading, "Auftragssumme")
    v(2mm)
    grid(
      columns: (1fr, 34mm),
      row-gutter: 2.4mm,
      ..data.contract.map(s => (
        if s.bold { strong(s.label) } else { s.label },
        align(right, if s.bold { strong(s.amount) } else { s.amount }),
      )).flatten(),
    )
  },
))

#for note in data.notes {
  v(2mm)
  text(size: 8pt, fill: muted, note)
}
