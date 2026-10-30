// Study handout layout. All text comes from /data.json, so nothing in the
// content is ever interpreted as Typst markup. Pictures are files under /img.

#let data = json("/data.json")
#let labels = data.labels
#let accent = rgb("#2F6F68")
#let warm = rgb("#9A6B1F")
#let right = rgb("#3B7A4A")
#let muted = luma(110)
#let rule = luma(210)

#set document(title: data.title)
#set text(font: "Libertinus Serif", size: 11pt, lang: lower(data.lang.split("-").at(0)))
#set par(justify: true, leading: 0.68em, spacing: 1.05em)
#set list(indent: 0.6em, body-indent: 0.5em)
#set enum(indent: 0.6em, body-indent: 0.5em)
#set heading(numbering: "1.1")

#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  set text(size: 20pt, fill: accent, weight: "bold")
  block(below: 0.4em, it)
}
#show heading.where(level: 2): it => {
  set text(size: 13.5pt, fill: accent)
  block(above: 1.4em, below: 0.6em, it)
}

#let note(title, color, items) = block(
  width: 100%,
  fill: color.lighten(92%),
  stroke: (left: 3pt + color),
  inset: (x: 12pt, y: 10pt),
  radius: (right: 3pt),
  above: 1em,
  below: 1em,
)[
  #text(weight: "bold", fill: color, title)
  #list(..items.map(item => [#item]))
]

// A picture with a thin frame and its source underneath.
#let picture(pic, height: 5.5cm) = if pic != none {
  block(breakable: false, above: 1em, below: 1em, width: 100%)[
    #align(center)[
      #box(stroke: 0.6pt + rule, inset: 3pt, image(pic.file, height: height, fit: "contain"))
      #v(2pt)
      #text(size: 8.5pt, fill: muted, style: "italic", pic.caption)
    ]
  ]
}

#let letter(i) = "ABCDEFGH".at(i)

// A letter in a circle: outlined for options, filled for the right answer.
#let badge(content, color, filled: false) = box(
  baseline: 22%,
  circle(
    radius: 7.5pt,
    fill: if filled { color } else { none },
    stroke: 0.8pt + color,
    inset: 0pt,
  )[
    #set align(center + horizon)
    #text(size: 8.5pt, weight: "bold", fill: if filled { white } else { color }, content)
  ],
)

// A coloured band that opens a block of the topic.
#let band(title, color) = block(
  width: 100%,
  fill: color,
  inset: (x: 12pt, y: 7pt),
  above: 1.6em,
  below: 0.9em,
  radius: 2pt,
  text(fill: white, weight: "bold", size: 12pt, title),
)

// Cover
#page(numbering: none)[
  #v(1fr)
  #align(center)[
    #text(size: 11pt, fill: muted, tracking: 0.12em, upper(labels.handout))
    #v(14pt)
    #text(size: 30pt, weight: "bold", fill: accent, data.title)
    #v(8pt)
    #text(size: 14pt, data.subtitle)
    #v(28pt)
    #line(length: 30%, stroke: 0.6pt + accent)
    #v(10pt)
    #text(size: 10pt, fill: muted, data.generated)
  ]
  #v(1.2fr)
]

// The reader's highlights, over the text of their page.
#let marks = data.at("marks", default: ())

#set page(
  paper: "a4",
  margin: (x: 2.3cm, top: 2.6cm, bottom: 2.4cm),
  numbering: "1",
  foreground: context {
    let here-page = here().page()
    for mark in marks.filter(m => m.page == here-page) {
      for r in mark.rects {
        place(top + left, dx: r.at(0) * 1pt, dy: r.at(1) * 1pt,
          rect(width: r.at(2) * 1pt, height: r.at(3) * 1pt, fill: rgb(mark.color)))
      }
    }
  },
  header: context {
    set text(size: 8.5pt, fill: muted)
    data.title
    h(1fr)
    data.subtitle
    v(-6pt)
    line(length: 100%, stroke: 0.4pt + luma(200))
  },
)
#counter(page).update(1)

#outline(title: labels.contents, depth: 2, indent: 1.2em)

#for topic in data.topics {
  heading(level: 1, topic.topic)
  text(size: 10pt, fill: muted, topic.subject)
  v(0.6em)
  picture(topic.at("picture", default: none), height: 6cm)
  par(topic.introduction)

  for section in topic.sections {
    heading(level: 2, section.heading)
    for paragraph in section.paragraphs {
      par(paragraph)
    }
    picture(section.at("picture", default: none))
    if section.key_points.len() > 0 {
      note(labels.key_points, accent, section.key_points)
    }
  }

  if topic.examples.len() > 0 {
    heading(level: 2, labels.examples)
    for example in topic.examples {
      block(inset: (left: 10pt), stroke: (left: 1pt + luma(200)), par(example))
    }
  }
  if topic.pitfalls.len() > 0 {
    note(labels.pitfalls, warm, topic.pitfalls)
  }
  if topic.summary.len() > 0 {
    heading(level: 2, labels.summary)
    list(..topic.summary.map(item => [#item]))
  }

  if topic.exercises.len() > 0 {
    heading(level: 2, labels.exercises)
    for (n, exercise) in topic.exercises.enumerate() {
      block(
        breakable: false,
        width: 100%,
        above: 0.9em,
        below: 0.9em,
        inset: (x: 12pt, y: 10pt),
        radius: 3pt,
        fill: accent.lighten(95%),
        stroke: 0.6pt + accent.lighten(55%),
      )[
        #grid(
          columns: (auto, 1fr),
          column-gutter: 8pt,
          box(fill: accent, inset: (x: 6pt, y: 3pt), radius: 2pt,
            text(fill: white, weight: "bold", size: 10pt, str(n + 1))),
          par(justify: false, exercise.prompt),
        )
        #v(4pt)
        #for (i, option) in exercise.options.enumerate() {
          grid(
            columns: (auto, 1fr),
            column-gutter: 7pt,
            row-gutter: 0pt,
            badge(letter(i), accent),
            par(justify: false, option),
          )
          v(2pt)
        }
      ]
    }

    // Answer key: a quick grid, then each answer explained.
    block(breakable: false, below: 0.8em)[
      #band(labels.answers, right)
      #text(size: 9.5pt, fill: muted, labels.answer_grid)
      #v(2pt)
      #grid(
        columns: (1fr,) * calc.min(topic.exercises.len(), 5),
        column-gutter: 6pt,
        row-gutter: 6pt,
        ..topic.exercises.enumerate().map(((n, exercise)) => box(
          width: 100%,
          stroke: 0.6pt + right.lighten(40%),
          radius: 2pt,
          inset: (y: 5pt),
          align(center, text(weight: "bold")[#str(n + 1) · #text(fill: right, letter(exercise.correct))]),
        )),
      )
    ]
    for (n, exercise) in topic.exercises.enumerate() {
      block(breakable: false, above: 0.7em, below: 0.7em)[
        #grid(
          columns: (auto, auto, 1fr),
          column-gutter: 7pt,
          text(weight: "bold", str(n + 1) + "."),
          badge(letter(exercise.correct), right, filled: true),
          par(justify: false, exercise.explanation),
        )
      ]
    }
  }
}
