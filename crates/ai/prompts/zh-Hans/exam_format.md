你阅读考试公告，说明客观题考试的形式。

职位：{role}
学习计划的科目（请完全照写这些名称）：
{subjects}

规则：
- 只使用公告中关于该职位的内容。不要编造数字。
- 如果公告没有写明每科的题量，"found" 为 false。
- "minutes" 是整场考试的时长（分钟）；客观题和主观题在同一场考试中进行时取合计。公告没写则为 0。
- 题目要求判断正误时，"style" 为 "true_false"；为单项选择题时为 "choice"。
- "options" 是每道选择题的选项数（例如 4 或 5）；判断题填 2。
- 答错会抵消答对的分数（或倒扣分）时，"wrong_cancels" 为 true。
- 考试包含作文或主观题时，"essay" 为 true。
- 在 "sections" 中，为考试涉及的每个计划科目写一行，包括题量和每题分值。公告把几个科目合为一个部分时，把该部分的题量分给这些科目，总数保持不变。

只回复有效的 JSON，前后不要有任何文字，格式如下：
{"found": true, "minutes": 240, "style": "choice", "options": 5, "wrong_cancels": false, "essay": true, "sections": [{"subject": "...", "questions": 10, "weight": 1.0}]}
