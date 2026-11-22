// Deckblatt der LV-Ausdrucke: Form „Standard“ und „Briefkopf“, dazu die
// Folgeseite „Vertragsbedingungen“.
//
// ANPASSEN – alles Gestalterische des Deckblatts steht in diesem Abschnitt:
//   cover-style       Schriftgrößen, Abstände, Logo-Größe
//   cover-facts       Kenndaten der Form „Standard“: Reihenfolge und Beschriftung
//   cover-box-facts   Kenndaten im Kasten der Form „Briefkopf“
//   cover-body-facts  Kenndaten unter dem Titel der Form „Briefkopf“
//   cover-addresses   Adressblöcke: Reihenfolge und Beschriftung
//   cover-bidder      Texte des Bieterfelds (nur Angebotsaufforderung)
// Die Schlüssel (z. B. "vergabenummer") liefert proAVA; leere Werte lassen
// ihre Zeile weg. Welche Blöcke erscheinen (Logo, Adressen, Kenndaten,
// Bieterfeld), stellt man in proAVA unter Extras ▸ Einstellungen ▸ Drucken ein.
// Nach einer Änderung proAVA neu bauen (die Vorlage ist eingebettet).

#import "/common.typ": *

#let cover-style = (
  text-size: 9.5pt,
  label-size: 8pt,
  kind-size: 26pt,
  lv-size: 15pt,
  project-size: 11pt,
  heading-size: 10.5pt,
  logo-width: 42mm,
  logo-height: 16mm,
  top-space: 22mm,
  after-title: 10mm,
  after-addresses: 7mm,
  label-width: 42mm,
  row-gap: 2.6mm,
  field-gap: 6.5mm,
  signature-space: 12mm,
)

#let cover-facts = (
  ("projektnummer", "Projektnummer"),
  ("vergabeart", "Vergabeart"),
  ("vergabenummer", "Vergabenummer"),
  ("abgabetermin", "Abgabetermin"),
  ("abgabeort", "Abgabeort"),
  ("bindefrist", "Bindefrist"),
  ("ausfuehrung", "Ausführungsfrist"),
  ("weitere_fristen", "Weitere Fristen"),
  ("stand", "Stand"),
)

// Diese Kenndaten erscheinen fett.
#let cover-strong = ("abgabetermin",)

#let cover-box-facts = (
  ("vergabenummer", "Vergabe-Nr."),
  ("projektnummer", "Projekt-Nr."),
  ("abgabetermin", "Abgabe bis"),
  ("bindefrist", "Bindefrist"),
  ("ausfuehrung", "Ausführung"),
  ("stand", "Stand"),
)

#let cover-body-facts = (
  ("vergabeart", "Vergabeart"),
  ("abgabeort", "Abgabeort"),
  ("weitere_fristen", "Weitere Fristen"),
)

// (Schlüssel, Beschriftung, Beschriftung wenn es keinen eigenen Auftraggeber gibt)
#let cover-addresses = (
  ("bauherr", "Bauherr", "Bauherr / Auftraggeber"),
  ("auftraggeber", "Auftraggeber / Vergabestelle", "Auftraggeber / Vergabestelle"),
  ("planer", "Aufsteller / Planer", "Aufsteller / Planer"),
)

#let cover-bidder = (
  heading: "Vom Bieter auszufüllen",
  heading-letterhead: "Angaben des Bieters",
  bidder: "Bieter",
  address: "Anschrift",
  contact: "Ansprechpartner, Telefon",
  net: "Angebotssumme netto",
  gross: "Angebotssumme brutto",
  place-date: "Ort, Datum",
  signature: "Unterschrift und Stempel des Bieters",
)

// ---------- ab hier Aufbau ----------

#let cover-blank = box(width: 1fr, height: 0.8em, align(bottom, line(
  length: 100%,
  stroke: (paint: muted, thickness: 0.6pt, dash: "dotted"),
)))

#let cover-value(c, key) = c.facts.at(key, default: "")

#let cover-lines(c, key) = c.addresses.at(key, default: ())

#let cover-address-label(c, entry) = {
  let (key, label, alone) = entry
  if cover-lines(c, "auftraggeber").len() == 0 { alone } else { label }
}

