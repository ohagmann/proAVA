#import "/common.typ": *
#import "/cover.typ": cover-page

// Small word under the mark of an Ausführungsbeschreibung („A0001“).
#let performance-word = "Ausführungsbeschr."

#let data = json("/data.json")
#let hd = data.header
// Währung of the LV behind amounts and in the column heads („€“, „USD“, „CHF“).
#let cur = hd.at("cur", default: "€")
#let priced = data.priced
#let opt = data.at("options", default: (
  landscape: false, font_size: 9, title_break: false, summary: true, summary_only: false,
  header_text: "", footer_text: "",
))

#show: setup.with(hd, landscape: opt.landscape)
#set text(size: opt.font_size * 1pt)

// Deckblatt (templates/cover.typ) statt des Titelblocks; das LV beginnt
// auf einer neuen Seite.
#let cover = data.at("cover", default: none)
#if cover != none { cover-page(hd, cover) } else { title-block(hd) }

#if data.at("parties", default: ()).len() > 0 {
  grid(
    columns: (45mm, 1fr),
    row-gutter: 3mm,
    ..data.parties.map(p => (text(fill: muted, p.label), p.text)).flatten(),
  )
  v(6mm)
}

#let amount(v) = if priced { if v == none { "–" } else { v } } else { dotted-blank }
// Sum of a title; one without positions shows „Nur Textinformation“.
#let sum-amount(r, bold: false) = if r.at("text_only", default: false) {
  text(size: 8pt, fill: muted, "Nur Textinformation")
} else if bold { strong(amount(r.sum)) } else { amount(r.sum) }

// „Übertrag“: sum of the GPs printed since the last title start or title
// sum, at the bottom of a page and the top of the next. Marks with the page
// they land on (not a running state: a position split over two pages is laid
// out before the table footer of the first page).
#let carry-add(cents) = [#metadata(cents) <oa-carry>]
#let carry-reset = [#metadata(none) <oa-carry>]
// Sum of the marks up to `page` (with it when `inclusive`): the marks before
// the first one that lies after that page, back to the last reset. The marks
// stand in the table rows, so their pages never fall along the marks (the mark
// of a position whose end moves to the next page lies there, like the rows
// after it): a binary search finds the first one after the page with a dozen
// page lookups. Every header and footer cell asks the same query, which Typst
// computes once per layout pass. A pass over all marks with their pages in
// every cell made Typst track pages × positions page lookups (2000 positions
// needed more than 16 GB).
#let carry-upto(page, inclusive) = {
  let limit = if inclusive { page } else { page - 1 }
  let marks = query(<oa-carry>)
  let (lo, hi) = (0, marks.len())
  while lo < hi {
    let mid = calc.quo(lo + hi, 2)
    if marks.at(mid).location().page() <= limit { lo = mid + 1 } else { hi = mid }
  }
  let sum = 0
  let open = false
  let i = lo - 1
  while i >= 0 {
    let m = marks.at(i)
    if m.value == none { break }
    sum += m.value
    open = true
    i -= 1
  }
  (sum: sum, open: open)
}

#let money-text(cents) = {
  let c = calc.abs(cents)
  let digits = str(calc.quo(c, 100)).clusters()
  let grouped = ""
  for (i, d) in digits.enumerate() {
    if i > 0 and calc.rem(digits.len() - i, 3) == 0 { grouped += "." }
    grouped += d
  }
  let rest = calc.rem(c, 100)
  (if cents < 0 { "-" } else { "" }) + grouped + "," + (if rest < 10 { "0" } else { "" }) + str(rest)
}

#let carry-cell(bottom) = table.cell(colspan: 6, align: right, inset: 0pt, context {
  let c = carry-upto(here().page(), bottom)
  if c.open {
    pad(x: 3pt, y: 2pt, text(fill: muted, {
      "Übertrag: "
      if priced { box(width: 25mm, align(right, money-text(c.sum))) } else { box(width: 30mm, dotted-blank) }
    }))
  }
})

#let head-cell(body, align: left) = table.cell(align: align + bottom, text(weight: "bold", fill: heading, hyphenate: false, body))

#let price-cols = (18mm, 14mm, 23mm, 25mm)
#let price-width = 48mm

