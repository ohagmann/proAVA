// Aufmaßprüfung: the planner's check of one invoice per position.
#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header

#show: setup.with(hd, landscape: true)

#title-block(hd)

#let warn-fill = rgb("#fbecd3")

#bookmark(data.at("bm", default: none))
#text(size: 11pt, weight: "bold", fill: heading, data.invoice)
#v(3mm)
#if data.facts.len() > 0 {
  let half = calc.ceil(data.facts.len() / 2)
  grid(
    columns: (38mm, 1fr, 38mm, 1fr),
    column-gutter: 4mm,
    row-gutter: 2.5mm,
    ..range(half).map(i => {
      let left = data.facts.at(i)
      let right = data.facts.at(i + half, default: none)
      (text(fill: muted, left.label), left.text) + if right == none { ([], []) } else {
        (text(fill: muted, right.label), right.text)
      }
    }).flatten(),
  )
  v(5mm)
}

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, hyphenate: false, body))
#let num(v) = if v == none or v == "" { [] } else { v }

#set text(size: 7.5pt)
#table(
  columns: (21mm, 1fr, 9mm, 15mm, 15mm, 15mm, 15mm, 17mm, 20mm, 20mm, 17mm, 10mm, 24mm),
  stroke: table-stroke(),
  inset: (x: 2.5pt, y: 3pt),
  align: (x, _) => if x in (3, 4, 5, 6, 7, 8, 9, 10, 11) { right + top } else { left + top },
  table.header(
    head("OZ"),
    head("Kurztext"),
    head("Einh."),
    head("Auftrag", align: right),
    head("bisher geprüft", align: right),
    head("Rechnung " + data.sense, align: right),
    head("geprüft " + data.sense, align: right),
    head("EP netto", align: right),
    head("Betrag Rechnung", align: right),
    head("Betrag geprüft", align: right),
    head("Differenz", align: right),
    head("Stand", align: right),
    head("Quelle"),
    table.hline(stroke: 1pt + accent-rule),
  ),
  ..data.rows.map(r => {
    if r.t == "title" {
      let fill = title-fills.at(calc.min(r.level, title-fills.len() - 1))
      (
        table.cell(fill: fill, {
          bookmark(r.at("bm", default: none))
          strong(r.oz)
        }),
        table.cell(fill: fill, colspan: 7, strong(r.text)),
        table.cell(fill: fill, strong(r.claimed)),
        table.cell(fill: fill, strong(r.checked)),
        table.cell(fill: fill, r.difference),
        table.cell(fill: fill, colspan: 2, []),
      )
    } else {
      let fill = if r.mark == "error" { high-fill } else if r.mark == "warn" { warn-fill } else { none }
      let body = {
        if r.change_order { text(fill: muted, "Nachtrag · ") }
        r.text
        if r.ordered == none { text(fill: muted, " (nicht im Auftrag)") }
        for note in r.notes {
          linebreak()
          text(style: "italic", note)
        }
      }
      (
        table.cell(fill: fill, {
          bookmark(r.at("bm", default: none))
          r.oz
        }),
        table.cell(fill: fill, body),
        table.cell(fill: fill, r.unit),
        table.cell(fill: fill, num(r.ordered)),
        table.cell(fill: fill, num(r.previous)),
        table.cell(fill: fill, num(r.claimed)),
        table.cell(fill: fill, strong(r.checked)),
        table.cell(fill: fill, num(r.price)),
        table.cell(fill: fill, num(r.claimed_total)),
        table.cell(fill: fill, strong(r.checked_total)),
        table.cell(fill: fill, num(r.difference)),
        table.cell(fill: fill, num(r.reached)),
        table.cell(fill: fill, r.basis),
        table.hline(stroke: thin),
      )
    }
  }).flatten(),
)

#if data.rows.len() == 0 {
  text(fill: muted, style: "italic", "Keine Positionen in dieser Rechnung.")
}

#set text(size: 8.5pt)
#v(4mm)
#bookmark-at(1, "Summen und Befunde")
#block(breakable: false, grid(
  columns: (1fr, 110mm),
  column-gutter: 10mm,
  {
    text(
      size: 7.5pt,
      fill: muted,
      "Mengen und Beträge " + data.sense + ", Beträge netto vor Nachlass. Farbig hinterlegt: Position mit "
        + "Befund (orange Warnung, rot Fehler), Einzelheiten unter dem Kurztext. „bisher geprüft“: kumulierte "
        + "geprüfte Menge der vorigen Rechnungen. „Stand“: geprüfte Menge kumuliert in Prozent der Auftragsmenge.",
    )
    if data.findings.len() > 0 {
      v(3mm)
      text(weight: "bold", fill: heading, "Befunde zur Rechnung")
      for f in data.findings {
        linebreak()
        f
      }
    }
    if data.remarks.len() > 0 {
      v(3mm)
      for r in data.remarks {
        r
        linebreak()
      }
    }
  },
  table(
    columns: (1fr, auto, 30mm),
    stroke: table-stroke(),
    inset: (x: 3pt, y: 3pt),
    align: (left, right, right),
    ..data.totals.map(l => {
      let cells = (
        if l.strong { strong(l.label) } else { l.label },
        text(fill: muted, l.extra),
        if l.amount == none { [] } else if l.strong { strong(l.amount) } else { l.amount },
      )
      if l.strong { (table.hline(stroke: 0.6pt + accent-rule),) + cells } else { cells }
    }).flatten(),
  ),
))
