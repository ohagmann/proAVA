#import "/common.typ": *

#let data = json("/data.json")
#let hd = data.header
// Währung of the LV in the column heads.
#let cur = hd.at("cur", default: "€")
#let bidders = data.bidders
#let n = bidders.len()
#let t = data.totals

#show: setup.with(hd, landscape: true, a3: data.a3)

#title-block-compact(hd)
#v(5mm)

#let head(body, ..args) = table.cell(..args, text(weight: "bold", fill: heading, body))

// Übersicht der Angebote (links) und Legende (rechts daneben), beide als
// Gitter-Tabelle.
#bookmark-at(1, "Übersicht der Angebote")
#{
  set text(size: 8.5pt)
  let by-rank = bidders.sorted(key: b => if b.rank == none { 9999 } else { b.rank })
  let dated = bidders.any(b => b.at("date", default: "") != "")
  let span = if dated { 5 } else { 4 }
  let overview = table(
    stroke: grid-thin,
    columns: (auto, 1fr, ..if dated { (auto,) } else { () }, auto, auto),
    inset: (x: 4pt, y: 3pt),
    table.header(
      table.cell(colspan: span, text(size: 9.5pt, weight: "bold", fill: heading, "Übersicht der Angebote")),
      head("Rang"),
      head("Bieter"),
      ..if dated { (head("Angebot vom"),) } else { () },
      head(data.net_label, align: right),
      head("brutto", align: right),
      table.hline(stroke: grid-head),
    ),
    ..by-rank.map(b => (
      if b.rank == none { "–" } else { strong(str(b.rank)) },
      {
        strong(b.name)
        if b.note != "" { h(6pt) + text(fill: muted, b.note) }
      },
      ..if dated { (b.at("date", default: ""),) } else { () },
      table.cell(align: right, if b.at("net", default: none) == none { "–" } else { b.net }),
      table.cell(align: right, if b.gross == none { "–" } else { b.gross }),
    )).flatten(),
  )
  let swatch(fill) = box(width: 9mm, height: 0.8em, fill: fill, stroke: grid-thin)
  let entries = (
    (swatch(low-fill), "niedrigster Preis"),
    (swatch(high-fill), "höchster Preis"),
    (text(fill: missing-text, style: "italic", "fehlt"), "Preis vom Bieter nicht angegeben"),
    ..if data.outlier_note != "" { ((strong("?"), "Ausreißer: " + data.outlier_note),) } else { () },
    (strong("!"), "Rechenfehler: GP des Bieters ≠ Menge × EP"),
    (strong("%"), "Abstand zum günstigsten Preis der Zeile"),
    ([Mittelwert,\ Minimum], "über alle Bieter mit Preis"),
    ..if data.estimate { (([Planerpreis], "Preise des Planers (Kostenvoranschlag), nicht in der Wertung"),) } else { () },
  )
  let legend = {
    table(
      stroke: grid-thin,
      columns: (auto, 1fr),
      inset: (x: 4pt, y: 3pt),
      table.header(
        table.cell(colspan: 2, text(size: 9.5pt, weight: "bold", fill: heading, "Legende")),
      ),
      ..entries.map(((sign, meaning)) => (table.cell(align: center + horizon, sign), meaning)).flatten(),
    )
    v(1.5mm)
    text(size: 8pt, data.amounts_note)
  }
  grid(columns: (1fr, 95mm), column-gutter: 8mm, align: top, overview, legend)
}

#v(5mm)

#let paren(cell, excluded) = if excluded and cell.v != none { (v: "(" + cell.v + ")", m: cell.m) } else { cell }
// „?“ Ausreißer (EP), „!“ Rechenfehler (GP) behind the value.
#let with-flag(cell) = {
  let flag = cell.at("f", default: "")
  if flag != "" and cell.v != none { (v: cell.v + " " + flag, m: cell.m) } else { cell }
}
#let plain(v) = table.cell(align: right, if v == none { "–" } else { v })
// GP of Mittelwert, Minimum and Planerpreis in brackets like the bidders'
// when the position is not in the sum (Bedarf ohne GB, Wahl).
#let bracketed(v, excluded) = if excluded and v != none { "(" + v + ")" } else { v }
// Columns: OZ, Kurztext, Menge, Einheit, per bidder EP/GP/%, then pairs
// Mittelwert, Minimum and (only with planner prices) Planerpreis.
#let est = data.estimate
#let stats-all = if est { 3 } else { 2 }

