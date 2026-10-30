//! How people study for exams where the student lives: the usual exams and
//! boards, question formats, written-test conventions, grading scales and
//! the teacher's manner. Teaching tasks (plan, lessons, quizzes, handouts,
//! reviews, written answers) get this note after their instructions;
//! extraction tasks (reading past exams and answer keys) do not, so they
//! stay faithful to the document.
//!
//! The notes follow [`PromptLanguage::pick`]: Portuguese, or English for the
//! other languages. The exam notice and past exams always win over them.

use crate::config::PromptLanguage;

/// The student's exam culture, from the region chosen in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExamCulture {
    Brazil,
    UnitedStates,
    EuropeanUnion,
    UnitedKingdom,
    Japan,
    Taiwan,
    HongKong,
    Singapore,
    MainlandChina,
}

impl ExamCulture {
    pub const ALL: [Self; 9] = [
        Self::Brazil,
        Self::UnitedStates,
        Self::EuropeanUnion,
        Self::UnitedKingdom,
        Self::Japan,
        Self::Taiwan,
        Self::HongKong,
        Self::Singapore,
        Self::MainlandChina,
    ];

    /// Options per multiple-choice quiz question in the usual local exams.
    pub fn quiz_options(self) -> u32 {
        match self {
            // Cebraspe aside (true/false items), FGV, FCC, Vunesp and the ENEM
            // use five options (A to E).
            Self::Brazil => 5,
            // Japanese civil service and qualification exams: five options.
            Self::Japan => 5,
            _ => 4,
        }
    }

    /// The note for the model, in the prompts' language.
    pub fn note(self, language: PromptLanguage) -> &'static str {
        let (pt, en) = match self {
            Self::Brazil => (BRAZIL_PT, BRAZIL_EN),
            Self::UnitedStates => (US_PT, US_EN),
            Self::EuropeanUnion => (EU_PT, EU_EN),
            Self::UnitedKingdom => (UK_PT, UK_EN),
            Self::Japan => (JAPAN_PT, JAPAN_EN),
            Self::Taiwan => (TAIWAN_PT, TAIWAN_EN),
            Self::HongKong => (HONG_KONG_PT, HONG_KONG_EN),
            Self::Singapore => (SINGAPORE_PT, SINGAPORE_EN),
            Self::MainlandChina => (CHINA_PT, CHINA_EN),
        };
        language.pick(pt, en)
    }
}

const BRAZIL_PT: &str = "\
Cultura de estudo do aluno (Brasil). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: concursos públicos, vestibulares, ENEM e exames da OAB. Bancas comuns: Cebraspe (itens certo ou errado; um item errado anula um certo, então deixar em branco é estratégia), FGV, FCC e Vunesp (múltipla escolha de A a E). Se o edital ou a prova indicar a banca, imite o estilo e as pegadinhas dela.
- Use leis, órgãos, jurisprudência e exemplos brasileiros (súmulas, artigos da Constituição de 1988).
- Discursiva: texto dissertativo-argumentativo contado em linhas (geralmente 20 a 30), norma-padrão, introdução, desenvolvimento e conclusão; nota em pontos pelos critérios do edital. No ENEM, as cinco competências, de 0 a 1000, com proposta de intervenção.
- Tom: professor de cursinho, direto e próximo, tratando o aluno por você; macetes, mnemônicos e as pegadinhas que mais caem.";

const BRAZIL_EN: &str = "\
The student's exam culture (Brazil). Follow it unless the exam notice or past exams say otherwise:
- Exams: public-service exams (concursos), university entrance exams, the ENEM and the bar exam (OAB). Common boards: Cebraspe (true/false items where a wrong answer cancels a right one, so leaving blanks is strategy), FGV, FCC and Vunesp (multiple choice A to E). When the notice or exam names the board, copy its style and traps.
- Use Brazilian laws, bodies, case law and examples (the 1988 Constitution).
- Written test: an argumentative essay counted in lines (usually 20 to 30), standard Portuguese, introduction, body and conclusion; scored in points by the notice's criteria. For the ENEM, its five competencies, 0 to 1000, with an intervention proposal.
- Manner: a prep-course teacher, direct and close; mnemonics and the traps that come up most.";

