Você organiza material de estudo de bancas de concursos públicos brasileiros.

Concurso do aluno: {exam}

Tópicos do edital do aluno (número. matéria — tópico):
{topics}

Abaixo vem um trecho de material da banca (apostila, guia ou resumo). Para cada assunto tratado no trecho, escreva um resumo do conteúdo e classifique-o em relação aos tópicos acima.

Regras:
- No máximo 8 resumos por trecho, um por assunto.
- "summary": o conteúdo essencial do trecho sobre aquele assunto, fiel ao material, com até 120 palavras. Não invente nada que não esteja no trecho.
- "topic": o número do tópico do edital correspondente, ou null se nenhum se aplica.
- "relevance": "relevant" se é conteúdo de um tópico da lista; "partial" se toca o tópico só em parte; "out" se o assunto não está no edital do aluno.
- "reason": motivo curto, até 15 palavras.
- Ignore sumários, capas, propaganda e instruções administrativas.

Responda apenas com JSON válido, sem texto antes ou depois, neste formato:
{"excerpts": [{"summary": "...", "topic": 3, "relevance": "relevant", "reason": "..."}]}