#let position(r) = {
  let gp = if r.excluded {
    if priced { if r.gp == none { "nur EP" } else { "(" + r.gp + ")" } } else { text(size: 8pt, fill: muted, "nur EP") }
  } else { amount(r.gp) }
  // The Übertrag grows where the GP is printed (end of the position).
  let numbers = {
    carry-add(r.at("cents", default: 0))
    grid(
      columns: (1fr, ..price-cols),
      inset: (x: 3pt, y: 2pt),
      align: (right, right, left, right, right),
      // Gitter: the price line in fields under Menge, Einheit, EP and GP.
      stroke: if grid-on { (x, _) => if x > 0 { (left: grid-thin, top: grid-thin) } } else { none },
      if r.excluded and priced { text(size: 7.5pt, fill: muted, "nicht in Summe") },
      if r.qty == none { "" } else { r.qty },
      r.unit,
      if r.at("lump", default: false) { [] } else { amount(r.ep) },
      gp,
    )
  }
  let text-part(body) = pad(left: 3pt, right: price-width + 3pt, body)
  // EP-Aufgliederung (Lohn, Stoffe, Geräte …): blank fields or the values.
  let part-list = r.at("parts", default: ())
  let parts = if part-list.len() > 0 {
    align(right, pad(right: 3pt, y: 2pt, {
      for (index, part) in part-list.enumerate() {
        if index > 0 { h(5mm) }
        text(fill: muted, part.label + " ")
        if part.value == none { box(width: 20mm, dotted-blank) } else { part.value }
        " " + cur
      }
    }))
  }
  let head = {
    strong(r.short)
    if r.labels.len() > 0 {
      linebreak()
      text(size: 8pt, style: "italic", fill: heading, r.labels.join(" · "))
    }
    // „Ausführung nach Ausführungsbeschreibung 3 – …“, „Position entfällt“ …
    for remark in r.at("remarks", default: ()) {
      linebreak()
      text(style: "italic", remark)
    }
  }
  // Mengenermittlung (Druckoption „mit Mengenermittlung“): Ort, Rechenansatz,
  // Ergebnis je Zeile und die Summe – schlicht, in der Textfarbe.
  let takeoff-list = r.at("takeoff", default: ())
  let takeoff = if takeoff-list.len() > 0 {
    block(above: 0.6em, below: 0.3em, {
      text(weight: "bold", "Mengenermittlung")
      v(0.2em)
      grid(
        columns: (1fr, auto, 24mm),
        column-gutter: 4mm,
        row-gutter: 0.35em,
        ..for line in takeoff-list {
          (
            line.label,
            line.formula,
            align(right, if line.result == none { "Fehler" } else { line.result + " " + r.unit }),
          )
        },
        grid.hline(stroke: thin),
        grid.cell(colspan: 2, align(right, "Summe")),
        align(right, strong(r.at("takeoff_sum", default: "") + " " + r.unit)),
      )
    })
  }
  // Kostengruppe/Leistungsbereich at the right of the short text, above the prices.
  let codes = r.at("codes", default: ())
  let head-part = if codes.len() == 0 { text-part(head) } else {
    grid(
      columns: (1fr, price-width),
      pad(left: 3pt, right: 3pt, head),
      pad(right: 3pt, align(right, text(size: 8pt, codes.join(h(3mm))))),
    )
  }
  // The last text block stays with the price line – unless it is too long for a
  // page: a block that cannot break runs over the end of the page and the rest of
  // the text is missing from the PDF (a long Langtext as one paragraph with many
  // line breaks, as other AVA programs deliver it). Measured with a narrow width,
  // so the estimate errs on the breakable side.
  // The clear cases come decided from the font metrics (`keep` from keep.rs):
  // measuring every position cost a second layout of each (2000: 0.5 GB).
  let keep-with-prices(above: auto, body) = if r.at("keep", default: none) == none {
    context {
      let tall = measure(body, width: 100mm).height > 110mm
      block(breakable: tall, above: above, body)
    }
  } else {
    block(breakable: r.keep == "tall", above: above, body)
  }
  let n = r.blocks.len()
  if n == 0 {
    if takeoff == none {
      block(breakable: false, { head-part; parts; numbers })
    } else {
      block(sticky: true, below: 0.5em, head-part)
      keep-with-prices({ text-part(takeoff); parts; numbers })
    }
  } else {
    block(sticky: true, below: 0.5em, head-part)
    if n > 1 { text-part(blocks(r.blocks.slice(0, n - 1))) }
    // Same distance as between the paragraphs of the text before.
    keep-with-prices(above: text-spacing, {
      text-part(blocks(r.blocks.slice(n - 1)))
      if takeoff != none { text-part(takeoff) }
      parts
      numbers
    })
  }
}

