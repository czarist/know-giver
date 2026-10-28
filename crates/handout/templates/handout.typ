// Study handout layout. All text comes from /data.json, so nothing in the
// content is ever interpreted as Typst markup.

#let data = json("/data.json")
#let labels = data.labels
#let accent = rgb("#2F6F68")
#let warm = rgb("#9A6B1F")
#let muted = luma(110)

#set document(title: data.title)
#set text(font: "Libertinus Serif", size: 11pt, lang: data.lang)
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

#let letter(i) = "ABCDEFGH".at(i)

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

#set page(
  paper: "a4",
  margin: (x: 2.3cm, top: 2.6cm, bottom: 2.4cm),
  numbering: "1",
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
  par(topic.introduction)

  for section in topic.sections {
    heading(level: 2, section.heading)
    for paragraph in section.paragraphs {
      par(paragraph)
    }
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
      block(breakable: false, above: 1em)[
        #text(weight: "bold", str(n + 1) + ". ")#exercise.prompt
        #pad(left: 1.2em)[
          #for (i, option) in exercise.options.enumerate() [
            #letter(i)) #option \
          ]
        ]
      ]
    }
    heading(level: 2, labels.answers, outlined: false, numbering: none)
    for (n, exercise) in topic.exercises.enumerate() {
      par[
        #text(weight: "bold", str(n + 1) + ". " + letter(exercise.correct) + " ")
        #exercise.explanation
      ]
    }
  }
}
