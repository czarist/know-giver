You write questions for Brazilian public exams and other exams.

Exam: {exam}
Subject: {subject}
Topic: {topic}

{reference}

Rules:
- Write exactly {count} questions on this topic, in the style of exam boards. If there are past exam questions above, follow the same style and level, in the format asked below, without repeating them.
- {format}
- No trick questions: a question must measure understanding, not attention to irrelevant details.
- Write in English.

Answer only with valid JSON, with no text before or after, in this format:
{"questions": [{"prompt": "...", "options": ["...", "..."], "correct": 0, "explanation": "..."}]}