const US_PT: &str = "\
Cultura de estudo do aluno (Estados Unidos). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: exames padronizados (SAT, ACT, GRE, GMAT, LSAT), o bar exam (MBE de múltipla escolha e MEE dissertativo), certificações profissionais e exames do serviço público federal e estadual.
- Múltipla escolha com uma resposta certa e distratores plausíveis; questões em contexto (passagens, casos, dados). No bar exam, fatos de um caso e a regra aplicável.
- Use leis e instituições americanas (Constituição federal, leis estaduais, Suprema Corte) e unidades americanas quando couber.
- Escrita: argumentative essay com tese clara, parágrafos com tópico frasal, evidências e conclusão; nas questões jurídicas, o método IRAC (issue, rule, application, conclusion). Notas por rubrica (por exemplo, de 1 a 6) ou pontos.
- Tom: coach encorajador e objetivo, com estratégias de prova (eliminar alternativas, gerenciar o tempo).";

const US_EN: &str = "\
The student's exam culture (United States). Follow it unless the exam notice or past exams say otherwise:
- Exams: standardized tests (SAT, ACT, GRE, GMAT, LSAT), the bar exam (multiple-choice MBE and essay MEE), professional certifications and federal and state civil service exams.
- Multiple choice with one best answer and plausible distractors; questions in context (passages, cases, data). For the bar, a fact pattern and the governing rule.
- Use American laws and institutions (the federal Constitution, state law, the Supreme Court) and US units where they apply.
- Writing: an argumentative essay with a clear thesis, topic sentences, evidence and a conclusion; for legal questions, IRAC (issue, rule, application, conclusion). Scored on a rubric (for example 1 to 6) or in points.
- Manner: an encouraging, efficient coach, with test-taking strategy (eliminating options, managing time).";

const EU_PT: &str = "\
Cultura de estudo do aluno (União Europeia). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: concursos públicos nacionais e os concursos das instituições europeias (EPSO: raciocínio verbal, numérico e abstrato, testes de julgamento situacional, estudo de caso escrito), exames de acesso à universidade e certificações.
- Use o direito e as instituições do país do edital e, quando couber, o direito da União (tratados, regulamentos, diretivas, Tribunal de Justiça).
- Escrita: texto estruturado e argumentado, no padrão do país (por exemplo, a dissertação francesa em tese, antítese e síntese); notas na escala do país (0 a 20, 1 a 10, 1 a 6).
- Tom: professor claro e rigoroso, que mostra o raciocínio passo a passo.";

const EU_EN: &str = "\
The student's exam culture (European Union). Follow it unless the exam notice or past exams say otherwise:
- Exams: national civil service competitions and the EU institutions' competitions (EPSO: verbal, numerical and abstract reasoning, situational judgement, a written case study), university entrance exams and certifications.
- Use the law and institutions of the notice's country and, where relevant, EU law (treaties, regulations, directives, the Court of Justice).
- Writing: a structured, argued text in the country's style (for example the French dissertation: thesis, antithesis, synthesis); grades on the country's scale (0 to 20, 1 to 10, 1 to 6).
- Manner: a clear, rigorous teacher who shows the reasoning step by step.";

const UK_PT: &str = "\
Cultura de estudo do aluno (Reino Unido). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: GCSE e A-levels (por board: AQA, Edexcel, OCR), testes do Civil Service (Civil Service Judgement Test, testes numéricos e verbais), exames profissionais (SQE, ACCA, ICAEW).
- Perguntas com command words (describe, explain, evaluate, discuss) e notas por mark scheme: cada ponto vale marcas; em perguntas longas, níveis de resposta.
- Use leis e instituições britânicas (Acts of Parliament, Supreme Court, Civil Service Code) e ortografia britânica em inglês.
- Escrita: ensaio com argumento claro, PEEL (point, evidence, explanation, link) e avaliação equilibrada antes da conclusão.
- Tom: tutor cordial e preciso, que mostra onde estão as marcas.";

const UK_EN: &str = "\
The student's exam culture (United Kingdom). Follow it unless the exam notice or past exams say otherwise:
- Exams: GCSEs and A-levels (by board: AQA, Edexcel, OCR), Civil Service tests (Civil Service Judgement Test, numerical and verbal tests), professional exams (SQE, ACCA, ICAEW).
- Questions use command words (describe, explain, evaluate, discuss) and are marked against a mark scheme: each point earns marks; long answers are marked in levels.
- Use British laws and institutions (Acts of Parliament, the Supreme Court, the Civil Service Code) and British spelling.
- Writing: an essay with a clear line of argument, PEEL paragraphs (point, evidence, explanation, link) and balanced evaluation before the conclusion.
- Manner: a warm, precise tutor who shows where the marks are.";

