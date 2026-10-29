# Written answers — manual check

Automated tests cover storage, rubrics and totals, invalid model answers, uncertain
transcripts, invented quotes, a missing vision model, page order, isolation between
attempts and safe retries, all with simulated model answers. What they cannot check is
how a real vision model reads real handwriting. Run this list before trusting a grade.

## Set up the vision model

1. Settings → AI → **Vision model**: a model that takes images at your AI address. With
   an OpenAI-compatible endpoint this is a multimodal model (for example `glm-4.5v` on
   Z.ai, `gpt-4o-mini` on OpenAI, `llava` or `qwen2.5vl` on Ollama). `.env` can set it
   with `MODEL_VISION=...`.
2. Save, open **Written answers** and press **Test image reading**. It sends a drawn
   three-digit number; "The model read the test image" means the model and the endpoint
   really take images. A model that answers without reading it shows a warning, and an
   endpoint that refuses images shows the HTTP status.
3. Without a vision model nothing is sent: the send button stays off and no text-only
   model is used instead.

Photos go to that endpoint; they stay in `%LOCALAPPDATA%\KnowGiver\discursive\` and are
deleted with their attempt.

## Pages to prepare

Write the same short answer (15–25 lines) on paper, on a proposal from a plan whose notice
has a written test, and photograph:

| Case | What to look for |
|---|---|
| A legible page, good light | No photo warnings; the transcript keeps your spelling mistakes and punctuation; lines match the sheet. |
| A page with crossed-out words | Crossed-out text comes as `[rasura]` or `[rasura: …]` and is not graded. |
| An answer on two or three pages | Pages can be reordered before sending; the transcript keeps page numbers; quotes in the grade name the right page and line. |
| A dark photo, a blurred one, one cut at the edge | Dark/blurry chips before sending; a cut page comes back as "take new photos" with no grade. |
| Very small or messy handwriting | Uncertain words come as `[?…?]`; with too many, the grade stays pending instead of guessing. |
| A rotated photo | ⟳ fixes it; the original file is unchanged. |

## While checking the transcript

- Fix one misread word and confirm: the grade view says how many reading corrections you
  made; the original reading is still stored (`discursive_transcripts.original`).
- Rewrite a whole sentence: the app refuses to confirm and offers **Save as new version**,
  which creates attempt N+1 and leaves the original untouched.

## The grade

- The total equals the sum of the criteria; each criterion is within its points.
- Every quote is in your text, with page and line; a note says when quotes from the model
  were dropped.
- Nothing comments on handwriting beyond legibility.
- The answer outline appears only after the grade, labelled as not your answer.
- Compare one essay with a human correction and write down the gap per criterion.

## Recovery

- Close the app while "Reading the pages…" shows: on the next start the attempt says the
  app was closed during processing and **Try again** resumes it.
- Double-click **Confirm and grade**: only one grade is made.
