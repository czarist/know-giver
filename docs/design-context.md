# UI continuity for the roadmap

The existing native egui interface is the design source. The authorized work extends
the current study, lesson and settings flows without replacing their visual system.

## Verified vocabulary

- `crates/ui/src/theme.rs`: warm paper light theme, charcoal dark theme, teal accent,
  amber highlight. Since v0.23.0 the layout is minimal and brutalist, like the covers of
  programming books: square corners, black rules (`ink`), hard offset shadows
  (`shadow`), IBM Plex Mono for titles and labels, Inter for running text.
  `Palette` owns application colors.
- `widgets.rs`: `card`, `header` (title over a heavy rule), `rule`, `kicker` (uppercase
  mono label), `cover` (book-cover block with the accent band and the engraving),
  `hint`, `callout`, `primary_button`, `empty_state`.
- `art.rs`: the black-and-white engraving of Thoth (`Seated` on the welcome cover,
  `Head` on Today and as a sidebar plate when there is room).
- `screens/settings.rs`: editable draft saved/discarded together; model progress,
  installed and missing states remain visible in the existing model list.
- `screens/lesson.rs`: stage, captions, whiteboard, transcript and bottom controls.
- `avatar.rs`: vector teacher with breathing, blinking and audio-driven mouth motion.
- Every added fixed label is a locale key in both `locales/pt-BR.yml` and `en.yml`.

## Flow decisions

Voice settings retain the saved draft flow. Local voices are selected by gender and
installation state; unavailable voices point to downloads. Preview provides immediate
busy/stop feedback and does not require a lesson. Cloud speech is explicitly optional,
with a notice next to its endpoint and credentials explaining the text transfer.

Board material reuse selects an existing document from another plan and reuses the
existing extraction progress/cancel feedback. Relevance belongs to the destination
plan. OCR configuration explains that scanned pages are sent to the configured vision
endpoint only when enabled.

Whiteboard formulas are bounded plain text, never executable markup. Diagrams are
small linear flows of two to four nodes; long labels wrap and a narrow board stacks
nodes vertically. All original text remains available in the board data.

Empty states use original native vector illustrations: an open study book, a marked
route, question cards and growing progress. They retain one title, explanation and
primary action. Illustration colors come from the existing palette in both themes.

The logo is Thoth's ibis head under the lunar disc and crescent, in a square: light
ibis on ink for light backgrounds, inverted for dark ones, amber disc in both. Teacher variations reuse the same animation; choice changes the outfit
and the chosen voice determines the female/male appearance.

## Validation

Run locale completeness checks, workspace checks and the existing GPU snapshot suite.
Inspect light/dark, populated/empty and lesson states before merging. Machine checks
do not establish voice quality or live microphone behavior.
