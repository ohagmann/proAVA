// Prüfung Nachtragsangebot: ein Nachtrag mit seinen Positionen angeboten und
// geprüft, Vermerken, Summen, Ergebnis, Empfehlung und Unterschriften.
#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd, landscape: true)

// Kompakter Kopf: Dokumentart und Nachtrag, daneben das Logo; Projekt- und
// Nachtragsdaten nebeneinander (passt mit Unterschriften auf eine Seite).
#grid(
  columns: (1fr, auto),
  gutter: 8mm,
  {
    bookmark(data.at("bm", default: none))
    text(size: 16pt, weight: "bold", fill: heading, hd.kind)
    v(1pt)
    text(size: 12pt, weight: "bold", data.title)
  },
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
#if data.reason != "" {
  v(2mm)
  grid(columns: (34mm, 1fr), text(fill: muted, "Begründung"), data.reason)
}
#v(2mm)
#line(length: 100%, stroke: 1pt + accent-rule)
#v(3mm)

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, hyphenate: false, body))
#let group(body, n) = table.cell(colspan: n, align: center, text(weight: "bold", fill: heading, body))
#let dropped(l, body) = if l.dropped { text(fill: muted, strike(body)) } else { body }

#bookmark-at(1, "Positionen")
#if data.lines.len() == 0 {
  text(fill: muted, style: "italic", "Der Nachtrag hat noch keine Positionen.")
} else {
  table(
    columns: (20mm, 1fr, 17mm, 11mm, 16mm, 20mm, 21mm, 16mm, 20mm, 21mm, 14mm, 44mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3.5pt),
    align: (x, _) => if x in (4, 5, 6, 7, 8, 9, 10) { right + top } else { left + top },
    table.header(
      table.cell(colspan: 4, []),
      group("angeboten", 3),
      group("geprüft", 3),
      [],
      [],
      head("OZ"),
      head("Kurztext"),
      head("Art"),
      head("Einh."),
      head("Menge", align: right),
      head("EP", align: right),
      head("GP", align: right),
      head("Menge", align: right),
      head("EP", align: right),
      head("GP", align: right),
      head("Abw. %", align: right),
      head("Vermerk"),
      table.hline(stroke: 1pt + accent-rule),
    ),
    ..data.lines.map(l => (
      {
        bookmark(l.at("bm", default: none))
        l.oz
      },
      dropped(l, l.text),
      l.kind,
      l.unit,
      l.oq,
      l.op,
      dropped(l, l.ot),
      l.cq,
      l.cp,
      dropped(l, l.ct),
      l.dev,
      {
        if l.status != "" {
          strong(l.status)
          if l.note != "" { [ – ] }
        }
        l.note
      },
      table.hline(stroke: thin),
    )).flatten(),
  )
}

#v(3mm)
#align(right, block(breakable: false, grid(
  columns: (auto, 36mm),
  column-gutter: 8mm,
  row-gutter: 2.2mm,
  ..data.sums.map(s => (
    if s.bold { strong(s.label) } else { s.label },
    align(right, if s.bold { strong(s.amount) } else { s.amount }),
  )).flatten(),
)))

#v(4mm)
#block(breakable: false, {
  bookmark-at(1, "Ergebnis der Prüfung")
  text(weight: "bold", fill: heading, "Ergebnis der Prüfung")
  v(2mm)
  let rows = (
    ("Prüfvermerk", data.check_note),
    ("Empfehlung", data.recommendation),
  ).filter(((_, v)) => v != "")
  if rows.len() > 0 {
    grid(columns: (34mm, 1fr), row-gutter: 2mm, ..rows.map(((k, v)) => (text(fill: muted, k), v)).flatten())
    v(2mm)
  }
  if data.checked != "" { par(data.checked) }
  if rows.len() == 0 and data.checked == "" {
    text(fill: muted, style: "italic", "Noch kein Prüfvermerk.")
  }
  for note in data.notes {
    par(text(fill: high-text, note))
  }
})

#if data.history.len() > 0 {
  v(2mm)
  text(size: 8pt, fill: muted, "Statusverlauf: " + data.history.join(" · "))
}

#v(1fr)
#block(breakable: false, {
  bookmark-at(1, "Unterschriften")
  v(12mm)
  grid(
    columns: data.signatures.map(_ => 1fr),
    column-gutter: 14mm,
    ..data.signatures.map(s => {
      line(length: 100%, stroke: 0.6pt + muted)
      text(size: 8pt, fill: muted, s)
    }),
  )
})
