// Printable mock exam: question booklet, essay sheet and answer card. All
// text comes from /data.json, so nothing in it is read as Typst markup.
// The answer card's geometry comes from data.card, in millimetres, so the
// app can find the marks on a photo of it.

#let data = json("/data.json")
#let labels = data.labels
#let card = data.card
#let ink = luma(20)
#let muted = luma(100)
#let rule = luma(190)

#set document(title: data.title)
#set text(font: "Libertinus Serif", size: 10.5pt, lang: lower(data.lang.split("-").at(0)), fill: ink)
#set par(justify: true, leading: 0.62em, spacing: 0.9em)

#set page(
  paper: "a4",
  margin: (x: 1.8cm, top: 2.2cm, bottom: 2cm),
  header: context {
    if here().page() > 1 {
      set text(size: 8.5pt, fill: muted)
      data.title
      h(1fr)
      labels.code + " " + card.code
      v(-6pt)
      line(length: 100%, stroke: 0.4pt + rule)
    }
  },
  footer: context {
    set text(size: 8.5pt, fill: muted)
    align(center, counter(page).display())
  },
)

// Cover with the rules of the sitting.
#block(width: 100%, stroke: 1pt + ink, inset: 14pt)[
  #text(size: 9pt, tracking: 0.12em, upper(labels.booklet))
  #v(6pt)
  #text(size: 20pt, weight: "bold", data.title)
  #v(2pt)
  #text(size: 11pt, data.subtitle)
  #v(10pt)
  #grid(
    columns: (1fr, 1fr, 1fr),
    gutter: 8pt,
    [#text(size: 8.5pt, fill: muted, labels.duration) \ #text(weight: "bold", data.duration)],
    [#text(size: 8.5pt, fill: muted, labels.questions) \ #text(weight: "bold", str(data.total))],
    [#text(size: 8.5pt, fill: muted, labels.code) \ #text(weight: "bold", card.code)],
  )
  #v(10pt)
  #labels.name: #box(width: 1fr, line(length: 100%, stroke: 0.5pt + ink))
]
#v(8pt)
#text(weight: "bold", labels.instructions)
#list(..data.instructions.map(item => [#item]))
#v(6pt)

// Questions, two columns, numbered across the whole booklet.
#columns(2, gutter: 16pt)[
  #for section in data.sections {
    block(
      width: 100%,
      fill: luma(235),
      inset: (x: 6pt, y: 4pt),
      above: 10pt,
      below: 8pt,
      text(weight: "bold", size: 10pt, section.subject),
    )
    for question in section.questions {
      block(breakable: true, below: 10pt)[
        #text(weight: "bold", str(question.number) + ".")
        #question.prompt
        #if question.options.len() > 0 {
          v(2pt)
          for (i, option) in question.options.enumerate() {
            block(above: 3pt, below: 3pt, grid(
              columns: (16pt, 1fr),
              text(weight: "bold", "(" + data.marks.at(i) + ")"),
              option,
            ))
          }
        }
      ]
    }
  }
]

// Essay: the proposal, then a lined sheet with numbered lines.
#if data.essay != none {
  let essay = data.essay
  pagebreak()
  text(size: 9pt, tracking: 0.12em, upper(labels.essay))
  v(4pt)
  text(size: 15pt, weight: "bold", essay.theme)
  v(6pt)
  for paragraph in essay.statement.split("\n").filter(p => p.trim() != "") {
    par(paragraph)
  }
  if essay.instructions != "" {
    v(4pt)
    block(fill: luma(240), inset: 8pt, width: 100%, text(size: 9.5pt, essay.instructions))
  }
  pagebreak()
  text(weight: "bold", labels.essay_sheet)
  h(1fr)
  text(size: 9pt, fill: muted, labels.code + " " + card.code)
  v(6pt)
  let per-page = 30
  let pages = calc.ceil(essay.lines / per-page)
  for p in range(pages) {
    if p > 0 {
      pagebreak()
    }
    let first = p * per-page
    let last = calc.min(essay.lines, first + per-page)
    grid(
      columns: (18pt, 1fr),
      rows: 7.6mm,
      column-gutter: 6pt,
      align: (right + bottom, bottom),
      ..range(first, last)
        .map(n => (text(size: 8pt, fill: muted, str(n + 1)), line(length: 100%, stroke: 0.4pt + rule)))
        .flatten(),
    )
  }
}

// Answer card: one page, fixed positions, four corner markers.
#page(margin: 0pt, header: none, footer: none)[
  #let mm = 1mm
  #let marker(x, y) = place(top + left, dx: x * mm, dy: y * mm,
    rect(width: card.marker * mm, height: card.marker * mm, fill: black))
  #marker(card.margin, card.margin)
  #marker(210 - card.margin - card.marker, card.margin)
  #marker(card.margin, 297 - card.margin - card.marker)
  #marker(210 - card.margin - card.marker, 297 - card.margin - card.marker)

  #place(top + left, dx: 26mm, dy: 14mm, block(width: 158mm)[
    #text(size: 9pt, tracking: 0.12em, upper(labels.answer_card))
    #h(1fr)
    #text(size: 9pt, labels.code + " " + card.code)
    #v(2pt)
    #text(size: 13pt, weight: "bold", data.title)
    #v(6pt)
    #labels.name: #box(width: 1fr, line(length: 100%, stroke: 0.5pt + ink))
    #v(4pt)
    #text(size: 8.5pt, fill: muted, labels.card_hint)
  ])

  #for q in range(data.total) {
    let column = calc.floor(q / card.rows)
    let row = calc.rem(q, card.rows)
    let x = card.left + column * card.column_width
    let y = card.top + row * card.row_pitch
    place(top + left, dx: x * mm, dy: (y - card.bubble / 2) * mm,
      box(width: card.number_width * mm, height: card.bubble * mm,
        align(right + horizon, text(size: 8pt, weight: "bold", str(q + 1)))))
    for (i, mark) in data.marks.enumerate() {
      let cx = x + card.number_width + card.first_gap + i * card.bubble_pitch
      place(top + left, dx: (cx - card.bubble / 2) * mm, dy: (y - card.bubble / 2) * mm,
        circle(radius: card.bubble / 2 * mm, stroke: 0.6pt + ink,
          align(center + horizon, text(size: 6pt, fill: muted, mark))))
    }
  }
]