// Row separators are the top stroke of a row (not a line under it): Typst
// leaves them out at the top of a page, so the bottom of every page has only
// the footer rule, no second line (Anwender 17.11.).
#let lv-rows(rows) = {
  let cells = ()
  let after-row = false
  // A title starts a new Übertrag – reset in the first row after it, not in
  // the title header itself (that repeats on every page).
  let reset-next = false
  for r in rows {
    // Gitter: every cell framed (the position as one block, its text without
    // lines inside).
    let sep = if grid-on { grid-thin } else if after-row { (top: thin) } else { none }
    after-row = r.t == "pos" or r.t == "note"
    let reset = reset-next and after-row
    if r.t == "title" { reset-next = true } else if after-row { reset-next = false }
    if r.t == "title" {
      let size = if r.level == 1 { 10.5pt } else { 9.5pt }
      let fill = title-fill-of(r.level)
      cells.push(table.header(
        level: r.level + 1,
        // PDF-Lesezeichen in the OZ column (the reader lands at the left edge).
        table.cell(fill: fill, inset: (x: 3pt, y: 5pt), {
          bookmark-once(r.at("bm", default: none))
          text(size: size, weight: "bold", r.oz)
        }),
        table.cell(colspan: 5, fill: fill, inset: (x: 3pt, y: 5pt), {
          // For {titel} in the page header and the Inhaltsverzeichnis.
          [#metadata((level: r.level, oz: r.oz, text: r.text, label: r.at("label", default: ""))) <oa-title>]
          let label = r.at("label", default: "")
          if label != "" { text(size: size - 1pt, fill: muted, label + "  ") }
          text(size: size, weight: "bold", r.text)
        }),
      ))
    } else if r.t == "pos" {
      cells.push(table.cell(stroke: sep, inset: (x: 3pt, top: 6pt, bottom: 4pt), {
        if reset { carry-reset }
        bookmark(r.at("bm", default: none))
        strong(r.oz)
      }))
      cells.push(table.cell(colspan: 5, stroke: sep, inset: (x: 0pt, top: 6pt, bottom: 4pt), position(r)))
    } else if r.t == "note" and r.at("preliminary", default: false) {
      // Vorbemerkung: over the full width from the OZ column, without a
      // mark – its heading says what it is (as AVA printouts set it).
      cells.push(table.cell(colspan: 6, stroke: sep, inset: (x: 3pt, top: 6pt, bottom: 4pt), {
        if reset { carry-reset }
        bookmark(r.at("bm", default: none))
        blocks(r.blocks)
      }))
    } else if r.t == "note" {
      // Hinweis / Ausführungsbeschreibung: quiet in the text flow, indented
      // like the position text, without a bar (Anwender 17.11.).
      let performance = r.at("performance", default: none)
      cells.push(table.cell(stroke: sep, inset: (x: 3pt, top: 6pt, bottom: 4pt), {
        if reset { carry-reset }
        bookmark(r.at("bm", default: none))
        // Ausführungsbeschreibung: its mark like an OZ:
        // „A0003“ with a small „Ausführungsbeschr.“ below.
        if performance != none {
          strong(performance)
          linebreak()
          text(size: 7pt, fill: muted, performance-word)
        }
      }))
      cells.push(table.cell(colspan: 5, stroke: sep, inset: (x: 3pt, top: 6pt, bottom: 4pt), pad(right: price-width, {
        if performance == none {
          text(size: 8pt, fill: muted, "Hinweis")
          v(1pt)
        }
        blocks(r.blocks)
      })))
    } else if r.t == "sum" {
      let text-only = r.at("text_only", default: false)
      cells.push(table.cell(colspan: if text-only { 6 } else { 5 }, align: right, fill: sum-fill, stroke: sep, inset: (x: 3pt, y: 5pt), {
        carry-reset
        text(weight: "bold", (if r.at("lump", default: false) { "Pauschalsumme " } else { "Summe " }) + r.oz + "  " + r.text)
        // No amount column: the note may be wider than it.
        if text-only { h(6mm) + sum-amount(r) }
      }))
      if not text-only {
        cells.push(table.cell(align: right, fill: sum-fill, stroke: sep, inset: (x: 3pt, y: 5pt), sum-amount(r, bold: true)))
      }
      cells += spacer-cell(6, 4mm)
    }
  }
  cells
}