const JAPAN_PT: &str = "\
Cultura de estudo do aluno (Japão). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: concursos públicos (国家公務員 総合職・一般職, 地方公務員), exames de qualificação (行政書士, 司法書士, 宅建, 簿記), vestibulares (共通テスト) e certificações.
- Múltipla escolha com cinco alternativas (五肢択一), muitas vezes do tipo “escolha a correta” ou “escolha a combinação correta (組合せ)”; o 教養試験 cobre 数的処理, 文章理解 e conhecimentos gerais; o 専門試験, direito e economia.
- Use leis e instituições japonesas (日本国憲法, 民法, 行政法) e exemplos do Japão.
- Escrita: 小論文 ou 論文 com limite de caracteres (por exemplo, 800 a 1200 字), no 原稿用紙, em estilo である, com 序論・本論・結論; nota por critérios ou pontos (por exemplo, 100点).
- Tom: professor educado, em です・ます, metódico, com repetição espaçada, 過去問 e pontos que mais caem (頻出).";

const JAPAN_EN: &str = "\
The student's exam culture (Japan). Follow it unless the exam notice or past exams say otherwise:
- Exams: civil service exams (national comprehensive and general track, local government), qualification exams (行政書士, 司法書士, 宅建, 簿記), university entrance (the Common Test) and certifications.
- Multiple choice with five options (五肢択一), often “choose the correct one” or “choose the correct combination (組合せ)”; the general test (教養試験) covers numerical reasoning (数的処理), reading (文章理解) and general knowledge; the specialist test (専門試験) law and economics.
- Use Japanese laws and institutions (the Constitution of Japan, the Civil Code, administrative law) and Japanese examples.
- Writing: a 小論文 or 論文 with a character limit (for example 800 to 1,200 characters) on 原稿用紙 squared paper, in the plain である style, with introduction, body and conclusion (序論・本論・結論); graded by criteria or points (for example out of 100).
- Manner: a polite teacher speaking in です・ます, methodical, built on past papers (過去問) and the points that come up most (頻出).";

const TAIWAN_PT: &str = "\
Cultura de estudo do aluno (Taiwan). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: concursos públicos (高普考, 地方特考, 司法特考), exames profissionais e vestibulares (學測, 分科測驗).
- Questões de múltipla escolha (測驗題) com quatro alternativas, e questões dissertativas (申論題) que pedem explicar e aplicar a lei ou a teoria.
- Use leis e instituições de Taiwan (憲法, 行政程序法, 民法) e exemplos locais.
- Escrita: 申論題 com estrutura clara (conceito, base legal, análise, conclusão), em tópicos numerados; a redação de língua (作文) com introdução, desenvolvimento e conclusão; nota em pontos.
- Tom: professor de cursinho (補習班) caprichoso, com resumos de pontos importantes (重點整理) e as perguntas que mais caem (考古題).";

const TAIWAN_EN: &str = "\
The student's exam culture (Taiwan). Follow it unless the exam notice or past exams say otherwise:
- Exams: civil service exams (高普考, local and judicial special exams), professional licensing exams and university entrance (學測, 分科測驗).
- Multiple-choice items (測驗題) with four options, and essay questions (申論題) that ask to explain and apply the law or theory.
- Use Taiwan's laws and institutions (the Constitution, the Administrative Procedure Act, the Civil Code) and local examples.
- Writing: 申論題 answers with a clear structure (concept, legal basis, analysis, conclusion) in numbered points; composition (作文) with introduction, body and conclusion; scored in points.
- Manner: a thorough cram-school (補習班) teacher, with key-point summaries (重點整理) and past questions (考古題).";

const HONG_KONG_PT: &str = "\
Cultura de estudo do aluno (Hong Kong). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: o HKDSE, os exames do serviço público (CRE: uso do inglês e do chinês, aptidão; JRE; Basic Law and National Security Law Test) e exames profissionais.
- Múltipla escolha com quatro alternativas; perguntas estruturadas e redações nos padrões do HKDSE.
- Use as leis e instituições de Hong Kong (Basic Law, ordenanças, o Civil Service Bureau) e exemplos locais; o aluno pode estudar em chinês ou em inglês.
- Escrita: texto argumentativo com posição clara e parágrafos bem estruturados; notas por níveis (HKDSE: 1 a 5**) ou pontos.
- Tom: tutor eficiente e exigente, com prática de provas anteriores (past papers) e técnica de prova.";

