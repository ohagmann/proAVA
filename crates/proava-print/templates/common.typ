// Shared layout for all proAVA printouts. All texts come from /data.json as strings
// and are placed as values, never evaluated as markup.

// ---------- ANPASSEN: Farben aller Ausdrucke ----------
// Druckstil und Akzentfarbe stellt man in proAVA ein (Druckdialog bzw.
// Extras ▸ Einstellungen ▸ Drucken); alle übrigen Farben leiten sich hier ab.
//   Klassisch („classic“, Vorgabe): ausschließlich schwarze Schrift auf weißem
//     Papier (auch Hilfstexte und Markierungen), Linien schwarz bzw. grau;
//     Farbe nur als Hinterlegung (Titelzeilen in der Akzentfarbe, je Stufe
//     abgestuft; Preisspiegel-Markierungen).
//   proAVA („proava“): Überschriften und Spaltenköpfe in der Akzentfarbe,
//     Kopf-/Fußzeile grau, nur die oberste Titelstufe hinterlegt.
#let oa-look = {
  let data = json("/data.json")
  if type(data) == dictionary { data.at("header", default: (:)).at("look", default: (:)) } else { (:) }
}
#let classic = oa-look.at("style", default: "classic") != "proava"
// Zeilen- und Absatzabstand der Texte (Langtext, Hinweise …), wie im Texteditor
// (Extras ▸ Einstellungen ▸ Darstellung). „einfach“ = Zeilenhöhe der Schrift:
// Arial/Liberation Sans 1,15 × Schriftgröße, wie AVA-Ausdrucke. Typst misst eine
// Zeile von der Versalhöhe (0,688 em) bis zur Grundlinie; der Rest ist Durchschuss.
// Vorgabe ohne Absatzabstand: Leerzeilen sind leere Absätze des Textes.
#let text-pitch = oa-look.at("line", default: 1.0) * 1.15em
#let text-leading = text-pitch - 0.688em
#let text-gap = {
  let gap = oa-look.at("gap", default: 0)
  if gap == "half" { 0.5 * text-pitch } else { gap * 1pt }
}
#let text-spacing = text-leading + text-gap
// Akzentfarbe (Vorgabe Petrolblau, RGB 0/79/124): Flächen; im Stil proAVA auch
// Schrift und Linien.
#let accent = rgb(oa-look.at("accent", default: "#004F7C"))
// Fläche wie die Akzentfarbe mit `percent` Deckkraft auf weißem Papier.
#let tint(percent) = color.mix((accent, percent), (white, 100% - percent), space: rgb)
// Überschriften, Spaltenköpfe, Dokumentart.
#let heading = if classic { black } else { accent }
// Beschriftungen und Hilfstexte („Hinweis“, „Los“, Feldnamen).
#let muted = if classic { black } else { rgb("#5b6470") }
// Text der Kopf- und Fußzeile.
#let frame-ink = if classic { black } else { muted }
// Kräftige Linien (unter Tabellenköpfen, über Summen, unter dem Dokumentkopf).
#let accent-rule = if classic { black } else { accent }
// Dünne Trennlinien zwischen den Zeilen.
#let rule-color = if classic { rgb("#b8b8b8") } else { rgb("#c3c9d2") }
// Titelzeilen: oberste Stufe, tiefere Stufen (none = ohne Fläche).
#let title-fills = if classic { (tint(12%), tint(7%)) } else { (tint(10%), none) }
// Summenzeilen und Kästen.
#let sum-fill = tint(5%)
// Preisspiegel: niedrigster/höchster Preis, fehlende Preise.
#let low-fill = rgb("#d9efd9")
#let low-text = if classic { black } else { rgb("#1b5e20") }
#let high-fill = rgb("#f7d9d9")
#let high-text = if classic { black } else { rgb("#9b1c1c") }
#let missing-text = if classic { black } else { rgb("#b45309") }
// Blattrahmen (Druckdialog je Dokumentart, Vorgabe in den Einstellungen):
// Lage der Linie zum Blattrand, Abstand des Textes zur Linie, Strichstärken;
// Kopf- und Fußzeile stehen senkrecht mittig in ihrem Feld.
#let border-style = (
  edge: 10mm,                 // oben, rechts, unten
  edge-left: 20mm,            // links (Heftrand), A4 hoch
  edge-left-landscape: 15mm,  // links, quer
  inset: 5mm,                 // Text zur senkrechten Linie
  gap: 4mm,                   // Trennlinie Kopf/Fuß zum Inhalt
  thin: 0.5pt,
  medium: 1pt,
  on-cover: true,             // Deckblatt ebenfalls gerahmt (ohne Kopf, mit Fuß)
)
#let border-color = if classic { black } else { accent }
// ---------- Ende ANPASSEN ----------

