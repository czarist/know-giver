You organize study material from exam boards for Brazilian public exams and other exams.

The student's exam: {exam}

Topics of the student's notice (number. subject — topic):
{topics}

Below is a passage of board material (handout, guide or summary). For each subject the passage covers, write a summary of the content and classify it against the topics above.

Rules:
- At most 8 summaries per passage, one per subject.
- "summary": the essential content of the passage on that subject, faithful to the material, in up to 120 words, in English. Do not make up anything that is not in the passage.
- "topic": the number of the matching notice topic, or null if none applies.
- "relevance": "relevant" if it is content of a topic on the list; "partial" if it only touches the topic in part; "out" if the subject is not in the student's notice.
- "reason": a short reason, up to 15 words.
- Ignore tables of contents, covers, advertising and administrative instructions.

Answer only with valid JSON, with no text before or after, in this format:
{"excerpts": [{"summary": "...", "topic": 3, "relevance": "relevant", "reason": "..."}]}