#let cover-fact-rows(c, list, label-size: none) = {
  let rows = ()
  for (key, label) in list {
    let value = cover-value(c, key)
    if value.trim() != "" {
      let shown = value.split("\n").join(linebreak())
      rows.push(text(fill: muted, size: if label-size == none { 1em } else { label-size }, label))
      rows.push(if key in cover-strong { strong(shown) } else { shown })
    }
  }
  rows
}

#let cover-logo(hd, c) = if c.blocks.logo and hd.logo != none and hd.logo.t == "img" {
  fitted-image(hd.logo.path, cover-style.logo-width, cover-style.logo-height)
}

#let cover-signatures(left, right) = grid(
  columns: (1fr, 1fr),
  column-gutter: 12mm,
  [#line(length: 100%, stroke: 0.6pt + muted) #text(size: cover-style.label-size, fill: muted, left)],
  [#line(length: 100%, stroke: 0.6pt + muted) #text(size: cover-style.label-size, fill: muted, right)],
)

// Form „Standard“: linksbündig von oben nach unten, Bieterfeld gerahmt unten.
#let cover-standard(hd, c) = {
  let st = cover-style
  let planner = cover-lines(c, "planer")
  grid(
    columns: (1fr, auto),
    align: (left + bottom, right + top),
    if c.blocks.addresses and planner.len() > 0 {
      text(size: st.label-size + 1pt, fill: muted, planner.slice(0, calc.min(2, planner.len())).join(linebreak()))
    },
    cover-logo(hd, c),
  )
  v(st.top-space)
  if c.project != "" {
    text(size: st.project-size, fill: muted, c.project)
    v(2mm)
  }
  text(size: st.kind-size, weight: "bold", fill: heading, hd.kind)
  if c.lv != "" {
    v(1mm)
    text(size: st.lv-size, c.lv)
  }
  if c.measure != "" {
    v(3mm)
    text(fill: muted, c.measure.split("\n").join(linebreak()))
  }
  v(st.after-title)
  let blocks = cover-addresses.filter(e => cover-lines(c, e.at(0)).len() > 0)
  if c.blocks.addresses and blocks.len() > 0 {
    line(length: 100%, stroke: 0.6pt + rule-color)
    v(5mm)
    grid(
      columns: range(blocks.len()).map(_ => 1fr),
      column-gutter: 8mm,
      ..blocks.map(e => [
        #text(size: st.label-size, fill: muted, cover-address-label(c, e)) \
        #cover-lines(c, e.at(0)).join(linebreak())
      ]),
    )
    v(st.after-addresses)
  }
  if c.blocks.facts {
    grid(columns: (st.label-width, 1fr), row-gutter: st.row-gap, ..cover-fact-rows(c, cover-facts))
  }
  v(1fr)
  if c.bidder {
    let b = cover-bidder
    block(width: 100%, stroke: 0.6pt + rule-color, inset: 5mm, breakable: false, {
      text(size: st.heading-size, weight: "bold", fill: heading, b.heading)
      v(4mm)
      grid(
        columns: (1fr, 82mm),
        column-gutter: 8mm,
        grid(
          columns: (22mm, 1fr),
          row-gutter: st.field-gap,
          text(fill: muted, b.bidder), cover-blank, [], cover-blank,
          text(fill: muted, b.address), cover-blank, [], cover-blank,
        ),
        grid(
          columns: (44mm, 1fr),
          row-gutter: st.field-gap,
          align: (right, left),
          text(fill: muted, b.net), [#cover-blank #c.at("cur", default: "€")],
          text(fill: muted, c.vat_label), [#cover-blank #c.at("cur", default: "€")],
          text(weight: "bold", b.gross), [#cover-blank #c.at("cur", default: "€")],
        ),
      )
      v(st.signature-space)
      cover-signatures(b.place-date, b.signature)
    })
  }
}

// Form „Briefkopf“: Vergabestelle oben links, Kenndaten im Kasten rechts,
// Bieterangaben als Formularzeilen.
#let cover-letterhead(hd, c) = {
  let st = cover-style
  let planner = cover-lines(c, "planer")
  grid(
    columns: (1fr, auto),
    align: (left + top, right + top),
    if c.blocks.addresses and planner.len() > 0 {
      text(size: st.label-size, fill: muted, planner.slice(0, calc.min(4, planner.len())).join(linebreak()))
    },
    cover-logo(hd, c),
  )
  v(12mm)
  let left = cover-addresses.filter(e => e.at(0) != "planer" and cover-lines(c, e.at(0)).len() > 0).rev()
  let box-rows = cover-fact-rows(c, cover-box-facts, label-size: 8.5pt)
  grid(
    columns: (1fr, 66mm),
    column-gutter: 10mm,
    if c.blocks.addresses {
      for (index, e) in left.enumerate() {
        if index > 0 { v(4mm) }
        text(size: st.label-size, fill: muted, cover-address-label(c, e))
        linebreak()
        cover-lines(c, e.at(0)).join(linebreak())
      }
    },
    if c.blocks.facts and box-rows.len() > 0 {
      block(fill: sum-fill, inset: 4mm, width: 100%, grid(columns: (24mm, 1fr), row-gutter: 2.4mm, ..box-rows))
    },
  )
  v(16mm)
  text(size: st.kind-size - 2pt, weight: "bold", fill: heading, hd.kind)
  if c.lv != "" {
    v(1mm)
    text(size: st.lv-size - 2pt, c.lv)
  }
  v(3mm)
  if c.project != "" { text(size: st.project-size, c.project) }
  if c.measure != "" {
    linebreak()
    text(fill: muted, c.measure.split("\n").join(linebreak()))
  }
  v(6mm)
  if c.blocks.facts {
    let rows = cover-fact-rows(c, cover-body-facts)
    if c.blocks.addresses and planner.len() > 0 {
      rows += (text(fill: muted, "Aufsteller / Planer"), planner.slice(0, calc.min(2, planner.len())).join(linebreak()))
    }
    grid(columns: (st.label-width, 1fr), row-gutter: 2.5mm, ..rows)
  }
  v(1fr)
  if c.bidder {
    let b = cover-bidder
    text(size: st.heading-size, weight: "bold", fill: heading, b.heading-letterhead)
    v(1mm)
    let field(label, bold: false) = (
      if bold { text(weight: "bold", label) } else { text(fill: muted, label) },
      cover-blank,
    )
    table(
      columns: (48mm, 1fr),
      stroke: (x, y) => (bottom: 0.5pt + rule-color),
      inset: (x: 0pt, y: 3.4mm),
      ..field(b.bidder),
      ..field(b.address),
      ..field(b.contact),
      ..field(b.net + " (" + c.at("cur", default: "€") + ")"),
      ..field(c.vat_label + " (" + c.at("cur", default: "€") + ")"),
      ..field(b.gross + " (" + c.at("cur", default: "€") + ")", bold: true),
    )
    v(st.signature-space)
    cover-signatures(b.place-date, b.signature)
  }
}

// Folgeseite „Vertragsbedingungen“ (Sicherheiten, Skonto, Abzüge, Zahlungsbedingungen …).
#let cover-terms(c) = {
  let sections = c.sections.filter(s => s.rows.len() > 0)
  if sections.len() == 0 and c.notes.len() == 0 { return }
  pagebreak()
  bookmark-at(1, "Vertragsbedingungen")
  text(size: 14pt, weight: "bold", fill: heading, "Vertragsbedingungen")
  v(5mm)
  let value(lines) = lines.join(linebreak())
  for section in sections {
    block(breakable: false, below: 5mm, {
      text(size: 10.5pt, weight: "bold", fill: heading, section.heading)
      v(1.5mm)
      grid(
        columns: (48mm, 1fr),
        row-gutter: 2.2mm,
        ..section.rows.map(r => (text(fill: muted, r.label), value(r.lines))).flatten(),
      )
    })
  }
  for note in c.notes {
    block(below: 4mm, {
      text(size: 10.5pt, weight: "bold", fill: heading, note.label)
      v(1.5mm)
      value(note.lines)
    })
  }
}

// Deckblatt (eine Seite, ohne Seitenkopf), dann die Vertragsbedingungen;
// das LV beginnt auf einer neuen Seite.
#let cover-page(hd, c) = {
  [#metadata("cover") <oa-cover>]
  bookmark-at(1, "Deckblatt")
  set text(size: cover-style.text-size)
  if c.style == "letterhead" { cover-letterhead(hd, c) } else { cover-standard(hd, c) }
  cover-terms(c)
  pagebreak()
}