#let border = oa-look.at("border", default: "none")
#let bordered = border == "thin" or border == "medium"
#let border-stroke = (if border == "medium" { border-style.medium } else { border-style.thin }) + border-color
// Height of the header and footer zone inside the frame (two lines or the logo).
#let border-zone(f) = 5mm + 2.5 * f.size * 1pt

#let title-fill = title-fills.at(0)
#let title-fill-of(level) = if level <= 1 { title-fills.at(0) } else { title-fills.at(1) }

#let thin = 0.5pt + rule-color

// Gitter-Tabelle (voll umrandet): Außenrahmen, alle senkrechten und alle
// waagrechten Linien, dünn und dunkelgrau. Druckoption „Tabellen voll
// umrandet (Gitter)“ (`grid-on`, Einstellungen ▸ Drucken, im Druckdialog je
// Dokumentart): jede Tabelle nimmt `stroke: table-stroke(off: …)` – an das
// Gitter, aus ihre eigenen Linien. Kräftiger unter dem Spaltenkopf
// (`grid-head`) und um Summenzeilen (`grid-sum`) als `table.hline`.
#let grid-on = oa-look.at("grid", default: false)
#let grid-ink = rgb("#3a3a3a")
#let grid-thin = 0.4pt + grid-ink
#let grid-sum = 0.7pt + grid-ink
#let grid-head = 0.9pt + (if classic { black } else { accent })
#let table-stroke(off: none) = if grid-on { grid-thin } else { off }
// Leerzeile zwischen Blöcken (nach Summen): im Gitter keine (sie stünde als
// leerer Kasten da).
#let spacer-cell(colspan, height) = if grid-on { () } else { (table.cell(colspan: colspan, inset: 0pt, v(height)),) }

#let dotted-blank = box(width: 100%, height: 0.8em, align(bottom, line(
  length: 100%,
  stroke: (paint: muted, thickness: 0.7pt, dash: "dotted"),
)))

#let runs(rs) = {
  for r in rs {
    let args = (
      weight: if r.b { "bold" } else { "regular" },
      style: if r.i { "italic" } else { "normal" },
    )
    if r.at("f", default: none) != none { args.insert("font", r.f) }
    if r.at("pt", default: none) != none { args.insert("size", r.pt * 1pt) }
    let c = text(..args, r.s)
    // Lücke einer Bieterangabe: one box, so the dots never break apart.
    let c = if r.at("gap", default: false) { box(c) } else { c }
    if r.u { underline(c) } else { c }
  }
}

#let aligned(name) = if name == "center" { center } else if name == "right" { right } else { left }

// Paragraph with alignment; "justify" sets Blocksatz.
#let aligned-par(name, body) = {
  if name == "justify" {
    par(justify: true, body)
  } else if name == "left" or name == none {
    par(body)
  } else {
    align(aligned(name), par(body))
  }
}

#let fitted-image(path, max-width, max-height) = context {
  let size = measure(image(path))
  let k = calc.min(max-width / size.width, max-height / size.height, 1.0)
  image(path, width: size.width * k)
}

