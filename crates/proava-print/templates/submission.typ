// Niederschrift über die Angebotsöffnung (Submissionsprotokoll).
#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd, landscape: true)

#title-block(hd)

#if data.facts.len() > 0 {
  grid(
    columns: (55mm, 1fr),
    row-gutter: 2.5mm,
    ..data.facts.map(f => (text(fill: muted, f.label), f.text)).flatten(),
  )
  v(6mm)
}

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, hyphenate: false, body))
#let amount(v) = if v == none { "–" } else { v }
#let lines(v) = if v == none { "–" } else { v.split("\n").join(linebreak()) }

#bookmark-at(1, "Angebote")
#table(
  columns: (8mm, 1fr, 24mm, 25mm, 25mm, 23mm, 25mm, 27mm, 15mm, 38mm),
  stroke: table-stroke(),
  inset: (x: 3pt, y: 4pt),
  align: (x, _) => if x in (4, 5, 6, 7) { right + top } else { left + top },
  table.header(
    head("Nr."),
    head("Bieter"),
    head("Eingang"),
    head("Status"),
    head("Angebot netto", align: right),
    head("Nachlass", align: right),
    head("netto nach Nachlass", align: right),
    head(data.vat_label, align: right),
    head("Neben-angebote"),
    head("Bemerkungen"),
    table.hline(stroke: 1pt + accent-rule),
  ),
  ..data.rows.map(r => (
    {
      bookmark(r.at("bm", default: none))
      r.nr
    },
    {
      strong(r.name)
      if r.place != "" { linebreak() + text(fill: muted, r.place) }
    },
    r.received,
    r.status,
    amount(r.net),
    lines(r.discount),
    amount(r.after),
    strong(amount(r.gross)),
    r.side,
    r.remark,
    table.hline(stroke: thin),
  )).flatten(),
)

#if data.rows.len() == 0 {
  text(fill: muted, style: "italic", "Keine Bieter erfasst.")
}

#for note in data.notes {
  v(2mm)
  par(note)
}

#v(1fr)
#block(breakable: false, {
  bookmark-at(1, "Unterschriften")
  v(14mm)
  grid(
    columns: data.signatures.map(_ => 1fr),
    column-gutter: 12mm,
    ..data.signatures.map(s => {
      line(length: 100%, stroke: 0.6pt + muted)
      text(size: 8pt, fill: muted, s)
    }),
  )
})
