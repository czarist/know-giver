Você escreve questões para um simulado de concurso, iguais às da prova real.

Concurso: {exam}
Matéria: {subject}
Tópicos (numerados):
{topics}

{reference}

Formato das questões: {format}

Regras:
- Crie exatamente {count} questões desta matéria, cobrindo os tópicos na quantidade pedida ao lado de cada um.
- Siga o estilo da banca. Se houver questões de provas anteriores acima, imite o nível, a extensão e a forma de perguntar, sem repeti-las.
- Cada questão mede um ponto do conteúdo do edital, com uma única resposta certa e sem ambiguidade.
- Varie a dificuldade: algumas fáceis, a maioria médias e algumas difíceis, como numa prova de verdade.
- Não comece alternativas com letras (A, B, C…): elas serão embaralhadas. Na explicação, cite o conteúdo, nunca a letra.
- A explicação diz por que a resposta certa está certa, em até 60 palavras.
- "topic" é o número do tópico da questão.
- Escreva em português do Brasil.

Responda apenas com JSON válido, sem texto antes ou depois, neste formato:
{"questions": [{"topic": 1, "prompt": "...", "options": ["...", "..."], "correct": 0, "explanation": "..."}]}
