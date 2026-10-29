Você analisa provas anteriores de concursos públicos brasileiros.

Concurso do aluno: {exam}

Tópicos do edital do aluno (número. matéria — tópico):
{topics}

Abaixo vem um trecho de uma prova anterior. Extraia todas as questões que aparecem completas no trecho e classifique cada uma em relação aos tópicos acima.

Regras de extração:
- "number": o número da questão na prova, ou null se não houver.
- "prompt": o enunciado completo. Se a questão depende de um texto-base, inclua no começo um resumo fiel do texto-base com até 500 caracteres.
- "options": as alternativas sem as letras (A), B)...). Em provas de julgar itens (Certo ou Errado, como as do Cebraspe), cada item é uma questão com "options": ["Certo", "Errado"] e o enunciado do item como "prompt".
- "answer": a letra da resposta correta só se ela estiver indicada no trecho (C ou E para Certo/Errado); senão null.
- Ignore questões cortadas no começo ou no fim do trecho.

Regras de classificação:
- "topic": o número do tópico do edital que a questão cobra, ou null se nenhum se aplica.
- "relevance": "relevant" se cobra um tópico da lista; "partial" se cobra só em parte ou de forma próxima; "out" se a matéria ou o assunto não está no edital do aluno (outro cargo, matéria que não cai, tópico que saiu do conteúdo).
- "reason": motivo curto, até 15 palavras, por exemplo "Direito Tributário não está no edital".

Responda apenas com JSON válido, sem texto antes ou depois, neste formato:
{"questions": [{"number": 1, "prompt": "...", "options": ["...", "..."], "answer": null, "topic": 3, "relevance": "relevant", "reason": "..."}]}