const HONG_KONG_EN: &str = "\
The student's exam culture (Hong Kong). Follow it unless the exam notice or past exams say otherwise:
- Exams: the HKDSE, civil service exams (CRE: Use of English, Use of Chinese, aptitude; JRE; the Basic Law and National Security Law Test) and professional exams.
- Multiple choice with four options; structured questions and essays in HKDSE style.
- Use Hong Kong's laws and institutions (the Basic Law, ordinances, the Civil Service Bureau) and local examples; the student may study in Chinese or English.
- Writing: an argumentative text with a clear stance and well-built paragraphs; graded in levels (HKDSE 1 to 5**) or points.
- Manner: an efficient, demanding tutor, built on past papers and exam technique.";

const SINGAPORE_PT: &str = "\
Cultura de estudo do aluno (Singapura). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: PSLE, GCE O-Level e A-Level, exames de acesso ao serviço público e certificações profissionais.
- Múltipla escolha com quatro alternativas e perguntas estruturadas com marcas por etapa; em matemática, mostrar o raciocínio (método de modelo de barras no primário).
- Use as leis e instituições de Singapura (Constituição, Public Service Commission) e exemplos locais; inglês como língua principal, chinês como língua materna.
- Escrita: redação argumentativa ou discursiva, com tese e parágrafos equilibrados; notas por bandas ou pontos.
- Tom: professor organizado e exigente, com prática frequente e revisão de erros.";

const SINGAPORE_EN: &str = "\
The student's exam culture (Singapore). Follow it unless the exam notice or past exams say otherwise:
- Exams: PSLE, GCE O-Level and A-Level, public service entry assessments and professional certifications.
- Multiple choice with four options and structured questions with method marks; in mathematics, show the working (the bar model method in primary).
- Use Singapore's laws and institutions (the Constitution, the Public Service Commission) and local examples; English is the main language, Chinese the mother tongue.
- Writing: an argumentative or discursive essay with a thesis and balanced paragraphs; graded in bands or points.
- Manner: an organized, demanding teacher, with frequent practice and mistake review.";

const CHINA_PT: &str = "\
Cultura de estudo do aluno (China continental). Siga-a quando o edital e as provas não disserem outra coisa:
- Provas: concursos públicos nacionais e provinciais (国考, 省考), o 高考, o 考研 e exames de qualificação profissional.
- Nos concursos, o 行测 (行政职业能力测验: 言语理解, 数量关系, 判断推理, 资料分析, 常识) em múltipla escolha com quatro alternativas, sob forte pressão de tempo; e o 申论, com resumo, análise, propostas e redação.
- Use as leis e instituições da China continental e exemplos locais.
- Escrita: o 申论 pede ler o material dado, extrair os pontos-chave e escrever uma redação argumentativa com título, tese e propostas concretas; o 高考 tem redação de cerca de 800 caracteres; nota em pontos.
- Tom: professor de curso preparatório, focado em técnicas (技巧), tempo por questão e pontos-chave (考点).";

const CHINA_EN: &str = "\
The student's exam culture (mainland China). Follow it unless the exam notice or past exams say otherwise:
- Exams: national and provincial civil service exams (国考, 省考), the gaokao, the graduate entrance exam (考研) and professional qualification exams.
- For civil service, the 行测 aptitude test (verbal comprehension, quantitative relations, logical judgement, data analysis, general knowledge) in four-option multiple choice under tight time, and the 申论 essay test (summary, analysis, proposals and an essay).
- Use mainland China's laws and institutions and local examples.
- Writing: the 申论 asks to read the given materials, extract the key points and write an argumentative essay with a title, a thesis and concrete proposals; the gaokao essay runs about 800 characters; scored in points.
- Manner: a prep-course teacher focused on techniques (技巧), time per question and key points (考点).";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_culture_has_both_notes_and_a_quiz_format() {
        for culture in ExamCulture::ALL {
            let pt = culture.note(PromptLanguage::Portuguese);
            let en = culture.note(PromptLanguage::English);
            assert!(pt.starts_with("Cultura de estudo do aluno"), "{culture:?}");
            assert!(en.starts_with("The student's exam culture"), "{culture:?}");
            // Japanese and Chinese prompts get the English note.
            assert_eq!(culture.note(PromptLanguage::Japanese), en);
            assert!((2..=6).contains(&culture.quiz_options()));
        }
        assert_eq!(ExamCulture::Brazil.quiz_options(), 5);
        assert_eq!(ExamCulture::UnitedStates.quiz_options(), 4);
    }
}
