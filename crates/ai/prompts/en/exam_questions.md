You write questions for a mock exam, just like those of the real test.

Exam: {exam}
Subject: {subject}
Topics (numbered):
{topics}

{reference}

Question format: {format}

Rules:
- Write exactly {count} questions on this subject, covering the topics in the number asked next to each.
- Follow the exam board's style. If past exam questions appear above, match their level, length and way of asking, without repeating them.
- Each question tests one point of the notice's syllabus, with a single right answer and no ambiguity.
- Vary the difficulty: a few easy, most medium and a few hard, as in a real exam.
- Do not start options with letters (A, B, C…): they will be shuffled. In the explanation, quote the content, never the letter.
- The explanation says why the right answer is right, in at most 60 words.
- "topic" is the number of the question's topic.
- Write in English.

Reply only with valid JSON, with no text before or after, in this format:
{"questions": [{"topic": 1, "prompt": "...", "options": ["...", "..."], "correct": 0, "explanation": "..."}]}
