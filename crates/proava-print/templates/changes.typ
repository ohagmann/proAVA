#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd, landscape: data.at("landscape", default: false))

#title-block(hd, extra: (("Verglichen mit", data.against), ("Ergebnis", data.summary)))

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, body))
#let kind-color(kind) = if kind == "neu" { low-text } else if kind == "entfallen" { high-text } else { heading }

#if data.entries.len() == 0 [
  Keine Unterschiede.
] else {
  table(
    columns: (20mm, 30mm, 1fr),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3pt),
    table.header(
      head("Änderung"),
      head("OZ"),
      head("Text und Unterschiede"),
      table.hline(stroke: 1pt + accent-rule),
    ),
    ..data.entries.map(e => (
      {
        bookmark(e.at("bm", default: none))
        text(weight: "bold", fill: kind-color(e.kind), e.kind) + linebreak() + text(size: 7.5pt, fill: muted, e.row_kind)
      },
      {
        e.ordinal
        if e.old_ordinal != "" {
          linebreak()
          text(size: 7.5pt, fill: muted, "vorher " + e.old_ordinal)
        }
      },
      {
        e.text
        for d in e.details {
          linebreak()
          text(size: 8pt, fill: muted, d)
        }
      },
      table.hline(stroke: thin),
    )).flatten(),
  )
}