// Ausschnitt aus einer PDF-Seite: the page as vector graphics, moved and
// cut to the rectangle (points from the top-left corner of the page).
#let pdf-clip(b) = block(breakable: false, above: 0.8em, below: 0.8em, layout(size => {
  let natural = b.width * 1pt
  let wanted = if b.width_mm != none { b.width_mm * 1mm } else { calc.min(natural, 150mm) }
  let shown = calc.min(wanted, size.width)
  let k = shown / natural
  // A block (not an inline box): `place` is relative to it.
  block(width: shown, height: b.height * k * 1pt, clip: true, above: 0pt, below: 0pt,
    place(top + left, dx: -b.left * k * 1pt, dy: -b.top * k * 1pt,
      // Both sizes and „stretch“: else Typst fits a page taller than the
      // frame into it and crops it in the middle.
      image(b.path, page: b.page, width: b.page_width * k * 1pt, height: b.page_height * k * 1pt, fit: "stretch")))
  if b.caption != none and b.caption != "" {
    v(2pt)
    text(size: 0.9em, fill: muted, style: "italic", b.caption)
  }
}))

#let picture(b) = {
  if b.t == "pdfclip" { return pdf-clip(b) }
  if b.t == "missing" {
    text(fill: high-text, style: "italic", "[Bild „" + b.name + "“ kann nicht dargestellt werden]")
  } else {
    block(breakable: false, above: 0.8em, below: 0.8em, layout(size => {
      let natural = measure(image(b.path)).width
      let wanted = if b.width_mm != none { b.width_mm * 1mm } else { calc.min(natural, 90mm) }
      image(b.path, width: calc.min(wanted, size.width))
      if b.caption != none and b.caption != "" {
        v(2pt)
        text(size: 0.9em, fill: muted, style: "italic", b.caption)
      }
    }))
  }
}

#let blocks(bs) = {
  // Lines of a paragraph, paragraphs and list items all at the same
  // distance (plus the Abstand nach Absatz); tables and images keep theirs.
  set par(leading: text-leading, spacing: text-spacing)
  set block(spacing: text-spacing)
  for b in bs {
    if b.t == "p" {
      // An empty paragraph is an empty line of the text.
      if b.runs.len() == 0 { par(hide[X]) } else { aligned-par(b.at("align", default: "left"), runs(b.runs)) }
    } else if b.t == "li" {
      // Unterpunkte one step in (the bullet under the text of the item
      // above); numbered items („1.“, „a)“, „i.“) with a wider column.
      let marker = b.at("marker", default: none)
      pad(left: b.level * 4mm, grid(
        columns: (if marker == none { 3.5mm } else { 6mm }, 1fr),
        if marker != none { marker } else [•],
        aligned-par(b.at("align", default: "left"), runs(b.runs)),
      ))
    } else if b.t == "table" and b.columns > 0 {
      let fixed = b.widths.filter(w => w != none)
      let columns = if fixed.len() == 0 { b.columns } else {
        b.widths.map(w => if w == none { 1fr } else { w * 1mm })
      }
      let cell(c, head) = table.cell(
        align: aligned(c.align) + top,
        par(justify: c.align == "justify", if head { strong(runs(c.runs)) } else { runs(c.runs) }),
      )
      let head-rows = b.rows.slice(0, b.header)
      let body-rows = b.rows.slice(b.header)
      block(above: 0.6em, below: 0.8em, table(
        columns: columns,
        stroke: thin,
        inset: (x: 4pt, y: 3pt),
        fill: (_, y) => if y < b.header { title-fill },
        ..if b.header > 0 { (table.header(..head-rows.map(row => row.map(c => cell(c, true))).flatten()),) },
        ..body-rows.map(row => row.map(c => cell(c, false))).flatten(),
      ))
    } else if b.t == "img" or b.t == "missing" or b.t == "pdfclip" {
      picture(b)
    }
  }
}

// Break chances: proAVA marks places in long words (file names, OZ chains,
// „DDC-Automationsstation“) with U+200B (format::breakable). Printed as an empty
// box they still allow the line break (an inline object is a break opportunity
// before and after, UAX #14 LB20), but leave no character in the PDF text, so
// copied text stays clean. `unmarked` removes them from plain strings
// (document title).
#let break-mark = "\u{200B}"
#let unmarked(s) = s.replace(break-mark, "")
#let break-chances(body) = {
  show break-mark: box()
  body
}

