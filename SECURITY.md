# Security Policy

Know Giver reads files you give it (exam notices, past exams, handwritten pages), downloads
speech models and runtimes, starts local helper programs and sends text to the AI service
you configure. Treat every one of those inputs as potentially hostile when you look for or
report a problem: a PDF, an image, an answer from the AI model, a downloaded archive or a
saved settings file.

## Supported versions

Only the latest release on `main` gets security fixes. Check that the problem still
happens there before reporting.

## Reporting a vulnerability

Give the maintainer a reasonable chance to investigate and fix the problem before you
publish exploit details, proof-of-concept code or crafted files.

- Use GitHub's private vulnerability reporting (**Security → Report a vulnerability** on
  the repository) when it is available.
- Otherwise, get in touch through [cezartrentin.dev](https://cezartrentin.dev) with a
  short, non-exploitable summary and ask for a private way to send the details.
- Do not open a public issue containing a working exploit, an API key, a crash dump with
  personal data, or a malicious attachment.
- If no private channel can be arranged, keep the weaponized material to yourself and
  share only the smallest safe description that lets the maintainer reach you.

## What a useful report includes

- The affected version, commit or release package, and the OS and architecture.
- How it was built or installed: portable zip, installer, `cargo run`, and features such
  as `cuda`.
- The affected part (for example PDF import, photo upload, model download, settings) and
  the security impact: what an attacker gains and what access they need.
- Exact steps to reproduce, what you expected and what happened, and the relevant lines
  of `know-giver.log`.
- Whether it happens from a clean checkout, and any workaround you know.
- Remove API keys, personal paths, exam documents, photos of your writing and any
  unrelated system information first.

## Malicious test files

Do not attach possibly harmful or non-redistributable files (PDFs, images, model
archives) to public issues. Describe the format, size, SHA-256, what it causes and where it
came from, and arrange a private transfer.

Test files accepted into the repository must be minimized, free of personal or
proprietary data (no real exam documents or handwriting), legally redistributable, and
documented with the failure they are expected to cause.

## Scope

In scope:

- **Document parsing:** exam notices, past exams, answer keys and board material (PDF text
  extraction, scanned pages), including oversized or malformed files and resource
  exhaustion.
- **Images:** photos of handwritten answers (JPEG/PNG decoding, size and pixel limits,
  rotation, storage under the data folder).
- **Prompt injection:** text in a notice, a past exam, a handout or a photographed page
  that makes the AI model act against the student, leak data or bypass the checks the app
  applies to its answers (scores, quotes, pages and lines).
- **AI answers:** model output that crashes the app, writes outside the data folder,
  renders unsafe content in handouts or the reader, or reaches a helper program as a
  command.
- **Downloads:** speech models, voices and runtimes (Whisper, Parakeet, Piper, Kokoro,
  sherpa-onnx) and Wikipedia pictures: tampered archives, path traversal on unpacking,
  redirects to other hosts.
- **Helper programs:** Piper, the Kokoro runtime and the local Parakeet speech server
  started as child processes, and the Kokoro library loaded in process (the only `unsafe`
  code, in `kg_audio::kokoro_ffi`).
- **Secrets and local data:** the API key, the SQLite database, logs, recordings and
  photos in `%LOCALAPPDATA%\KnowGiver` (or `KG_DATA_DIR`).
- **Supply chain:** dependencies, CI workflows, packaging scripts, the installer and
  release artifacts.

Bugs without a security impact (a wrong grade, a misread word, a crash on a valid file)
go through the normal issue tracker.

## Current limitations

These are known and documented; reports that show a worse impact than described here are
welcome.

- **No checksum on downloads yet.** Models and runtimes come from fixed URLs over HTTPS and
  are checked for completeness, not against a published hash. A compromised upstream
  host would not be detected. Pinned SHA-256 checks are planned (ROADMAP, Phase 8.5).
- **The API key is stored in plain text** in the local SQLite database when typed in
  Settings (a key that only comes from `.env` is not copied there). Anyone who can read
  your user profile can read it.
- **The Parakeet speech server** (sherpa-onnx, started by the app) runs on a random port
  while lessons need it and has no authentication. The address it listens on is the
  upstream server's default, which may include network interfaces besides loopback; on an
  untrusted network, keep the system firewall prompt denied for it.
- **Content sent to the AI service:** text from your notice, material and answers goes to
  the endpoint you configure, and photos of handwritten pages go to the vision model when
  you set one. Choose an endpoint you trust; the app cannot protect data once it reaches
  that service.
- **Managed local AI** (LiteLLM, llama.cpp, gpu-unlock) is planned, not shipped; it is not
  in scope until it exists.