#let all-rows = data.chunks.flatten()
// The marks of the Ausführungsbeschreibungen („A0001“ and the small
// „Ausführungsbeschr.“ below) stand in the OZ column too and must not wrap.
#let has-performance = all-rows.any(r => r.t == "note" and r.at("performance", default: none) != none)
#let oz-entries = all-rows.filter(r => r.t == "title" or r.t == "pos" or (r.t == "note" and r.at("performance", default: none) != none)).map(r => (
  if r.t == "note" { r.performance } else { r.oz },
  (weight: "bold", size: if r.t != "title" { 9pt } else if r.level == 1 { 10.5pt } else { 9.5pt }),
)) + if has-performance { ((performance-word, (size: 7pt)),) } else { () }

// Inhaltsverzeichnis with the page of every title (from the title metadata)
// and its place for the link: (value, page, location).
#let title-pages() = {
  let seen = ()
  let found = ()
  for m in query(<oa-title>) {
    let key = m.value.oz + "|" + m.value.text
    if key not in seen {
      seen.push(key)
      found.push((m.value, m.location().page(), m.location()))
    }
  }
  found
}

#if opt.at("contents", default: false) and not opt.summary_only and all-rows.any(r => r.t == "title") {
  bookmark-at(1, "Inhaltsverzeichnis")
  text(size: 14pt, weight: "bold", fill: heading, "Inhaltsverzeichnis")
  v(4mm)
  context {
    let entries = title-pages()
    let summary = query(<oa-summary>).map(m => m.location()).first(default: none)
    let oz-w = oz-column-width(entries.map(((v, ..)) => (v.oz, (weight: "bold"))), min: 14mm)
    table(
      columns: (oz-w + 4mm * calc.max(0, ..entries.map(((v, ..)) => v.level - 1)), auto, 1fr, 12mm),
      // Separator above each row (none at the page bottom, see lv-rows).
      stroke: table-stroke(off: (_, y) => if y > 1 { (top: thin) }),
      inset: (x: 3pt, y: 3.2pt),
      table.header(
        head-cell("Nr."), head-cell(""), head-cell("Bezeichnung"), head-cell("Seite", align: right),
        table.hline(stroke: 1pt + accent-rule),
      ),
      // Every entry is a link to its title in the PDF.
      ..entries.map(((v, page, place)) => {
        let top = v.level == 1
        let style(body) = link(place, if top { strong(body) } else { body })
        (
          pad(left: (v.level - 1) * 4mm, style(v.oz)),
          link(place, text(fill: muted, v.label)),
          style(v.text),
          table.cell(align: right, style(str(page))),
        )
      }).flatten(),
      ..if summary != none {
        let style(body) = link(summary, strong(body))
        ([], [], style("Zusammenstellung"), table.cell(align: right, style(str(summary.page()))))
      } else { () },
    )
  }
  pagebreak()
}

#if not opt.summary_only {
  for (index, rows) in data.chunks.enumerate() {
    if index > 0 { pagebreak(weak: true) }
    context table(
      columns: (oz-column-width(oz-entries), 1fr, ..price-cols),
      stroke: table-stroke(),
      inset: (x: 3pt, y: 3pt),
      align: left + top,
      table.header(
        head-cell("OZ"),
        head-cell("Leistung"),
        head-cell("Menge", align: right),
        head-cell("Einheit"),
        head-cell("EP " + cur, align: right),
        head-cell("GP " + cur, align: right),
        table.hline(stroke: 1pt + accent-rule),
        ..if opt.at("carry", default: false) { (carry-cell(false),) } else { () },
      ),
      ..lv-rows(rows),
      ..if opt.at("carry", default: false) { (table.footer(repeat: true, carry-cell(true)),) } else { () },
    )
  }
}

