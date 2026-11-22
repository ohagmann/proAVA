// Schreiben an Bieter (Serienbrief), DIN 5008 Form B vereinfacht.
#import "/common.typ": *

#let data = json("/data.json")

#set document(title: unmarked(data.title))
#set text(font: "Liberation Sans", size: 10.5pt, lang: "de", region: "DE")
#show: break-chances
#show: bookmarks
#set par(justify: false, leading: 0.6em, spacing: 0.9em)
#set page(
  paper: "a4",
  margin: (left: 25mm, right: 20mm, top: 20mm, bottom: 22mm),
  footer-descent: 35%,
  footer: oa-frame-foot(data.header),
)

#for (index, letter) in data.letters.enumerate() {
  if index > 0 { pagebreak() }
  counter(page).update(1)
  // PDF-Lesezeichen: one per letter (recipient).
  bookmark(letter.at("bm", default: none))
  // Briefkopf: Absender rechts, Logo darüber
  block(height: 25mm, width: 100%, grid(
    columns: (1fr, auto),
    [],
    align(right, {
      if data.logo != none and data.logo.t == "img" { fitted-image(data.logo.path, 50mm, 12mm); v(1mm) }
      set text(size: 8.5pt, fill: muted)
      letter.sender.join(linebreak())
    }),
  ))
  // Anschriftfeld (45 mm vom Blattrand) und Bezugszeichen rechts daneben
  place(top + left, dy: 25mm, block(width: 85mm, height: 45mm, {
    text(size: 7pt, fill: muted, letter.return_line)
    v(2mm)
    letter.recipient.join(linebreak())
  }))
  place(top + left, dx: 100mm, dy: 30mm, block(width: 65mm, grid(
    columns: (auto, 1fr),
    column-gutter: 3mm,
    row-gutter: 1.6mm,
    ..letter.references.map(r => (text(size: 8.5pt, fill: muted, r.label), text(size: 9pt, r.value))).flatten(),
  )))
  v(58mm)
  text(weight: "bold", letter.subject)
  v(5mm)
  blocks(letter.body)
  v(10mm)
  letter.signature.join(linebreak())
  // End of this letter: „{seiten}“ in the footer counts up to here.
  [#metadata(index) <oa-part-end>]
}