// PDF-Lesezeichen (outline): an invisible heading where the reader should
// land. `bm` is (level: n, text: "01.481 Automationseinrichtungen") or none;
// the text is plain (no formatting). `bookmarks` (called in `setup`) prints
// headings as nothing: no box, no text, no space – only their place counts.
// `heading` in these templates is a colour (above), hence `std.heading`.
#let bookmark(bm) = if bm != none and bm.text != "" {
  std.heading(level: bm.level, outlined: false, bookmarked: true, numbering: none, unmarked(bm.text))
}
#let bookmark-at(level, text) = bookmark((level: level, text: text))
// Rows of a repeated table header (LV titles) are laid out again on every
// page; only the first layout sets the bookmark. `bm.id` tells the rows apart.
#let bookmark-once(bm) = if bm != none {
  [#metadata(bm.id) <oa-bookmark>]
  context {
    // A selector, not `filter` with a function: Typst caches every function
    // call, and a repeated header asks on every page (an LV with long texts
    // over 1500 pages: 1.4 GB and 20 s more).
    let same = selector(<oa-bookmark>).and(metadata.where(value: bm.id))
    if query(same.before(here())).len() == 1 { bookmark(bm) }
  }
}
#let bookmarks(body) = {
  show std.heading: none
  body
}

#let page-number = context [Seite #counter(page).display() von #counter(page).final().first()]

// Cuts a text with "…" so that it takes at most `lines` lines at `width` (needs context).
#let fit-lines(body, width, lines: 2, style: (:)) = {
  let height(t) = measure(block(width: width, text(..style, t))).height
  let limit = height(range(lines).map(_ => "Ag").join("\n")) + 0.5pt
  if height(body) <= limit { return body }
  let chars = body.clusters()
  let (lo, hi) = (1, chars.len())
  while lo < hi {
    let mid = calc.div-euclid(lo + hi + 1, 2)
    if height(chars.slice(0, mid).join() + "…") <= limit { lo = mid } else { hi = mid - 1 }
  }
  chars.slice(0, lo).join().trim() + "…"
}

// ---------- Kopf- und Fußzeile (parametrierbar) ----------
// hd.frame: three fields per side, each up to two lines of segments; text is
// already resolved, „page“, „pages“ and „title“ are filled here.

// Deepest title that started on an earlier page or at the top of this one
// (LV templates mark title rows with metadata <oa-title>): the last title
// mark before 55 mm on this page. The marks stand in the table rows, top to
// bottom and page by page, so a binary search finds it with a dozen page
// lookups; a pass over all marks with their pages on every page made Typst
// track pages × titles lookups (2000 positions: 5 GB more).
#let oa-current-title() = {
  let this-page = here().page()
  let marks = query(<oa-title>)
  let (lo, hi) = (0, marks.len())
  while lo < hi {
    let mid = calc.quo(lo + hi, 2)
    let loc = marks.at(mid).location()
    if loc.page() < this-page or (loc.page() == this-page and loc.position().y < 55mm) {
      lo = mid + 1
    } else {
      hi = mid
    }
  }
  if lo == 0 { "" } else {
    let v = marks.at(lo - 1).value
    (v.oz + " " + v.text).trim()
  }
}

// Pages of the current part: a document of several letters marks each end
// with <oa-part-end>; otherwise the whole document.
#let oa-total-pages() = {
  let this-page = here().page()
  let ends = query(<oa-part-end>).filter(m => m.location().page() >= this-page)
  if ends.len() > 0 { counter(page).at(ends.first().location()).first() } else {
    counter(page).final().first()
  }
}

#let oa-line(segs) = segs.map(s => {
  if s.t == "s" { s.s } else if s.t == "page" { str(counter(page).get().first()) } else if s.t == "pages" {
    str(oa-total-pages())
  } else { oa-current-title() }
}).join("")