#if opt.summary {
  if not opt.summary_only {
    pagebreak(weak: true)
    [#metadata("summary") <oa-summary>]
    bookmark-at(1, "Zusammenstellung")
    text(size: 14pt, weight: "bold", fill: heading, "Zusammenstellung")
    v(3mm)
  } else {
    bookmark-at(1, "Zusammenstellung")
  }
  let with-pages = opt.at("contents", default: false) and not opt.summary_only
  // Column „Anteil“: each title sum in % of the net total (print option),
  // left of „Betrag“ – the amounts stay right, under the GP and above the totals.
  let with-share = data.summary.any(s => s.at("share", default: none) != none)
  context {
    let pages = if with-pages { title-pages() } else { () }
    let page-of(s) = pages.find(((v, ..)) => v.oz == s.oz and v.text == s.text)
    table(
      columns: (
        oz-column-width(data.summary.map(s => (s.oz, (weight: "bold"))), min: 18mm) + 4mm * calc.max(0, ..data.summary.map(s => s.level - 1)),
        1fr,
        ..if with-pages { (12mm,) } else { () },
        ..if with-share { (16mm,) } else { () },
        35mm,
      ),
      stroke: table-stroke(off: (_, y) => if y > 1 { (top: thin) }),
      inset: (x: 3pt, y: 4pt),
      table.header(
        head-cell("OZ"),
        head-cell("Bezeichnung"),
        ..if with-pages { (head-cell("Seite", align: right),) } else { () },
        ..if with-share { (head-cell("Anteil", align: right),) } else { () },
        head-cell("Betrag " + cur, align: right),
        table.hline(stroke: 1pt + accent-rule),
      ),
      ..data.summary.map(s => {
        let bold(body) = if s.level == 1 { strong(body) } else { body }
        let label = s.at("label", default: "")
        (
          pad(left: (s.level - 1) * 4mm, bold(s.oz)),
          pad(left: (s.level - 1) * 4mm, {
            if label != "" { text(fill: muted, label + "  ") }
            bold(s.text)
            if s.at("lump", default: false) { text(fill: muted, "  (pauschal)") }
          }),
          ..if with-pages {
            let found = page-of(s)
            // The page number jumps to the title.
            (table.cell(align: right, if found == none { [] } else { link(found.at(2), text(fill: muted, str(found.at(1)))) }),)
          } else { () },
          ..if with-share {
            let share = s.at("share", default: none)
            (table.cell(align: right, if share == none { [] } else { bold(share) }),)
          } else { () },
          table.cell(align: right, {
            sum-amount(s, bold: s.level == 1)
            // Room of „ €“ as in the totals below: the digits stand under each other.
            if priced { hide(" " + cur) }
          }),
        )
      }).flatten(),
    )
  }
}

#v(4mm)
// Without Zusammenstellung the sums stand alone at the end.
#if not opt.summary { bookmark-at(1, "Gesamtsumme") }

#let total-line(label, value, bold: false) = (
  table.cell(colspan: 2, align: right, if bold { strong(label) } else { label }),
  table.cell(align: right, if bold { strong(value) } else { value }),
)

#let t = data.totals
#table(
  columns: (25mm, 1fr, 35mm),
  stroke: table-stroke(),
  inset: (x: 3pt, y: 4pt),
  ..if priced and t.at("lump", default: none) != none {
    (
      ..total-line("Summe der Positionen", t.positions),
      ..total-line("Pauschalsumme netto", t.lump, bold: true),
    )
  } else {
    total-line("Summe netto", if priced { t.net } else { dotted-blank }, bold: true)
  },
  ..if priced and t.discount != none {
    (
      ..total-line(t.discount.label, t.discount.amount),
      ..total-line("Summe netto nach Nachlass", t.discount.after, bold: true),
    )
  } else { () },
  ..total-line(data.vat_label, if priced { t.vat } else { dotted-blank }),
  table.hline(start: 2, stroke: 1pt + accent-rule),
  ..total-line("Summe brutto", if priced { t.gross } else { dotted-blank }, bold: true),
)

// Schlussbemerkungen (e.g. a „Schlussblatt“ of the planner): after the
// totals, each on a page of its own, as the planner's AVA prints them.
#for c in data.at("closing", default: ()) {
  pagebreak()
  bookmark(c.bm)
  blocks(c.blocks)
}

#if not priced {
  v(14mm)
  bookmark-at(1, "Angaben des Bieters")
  text(size: 11pt, weight: "bold", fill: heading, "Angaben des Bieters")
  v(4mm)
  let field(label) = (text(fill: muted, label), dotted-blank)
  grid(
    columns: (45mm, 1fr),
    row-gutter: 9mm,
    ..field("Firma"),
    ..field("Anschrift"),
    ..field(""),
    ..field("Ansprechpartner / Telefon"),
  )
  v(18mm)
  grid(
    columns: (1fr, 1fr),
    column-gutter: 15mm,
    {
      line(length: 100%, stroke: 0.6pt + muted)
      text(size: 8pt, fill: muted, "Ort, Datum")
    },
    {
      line(length: 100%, stroke: 0.6pt + muted)
      text(size: 8pt, fill: muted, "Rechtsverbindliche Unterschrift und Stempel des Bieters")
    },
  )
}

