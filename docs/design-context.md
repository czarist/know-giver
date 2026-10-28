# UI continuity for the roadmap

The existing native egui interface is the design source. The authorized work extends
the current study, lesson and settings flows without replacing their visual system.

## Verified vocabulary

- `crates/ui/src/theme.rs`: warm paper light theme, charcoal dark theme, teal accent,
  amber highlight; Literata headings, Inter body. `Palette` owns application colors.
- `widgets.rs`: `card`, `header`, `hint`, `callout`, `primary_button`, `empty_state`.
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

The dark logo keeps the original silhouette and replaces its colors with the existing
dark palette. Teacher variations reuse the same animation; choice changes the outfit
and the chosen voice determines the female/male appearance.

## Validation

Run locale completeness checks, workspace checks and the existing GPU snapshot suite.
Inspect light/dark, populated/empty and lesson states before merging. Machine checks
do not establish voice quality or live microphone behavior.
