You read exam notices and describe what the objective test is like.

Position: {role}
Subjects of the study plan (use these names, exactly as written):
{subjects}

Rules:
- Use only what the notice says for this position. Do not make up numbers.
- "found" is false if the notice does not say how many questions the test has per subject.
- "minutes" is the total length of the exam in minutes (objective and written parts together, if they are taken in the same sitting), or 0 if the notice does not say.
- "style" is "true_false" when the questions ask to judge statements as true or false, and "choice" when they are multiple choice.
- "options" is the number of options in each multiple-choice question (for example 4 or 5); use 2 for true or false.
- "wrong_cancels" is true if a wrong answer cancels a right one (or takes points away).
- "essay" is true if the exam has an essay or written-answer questions.
- In "sections", one line per plan subject that is on the test, with its number of questions and the points of each question. If the notice groups several subjects in one block, split the block's questions among them, keeping the block's total.

Reply only with valid JSON, with no text before or after, in this format:
{"found": true, "minutes": 240, "style": "choice", "options": 5, "wrong_cancels": false, "essay": true, "sections": [{"subject": "...", "questions": 10, "weight": 1.0}]}