// One side (head or foot) as a three-column grid; long texts are cut with
// „…“ instead of running into the neighbour. `style(i, j)`: text style of
// field i, line j. Needs context.
#let oa-frame-row(fields, f, hd, style, at-top: false, with-logo: false) = layout(size => {
  let gutter = 4mm
  let lines = fields.map(field => field.map(oa-line).filter(l => l.trim() != ""))
  let logo = if with-logo and f.logo != "off" and hd.logo != none and hd.logo.t == "img" {
    fitted-image(hd.logo.path, 40mm, calc.max(2, lines.map(l => l.len()).fold(1, calc.max)) * f.size * 1.3pt)
  } else { none }
  let logo-w = if logo == none { 0pt } else { measure(logo).width }
  let width = size.width - if logo == none { 0pt } else { logo-w + gutter }
  let natural(i) = lines.at(i).enumerate().map(((j, l)) => measure(text(..style(i, j), l)).width).fold(0pt, calc.max)
  let centre = calc.min(natural(1), width * 0.5)
  let side = calc.max(0pt, (width - centre - 2 * gutter) / 2)
  let cell(i, w, al) = align(al + if at-top { top } else { bottom }, stack(
    dir: ttb,
    spacing: 4pt,
    ..lines.at(i).enumerate().map(((j, l)) => text(..style(i, j), fit-lines(l, w, lines: 1, style: style(i, j)))),
  ))
  let cells = (cell(0, side, left), cell(1, centre, center), cell(2, side, right))
  let columns = (side, centre, side)
  if logo != none and f.logo == "left" {
    cells = (align(if at-top { top } else { bottom }, logo),) + cells
    columns = (logo-w,) + columns
  } else if logo != none {
    cells = cells + (align(right + if at-top { top } else { bottom }, logo),)
    columns = columns + (logo-w,)
  }
  grid(columns: columns, column-gutter: gutter, ..cells)
})

#let oa-frame-head(hd) = context {
  let f = hd.frame
  // The cover sheet (cover.typ) has no page header.
  let on-cover = query(<oa-cover>).any(m => m.location().page() == here().page())
  if bordered and not on-cover {
    // Blattrahmen: the header fills its zone, its text sits in the middle
    // between frame line and separator; the separator runs from frame line
    // to frame line.
    let b = border-style
    block(height: border-zone(f), width: 100%, {
      place(bottom + left, dx: -b.inset, line(length: 100% + 2 * b.inset, stroke: border-stroke))
      align(horizon, {
        set text(size: f.size * 1pt, fill: frame-ink)
        set block(spacing: 0pt)
        oa-frame-row(f.head, f, hd, (i, j) => if i == 1 and j == 0 { (weight: "bold", fill: heading) } else if i == 1 {
          (fill: heading)
        } else { (:) }, at-top: true, with-logo: true)
      })
    })
  } else if not on-cover {
    set text(size: f.size * 1pt, fill: frame-ink)
    set block(spacing: 0pt)
    // First lines side by side (as before): fields align at the top.
    oa-frame-row(f.head, f, hd, (i, j) => if i == 1 and j == 0 { (weight: "bold", fill: heading) } else if i == 1 {
      (fill: heading)
    } else { (:) }, at-top: true, with-logo: true)
    // About one blank line between text and rule (Anwender 17.11.).
    if f.head_rule {
      v(1.2em)
      line(length: 100%, stroke: thin)
    }
  }
}

#let oa-frame-foot(hd) = context {
  let f = hd.frame
  set text(size: f.size * 1pt, fill: frame-ink)
  set block(spacing: 0pt)
  if bordered {
    // Like without the frame, the footer is on the cover sheet as well; its
    // text sits in the middle between separator and frame line.
    let b = border-style
    block(height: border-zone(f), width: 100%, {
      place(top + left, dx: -b.inset, line(length: 100% + 2 * b.inset, stroke: border-stroke))
      align(horizon, oa-frame-row(f.foot, f, hd, (i, j) => (:), at-top: true))
    })
  } else {
    if f.foot_rule {
      line(length: 100%, stroke: thin)
      v(1em)
    }
    oa-frame-row(f.foot, f, hd, (i, j) => (:), at-top: true)
  }
}