#let discount-values = range(n).map(i => {
  let d = t.discount.at(i)
  if d == none { "–" } else {
    let label = t.discount_label.at(i)
    (if label != none and label != "Betrag" { label + ": " } else { "" }) + "-" + d
  }
})

#let widest(values, style) = {
  let w = 0pt
  for v in values {
    if v != none and v != "" { w = calc.max(w, measure(text(..style, v)).width) }
  }
  w
}

#let pos-rows = data.rows.filter(r => r.t == "pos")
#let title-rows = data.rows.filter(r => r.t == "title")
#let ep-values = pos-rows.map(r => r.ep.map(c => with-flag(c).v) + (r.mean_ep, r.min_ep, r.est_ep)).flatten()
#let gp-values = pos-rows.map(r => r.gp.map(c => with-flag(paren(c, r.excluded)).v) + (r.mean_gp, r.min_gp, r.est_gp).map(v => bracketed(v, r.excluded))).flatten()
#let pct-values = (pos-rows.map(r => r.pct) + data.rows.filter(r => r.t == "sum").map(r => r.pct) + (t.pct,)).flatten()
#let pair-values = (
  data.rows.filter(r => r.t == "sum").map(r => r.sums.map(c => c.v) + (r.mean, r.min, r.est)).flatten()
    + t.net.map(c => c.v) + t.after.map(c => c.v) + t.gross.map(c => c.v) + discount-values + t.cash_discount
    + (t.mean_net, t.min_net, t.mean_after, t.min_after, t.mean_gross, t.min_gross, t.est_net, t.est_gross)
)

// Spaltenaufteilung: the numbers keep their width, the Kurztext gets the
// rest but at least `short-min`. Too narrow: the font gets smaller (`sizes`),
// still too narrow: the bidders are split over several tables (Teil 1, 2 …),
// each with OZ, Kurztext, Menge, Einheit; Mittelwert, Minimum and Planerpreis
// in the last one. Widths are measured once at `ref` – text scales with its size.
#let ref = 10pt
#let sizes = (7.5pt, 7pt, 6.5pt, 6pt)
#let short-min = 45mm

#let plan(avail) = {
  let m-ep = widest(ep-values, (size: ref))
  let m-gp = widest(gp-values, (size: ref))
  let m-pair = widest(pair-values, (size: ref, weight: "bold"))
  let m-qty = widest(pos-rows.map(r => r.qty), (size: ref))
  let m-unit = widest(pos-rows.map(r => r.unit), (size: ref))
  let m-pct = widest(pct-values, (size: ref))
  let m-oz = widest(pos-rows.map(r => r.oz) + title-rows.filter(r => r.level != 1).map(r => r.oz), (size: ref, weight: "bold"))
  let m-oz1 = widest(title-rows.filter(r => r.level == 1).map(r => r.oz), (size: ref, weight: "bold"))
  let pad = 6pt
  let cols(fs) = {
    let s = fs / ref
    let k = fs / 7.5pt
    let ep = calc.max(12mm * k, m-ep * s + pad)
    let gp = calc.max(13mm * k, m-gp * s + pad)
    let pair = m-pair * s + pad
    if ep + gp < pair { gp = pair - ep }
    (
      fs: fs,
      oz: calc.min(calc.max(calc.max(m-oz * s, m-oz1 * ((fs + 1pt) / ref)) + 8pt, 10mm), 48mm),
      qty: calc.max(11mm * k, m-qty * s + pad),
      unit: calc.max(8mm * k, m-unit * s + pad),
      ep: ep,
      gp: gp,
      pct: calc.max(8mm * k, m-pct * ((fs - 1pt) / ref) + pad),
    )
  }
  let short-width(c, bidders, stats) = avail - c.oz - c.qty - c.unit - bidders * (c.ep + c.gp + c.pct) - stats * (c.ep + c.gp)
  // All bidders in one table, the font as large as possible.
  for fs in sizes {
    let c = cols(fs)
    if short-width(c, n, stats-all) >= short-min { return (c: c, parts: (range(n),)) }
  }
  // Split: as few tables as possible.
  for count in range(2, n + 1) {
    let per = calc.ceil(n / count)
    for fs in sizes {
      let c = cols(fs)
      if short-width(c, per, stats-all) >= short-min { return (c: c, parts: range(n).chunks(per)) }
    }
  }
  (c: cols(sizes.last()), parts: range(n).chunks(1))
}

