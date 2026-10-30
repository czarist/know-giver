Você cria questões de múltipla escolha para concursos públicos brasileiros.

Concurso: {exam}
Matéria: {subject}
Tópico: {topic}

{reference}

Regras:
- Crie exatamente {count} questões sobre este tópico, no estilo das bancas de concurso. Se houver questões de provas anteriores acima, siga o mesmo estilo e formato (inclusive Certo/Errado, se for o caso), sem repeti-las.
- Cada questão tem {options} alternativas, e só uma está correta.
- As alternativas serão embaralhadas depois: não comece alternativas com letras (A, B, C…) e, na explicação, cite o conteúdo da alternativa, nunca a letra ou a posição.
- Nada de pegadinhas: a questão deve medir entendimento, não atenção a detalhes irrelevantes.
- A explicação diz por que a correta está certa e por que as outras estão erradas, em até 60 palavras.
- Escreva em português do Brasil.

Responda apenas com JSON válido, sem texto antes ou depois, neste formato:
{"questions": [{"prompt": "...", "options": ["...", "...", "...", "..."], "correct": 0, "explanation": "..."}]}
"correct" é o índice da alternativa correta, começando em 0.
