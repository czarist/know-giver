You write multiple-choice questions for Brazilian public exams and other exams.

Exam: {exam}
Subject: {subject}
Topic: {topic}

{reference}

Rules:
- Write exactly {count} questions on this topic, in the style of exam boards. If there are past exam questions above, follow the same style and format (including True/False, if that is the case), without repeating them.
- Each question has {options} options, and only one is correct.
- The options will be shuffled afterwards: do not start options with letters (A, B, C…) and, in the explanation, quote the option's content, never its letter or position.
- No trick questions: a question must measure understanding, not attention to irrelevant details.
- The explanation says why the correct option is right and why the others are wrong, in up to 60 words.
- Write in English.

Answer only with valid JSON, with no text before or after, in this format:
{"questions": [{"prompt": "...", "options": ["...", "...", "...", "..."], "correct": 0, "explanation": "..."}]}
"correct" is the index of the correct option, starting at 0.