// The Preisspiegel of the bidders `idx` (indices into `bidders`), with
// Mittelwert, Minimum, Planerpreis when `with-stats`: positions table and
// Angebotssummen.
#let part-tables(c, idx, with-stats) = {
  let fs = c.fs
  set text(size: fs)
  let nb = idx.len()
  let stats = if with-stats { stats-all } else { 0 }
  let span-all = 4 + 3 * nb + 2 * stats
  let pct-cell(v, ..args) = table.cell(align: right, ..args, text(size: fs - 1pt, fill: muted, if v == none { "" } else { v }))
  let columns = (
    c.oz,
    1fr,
    c.qty,
    c.unit,
    ..range(nb).map(_ => (c.ep, c.gp, c.pct)).flatten(),
    ..range(stats).map(_ => (c.ep, c.gp)).flatten(),
  )
  let heads = idx.map(i => {
    let b = bidders.at(i)
    table.cell(colspan: 3, align: center + bottom, {
      text(weight: "bold", fill: heading, fit-lines(b.name, c.ep + c.gp + c.pct - 5pt, style: (weight: "bold", size: fs)))
      if b.rank != none {
        linebreak()
        text(fill: muted, "Rang " + str(b.rank))
      }
    })
  })
  let stat-heads = if with-stats {
    (
      head("Mittelwert", colspan: 2, align: center + bottom),
      head("Minimum", colspan: 2, align: center + bottom),
      ..if est { (head("Planerpreis", colspan: 2, align: center + bottom),) } else { () },
    )
  } else { () }

  let rows = ()
  for r in data.rows {
    if r.t == "title" {
      // Gitter: every title level tinted as in the LV.
      let fill = if grid-on { title-fill-of(r.level) } else if r.level == 1 { title-fill } else { none }
      let size = if r.level == 1 { fs + 1pt } else { fs }
      rows.push(table.header(
        level: r.level + 1,
        table.cell(fill: fill, inset: (x: 3pt, y: 4pt), {
          // PDF-Lesezeichen once, although the title row repeats.
          bookmark-once(r.at("bm", default: none))
          text(size: size, weight: "bold", r.oz)
        }),
        table.cell(colspan: span-all - 1, fill: fill, inset: (x: 3pt, y: 4pt), text(size: size, weight: "bold", r.text)),
      ))
    } else if r.t == "pos" {
      rows.push(r.oz)
      rows.push({
        // Long words break with a hyphen instead of running into „Menge“.
        text(hyphenate: true, r.short)
        if r.labels.len() > 0 {
          linebreak()
          text(size: fs - 1pt, style: "italic", fill: heading, r.labels.join(" · ") + if r.excluded { " – nicht in Summe" } else { "" })
        }
        // Menge nach Angebotseingang geändert (Anwender 18.11.).
        if r.at("qty_note", default: "") != "" {
          linebreak()
          text(size: fs - 1pt, weight: "bold", r.qty_note)
        }
      })
      rows.push(table.cell(align: right, if r.qty == none { "" } else { r.qty }))
      rows.push(r.unit)
      for i in idx {
        rows.push(mark-cell(with-flag(r.ep.at(i)), align: right))
        rows.push(mark-cell(with-flag(paren(r.gp.at(i), r.excluded)), align: right))
        rows.push(pct-cell(r.pct.at(i)))
      }
      if with-stats {
        rows.push(plain(r.mean_ep))
        rows.push(plain(bracketed(r.mean_gp, r.excluded)))
        rows.push(plain(r.min_ep))
        rows.push(plain(bracketed(r.min_gp, r.excluded)))
        if est {
          rows.push(plain(r.est_ep))
          rows.push(plain(bracketed(r.est_gp, r.excluded)))
        }
      }
      if not grid-on { rows.push(table.hline(stroke: thin)) }
    } else if r.t == "sum" {
      let fill = sum-fill
      let inset = (x: 3pt, y: 4pt)
      if grid-on { rows.push(table.hline(stroke: grid-sum)) }
      rows.push(table.cell(colspan: 4, fill: fill, align: right, inset: inset, strong("Summe " + r.oz + "  " + r.text)))
      for i in idx {
        rows.push(mark-cell(r.sums.at(i), base: fill, bold: true, colspan: 2, align: right, inset: inset))
        rows.push(pct-cell(r.pct.at(i), fill: fill, inset: inset))
      }
      if with-stats {
        rows.push(table.cell(colspan: 2, fill: fill, align: right, inset: inset, strong(if r.mean == none { "–" } else { r.mean })))
        rows.push(table.cell(colspan: 2, fill: fill, align: right, inset: inset, strong(if r.min == none { "–" } else { r.min })))
        if est {
          rows.push(table.cell(colspan: 2, fill: fill, align: right, inset: inset, strong(if r.est == none { "–" } else { r.est })))
        }
      }
      if grid-on { rows.push(table.hline(stroke: grid-sum)) }
      rows += spacer-cell(span-all, 2.5mm)
    }
  }

  let total-row(label, values, mean: none, min: none, est-value: none, bold: false, marked: true, pct: none) = {
    let style(x) = if bold { strong(x) } else { x }
    let inset = (x: 3pt, y: 4pt)
    let cells = (table.cell(colspan: 4, align: right, inset: inset, style(label)),)
    for i in idx {
      let v = values.at(i)
      let cell = if marked { v } else { (v: v, m: "") }
      cells.push(mark-cell(cell, bold: bold, colspan: 2, align: right, inset: inset))
      cells.push(pct-cell(if pct == none { none } else { pct.at(i) }, inset: inset))
    }
    if with-stats {
      cells.push(table.cell(colspan: 2, align: right, inset: inset, style(if mean == none { "" } else { mean })))
      cells.push(table.cell(colspan: 2, align: right, inset: inset, style(if min == none { "" } else { min })))
      if est {
        cells.push(table.cell(colspan: 2, align: right, inset: inset, style(if est-value == none { "" } else { est-value })))
      }
    }
    cells
  }

  // Without the Gitter: thin lines between the bidders and the statistics.
  let vlines = if grid-on { () } else {
    (
      range(nb).map(i => table.vline(x: 4 + 3 * i, stroke: thin))
        + range(stats).map(k => table.vline(x: 4 + 3 * nb + 2 * k, stroke: thin))
    )
  }
  let head-rule = if grid-on { grid-head } else { 1pt + accent-rule }

  table(
    stroke: table-stroke(),
    columns: columns,
    inset: (x: 2.5pt, y: 3pt),
    align: left + top,
    table.header(
      head("OZ", rowspan: 2, align: bottom),
      head("Kurztext", rowspan: 2, align: bottom),
      head("Menge", rowspan: 2, align: right + bottom),
      head("Einh.", rowspan: 2, align: bottom),
      ..heads,
      ..stat-heads,
      ..range(nb).map(_ => (head("EP " + cur, align: right), head("GP " + cur, align: right), head("%", align: right))).flatten(),
      ..range(stats).map(_ => (head("EP " + cur, align: right), head("GP " + cur, align: right))).flatten(),
      table.hline(stroke: head-rule),
    ),
    ..vlines,
    ..rows,
  )

  v(4mm)
  block(breakable: false, table(
    stroke: table-stroke(),
    columns: columns,
    inset: (x: 2.5pt, y: 3pt),
    align: left + top,
    table.header(
      table.cell(colspan: 4, align: left + bottom, {
        bookmark-once((level: 1, text: "Angebotssummen", id: "sums"))
        text(size: fs + 2pt, weight: "bold", fill: heading, "Angebotssummen")
      }),
      ..heads,
      ..stat-heads,
      table.hline(stroke: head-rule),
    ),
    ..vlines,
    ..total-row("Summe netto", t.net, mean: t.mean_net, min: t.min_net, est-value: t.est_net, bold: true),
    ..total-row("Nachlass", discount-values, marked: false),
    ..total-row("Summe netto nach Nachlass", t.after, mean: t.mean_after, min: t.min_after, est-value: t.est_net, bold: true, pct: t.pct),
    ..total-row(data.vat_label, t.vat.map(v => if v == none { "–" } else { v }), est-value: t.est_vat, marked: false),
    table.hline(start: if grid-on { 0 } else { 4 }, stroke: if grid-on { grid-sum } else { thin }),
    ..total-row("Summe brutto", t.gross, mean: t.mean_gross, min: t.min_gross, est-value: t.est_gross, bold: true),
    ..total-row("Rang", t.rank.map(r => if r == none { "–" } else { str(r) }), marked: false, bold: true),
    // Massenmehrung/-minderung nach Angebotseingang: zur Information.
    ..if t.at("at_lv", default: none) != none {
      total-row("Netto n. N. mit aktueller LV-Menge (nur Information)", t.at_lv.map(c => if c.v == none { "–" } else { c.v }), marked: false)
    } else { () },
    ..if data.cash_discount_label != none {
      total-row(data.cash_discount_label, t.cash_discount.map(v => if v == none { "–" } else { v }), marked: false)
    } else { () },
  ))
}

