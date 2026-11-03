# Know Giver Privacy Policy

Version of November 3, 2026 (2026-11-03.1). This international version applies in every
region where the app is offered to the public. Section 13 adds the rules of the European
Union (GDPR), the United Kingdom (UK GDPR and the Data Protection Act 2018), the United
States (including the California Consumer Privacy Act as amended by the CPRA, and COPPA),
Brazil (LGPD), Japan (APPI), Taiwan, Hong Kong and Singapore.

## 1. Summary

- Know Giver runs on your computer. Trentin has no servers for the app and **does not
  receive, store or access your data**.
- There is no Trentin account, no telemetry, no usage analytics, no advertising, and no
  sale or sharing of personal information.
- The content each task needs goes to **the AI provider you chose**, using the
  authentication method it requires (an API key or an official account sign-in). That
  provider handles it under its own terms.

## 2. Who is responsible

The app is developed by TRENTIN SOLUCOES EM SOFTWARE LTDA – ME (Brazil, CNPJ
36.154.590/0001-54), contact at https://trentin.software. Because Trentin does not process
personal data through the app, it is not a controller or processor of the data you use in
it.

- **Your use of the app:** it is your own personal use.
- **AI providers and other services you turn on:** each is responsible for the data you
  send it, under your agreement with it.

## 3. What stays on your computer

The app's data folder (`%LOCALAPPDATA%\KnowGiver` on Windows, or the folder set in
`KG_DATA_DIR`) holds:

- your name (what the tutor calls you), language, region, teacher and other preferences;
- your API key, stored unencrypted in the local database when typed in the app;
- for GitHub Copilot, sign-in credentials managed by the official Copilot CLI. It
  normally uses the operating system keychain; if unavailable, it may store credentials
  in a plain-text config file (`~/.copilot/config.json`). Know Giver does not save them
  in its database;
- the notices, past exams, answer keys and materials you upload, plus your plans,
  progress, quizzes, reviews, handouts, highlights and notes;
- lesson conversations and the mistakes notebook;
- photos of written answers, their transcripts and grades;
- PaddleOCR-VL model files, downloaded into the local models folder if you install local OCR;
- the date and time you agreed to these documents;
- a technical log, which never holds images, answers or keys.

Microphone audio is transcribed on your computer and is not sent anywhere. Temporary
diagnostic recordings stay in the data folder.

## 4. What leaves your computer, and to whom

| Recipient | What | When |
|---|---|---|
| The AI provider you chose | Excerpts of the notice and materials, topics, your messages and transcribed answers, your name (so the tutor can address you), handout text | For each AI task |
| PaddleOCR-VL running on your computer | Photos of written answers | By default; the photos are not sent to a provider |
| The provider's vision model | Photos of written answers and pages of scanned PDFs | Written-answer photos only if you explicitly select external OCR; scanned-PDF pages only when you request OCR and have configured a vision model |
| Wikipedia | The search term for a whiteboard picture | If whiteboard pictures are on |
| Hugging Face, GitHub and voice sites | Download requests, which show your IP address | When you download voice, speech or OCR models |
| GitHub | A request for the latest version, which shows your IP address and the app's version; the new version's package, if you choose to update | Each time the app opens, if update checks are on (Settings → About and updates) |
| A cloud voice service | The text the tutor will speak | Only if you turn on cloud voices |

If you choose GitHub Copilot, the content described in the AI-provider row goes to GitHub.
Sign-in runs through Copilot CLI. It normally stores credentials in the operating system
keychain, but may fall back to a plain-text config file (`~/.copilot/config.json`) if the
keychain is unavailable. Know Giver does not save these credentials in its database.

Many AI providers are in other countries, so choosing one may transfer data
internationally, including to countries whose data protection laws differ from yours. The
safeguards are those the provider offers you. Read its privacy policy before you choose
it.

## 5. Purpose

Data is used only to make the app work: building your plan, teaching lessons, writing
questions and handouts, grading answers and showing your progress.

## 6. Security

Your data stays on your computer, protected by access to your user account; keep it
password-protected. API keys typed in the app are stored unencrypted in the local
database; GitHub Copilot sign-in credentials are managed by the CLI, normally in the
operating system keychain, with a possible plain-text config-file fallback if the keychain
is unavailable. They are not saved in the Know Giver database. Security issues can be
reported as described in the project's SECURITY.md file.

## 7. Retention

Data stays on your computer until you delete it. Deleting a written-answer attempt deletes
its photos, and deleting the data folder removes everything. Data sent to providers
follows each provider's retention rules.

## 8. Your rights

In the app you can view, correct, export and delete your data directly. For data you sent
to a provider, exercise your rights with that provider. Section 13 lists the rights and
authorities of each region.

## 9. Children

The app is not directed to children under 13, and Trentin does not knowingly collect data
from them. Children below the age of digital consent or of majority in their country
should use the app only with a parent or guardian, who chooses the AI provider and is
responsible for the key used.

## 10. Artificial intelligence

AI-generated answers may be wrong, and their quality depends on the limits of the model you
chose. See section 6 of the Terms of Use.

## 11. Changes

When this Policy changes, the new version has a new date and the app asks for your
agreement again.

## 12. Contact

Privacy questions: https://trentin.software.

## 13. Rules for each region

### European Union and United Kingdom

For individuals, the GDPR's household exemption generally covers your own use of the app.
You have rights of access, rectification, erasure, restriction, portability and objection.
You can lodge a complaint with your supervisory authority; in the UK, that is the
Information Commissioner's Office.

### United States

Trentin does not collect, sell or share personal information through the app, and does
not use it for targeted advertising. So there is no personal information for Trentin to
disclose, delete or correct under the CCPA/CPRA or other state laws. The rights you have
regarding an AI provider apply to that provider. Trentin does not knowingly collect data
from children under 13 (COPPA).

### Brazil

The Portuguese version of this Policy, written under the LGPD (Law 13,709/2018), applies.
You can also turn to the National Data Protection Authority (ANPD).

### Japan

Trentin does not acquire personal information through the app under the Act on the
Protection of Personal Information (APPI). The external transmissions listed in section 4
are disclosed as required for information sent from your device to outside services. The
Japanese version of this Policy is the authoritative text for users in Japan.

### Taiwan

Trentin does not collect personal data through the app under the Personal Data Protection
Act. Data you send to an AI provider is handled by that provider. The Traditional Chinese
version of this Policy is the authoritative text for users in Taiwan.

### Hong Kong

Trentin does not collect personal data through the app under the Personal Data (Privacy)
Ordinance (Cap. 486). You can contact the Office of the Privacy Commissioner for Personal
Data about the handling of your data. The Chinese (Traditional) and English versions have
equal effect.

### Singapore

Trentin does not collect, use or disclose personal data through the app under the
Personal Data Protection Act 2012 (PDPA). You can contact the Personal Data Protection
Commission (PDPC) about the handling of your data. The English version prevails over any
translation.