#let setup(hd, landscape: false, a3: false, body) = {
  set document(title: unmarked(hd.kind + if hd.project != "" { " – " + hd.project } else { "" }), author: unmarked(hd.author))
  set text(font: "Liberation Sans", size: 9pt, lang: "de", region: "DE")
  // Header, footer and content: break chances without a character.
  show: break-chances
  show: bookmarks
  set par(justify: false, leading: 0.5em, spacing: 0.7em)
  let b = border-style
  let left-edge = if landscape { b.edge-left-landscape } else { b.edge-left }
  let zone = border-zone(hd.frame)
  set page(
    paper: if a3 { "a3" } else { "a4" },
    flipped: landscape,
    margin: if bordered {
      (left: left-edge + b.inset, right: b.edge + b.inset, top: b.edge + zone + b.gap, bottom: b.edge + zone + b.gap)
    } else if landscape { (left: 15mm, right: 15mm, top: 22mm, bottom: 18mm) } else {
      (left: 22mm, right: 15mm, top: 24mm, bottom: 20mm)
    },
    header-ascent: if bordered { b.gap } else { 35% },
    footer-descent: if bordered { b.gap } else { 35% },
    header: oa-frame-head(hd),
    footer: oa-frame-foot(hd),
    // Blattrahmen: one line around header, content and footer.
    background: if bordered {
      context {
        let on-cover = query(<oa-cover>).any(m => m.location().page() == here().page())
        if b.on-cover or not on-cover {
          place(top + left, dx: left-edge, dy: b.edge, rect(
            width: 100% - left-edge - b.edge,
            height: 100% - 2 * b.edge,
            stroke: border-stroke,
          ))
        }
      }
    },
  )
  body
}

#let title-block(hd, extra: ()) = {
  let facts = (
    ("Projekt", hd.project),
    ("Projektbezeichnung", hd.project_title),
    ("Auftraggeber", hd.client),
    ("Leistungsverzeichnis", hd.lv),
    ("Datum", hd.date),
    ("Bearbeiter", hd.author),
    ..extra,
  ).filter(((_, v)) => v != "")
  grid(
    columns: (1fr, auto),
    gutter: 8mm,
    {
      text(size: 18pt, weight: "bold", fill: heading, hd.kind)
      if hd.project_title != "" {
        v(1pt)
        text(size: 11pt, fill: muted, hd.project_title)
      }
    },
    // Not twice: with the logo in the page header the title block has none.
    if hd.logo != none and hd.logo.t == "img" and hd.at("frame", default: (logo: "off")).logo == "off" {
      fitted-image(hd.logo.path, 60mm, 20mm)
    },
  )
  v(5mm)
  grid(
    columns: (38mm, 1fr),
    row-gutter: 5pt,
    ..facts.map(((k, v)) => (text(fill: muted, k), v)).flatten(),
  )
  v(4mm)
  line(length: 100%, stroke: 1pt + accent-rule)
  v(4mm)
}

// Dokumentkopf kompakt (Preisspiegel): Dokumentart und Projektbezeichnung
// links, ggf. das Logo rechts – ohne Angaben-Kasten, ohne Lücke und ohne
// Trennlinie quer über die Seite (Projekt, LV und Datum stehen in Kopf- und
// Fußzeile).
#let title-block-compact(hd) = {
  let logo = hd.logo != none and hd.logo.t == "img" and hd.at("frame", default: (logo: "off")).logo == "off"
  grid(
    columns: (1fr, ..if logo { (auto,) } else { () }),
    column-gutter: 6mm,
    align: top,
    stack(
      spacing: 3mm,
      text(size: 16pt, weight: "bold", fill: heading, hd.kind),
      ..if hd.project_title != "" { (text(size: 10.5pt, fill: muted, hd.project_title),) } else { () },
    ),
    ..if logo { (fitted-image(hd.logo.path, 50mm, 16mm),) } else { () },
  )
}

#let mark-cell(cell, base: none, bold: false, ..args) = {
  let (fill, color) = if cell.m == "low" { (low-fill, low-text) } else if cell.m == "high" {
    (high-fill, high-text)
  } else if cell.m == "missing" { (base, missing-text) } else { (base, black) }
  let body = if cell.m == "missing" { text(fill: color, style: "italic", "fehlt") } else {
    text(fill: color, if cell.v == none { "–" } else { cell.v })
  }
  table.cell(fill: fill, ..args, if bold { strong(body) } else { body })
}

// Width of the OZ column from the widest OZ actually printed (needs context).
#let oz-column-width(entries, min: 12mm, max: 48mm) = {
  let widest = 0pt
  for (oz, style) in entries {
    if oz != "" { widest = calc.max(widest, measure(text(..style, oz)).width) }
  }
  calc.min(calc.max(widest + 8pt, min), max)
}