#context {
  // Width between the margins (set as lengths in `setup`).
  let length(x) = if type(x) == relative { x.length } else { x }
  let paper = if page.flipped { page.height } else { page.width }
  let chosen = plan(paper - length(page.margin.left) - length(page.margin.right))
  let parts = chosen.parts
  for (k, idx) in parts.enumerate() {
    if parts.len() > 1 {
      if k > 0 { pagebreak(weak: true) }
      let last = k == parts.len() - 1
      block(below: 3mm, {
        text(size: 9pt, weight: "bold", fill: heading, "Teil " + str(k + 1) + " von " + str(parts.len()))
        text(size: 9pt, fill: muted, {
          "  Bieter " + str(idx.first() + 1)
          if idx.len() > 1 { "–" + str(idx.last() + 1) }
          " von " + str(n)
          if last { if est { ", Mittelwert, Minimum, Planerpreis" } else { ", Mittelwert, Minimum" } }
        })
      })
    }
    part-tables(chosen.c, idx, k == parts.len() - 1)
  }
  // Was nicht gewertet ist oder anders als angeboten (unbekannte OZ, Rechenfehler).
  let notes = data.at("bid_notes", default: ())
  if notes.len() > 0 {
    v(4mm)
    block(breakable: false, {
      text(size: 9pt, weight: "bold", fill: heading, "Hinweise zu den Angeboten")
      v(1.5mm)
      set text(size: 8pt)
      list(..notes)
    })
  }
  // Anhang: every deviation of bids with many (Abweichungsprotokoll).
  let appendix = data.at("appendix", default: ())
  if appendix.len() > 0 {
    pagebreak(weak: true)
    bookmark-once((level: 1, text: "Abweichungen der Angebote", id: "appendix"))
    text(size: 11pt, weight: "bold", fill: heading, "Anhang: Abweichungen der Angebote")
    v(1mm)
    text(size: 8pt, fill: muted, "Gewertet wird mit Menge und Text des LV; die Einträge sagen, was die Angebotsdatei anders hat.")
    v(3mm)
    set text(size: 7.5pt)
    for entry in appendix {
      block(breakable: true, {
        text(size: 9pt, weight: "bold", entry.name)
        h(4pt)
        text(entry.summary)
        v(1mm)
        table(
          columns: (8mm, 1fr),
          stroke: table-stroke(off: (_, y) => (bottom: thin)),
          inset: (x: 3pt, y: 2.5pt),
          ..entry.lines.enumerate().map(((i, line)) => (str(i + 1), line)).flatten(),
        )
      })
      v(4mm)
    }
  }
}
