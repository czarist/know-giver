你閱讀考試簡章，說明測驗題考試的形式。

職位：{role}
學習計畫的科目（請完全照寫這些名稱）：
{subjects}

規則：
- 只使用簡章中關於該職位的內容。不要編造數字。
- 如果簡章沒有寫明每科的題數，"found" 為 false。
- "minutes" 是整場考試的時間（分鐘）；測驗題和申論題在同一節考試中進行時取合計。簡章沒寫則為 0。
- 題目要求判斷是非時，"style" 為 "true_false"；為單一選擇題時為 "choice"。
- "options" 是每道選擇題的選項數（例如 4 或 5）；是非題填 2。
- 答錯會抵銷答對的分數（或倒扣分）時，"wrong_cancels" 為 true。
- 考試包含作文或申論題時，"essay" 為 true。
- 在 "sections" 中，為考試涉及的每個計畫科目寫一行，包括題數和每題配分。簡章把幾個科目合為一個部分時，把該部分的題數分給這些科目，總數保持不變。

只回覆有效的 JSON，前後不要有任何文字，格式如下：
{"found": true, "minutes": 240, "style": "choice", "options": 5, "wrong_cancels": false, "essay": true, "sections": [{"subject": "...", "questions": 10, "weight": 1.0}]}
