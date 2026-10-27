//! Small signal and text helpers.

/// Resamples mono audio with linear interpolation. When downsampling, each
/// output sample averages the input window it covers, which is enough
/// anti-aliasing for speech recognition.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() || from == 0 || to == 0 {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((samples.len() as f64) / ratio).floor() as usize;
    let last = samples.len() - 1;
    (0..out_len)
        .map(|i| {
            let start = i as f64 * ratio;
            if ratio > 1.0 {
                let a = start.floor() as usize;
                let b = ((start + ratio).ceil() as usize)
                    .min(samples.len())
                    .max(a + 1);
                let window = &samples[a.min(last)..b];
                window.iter().sum::<f32>() / window.len() as f32
            } else {
                let a = start.floor() as usize;
                let frac = (start - a as f64) as f32;
                let s0 = samples[a.min(last)];
                let s1 = samples[(a + 1).min(last)];
                s0 + (s1 - s0) * frac
            }
        })
        .collect()
}

/// Root mean square of a block of samples.
pub(crate) fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Longest chunk sent to the TTS at once. Short chunks start playing sooner.
const MAX_CHUNK_CHARS: usize = 220;

/// Cleans text for speech and splits it into sentence-sized chunks, so the
/// first sentence can play while the next ones are synthesized.
pub fn split_for_speech(text: &str) -> Vec<String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !matches!(c, '*' | '#' | '`' | '_' | '>' | '|'))
        .collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    for sentence in sentences(&cleaned) {
        if !current.is_empty()
            && current.chars().count() + sentence.chars().count() > MAX_CHUNK_CHARS
        {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(sentence);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn sentences(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        let mut end = trimmed.len();
        let mut chars = trimmed.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let ends_sentence = matches!(c, '.' | '!' | '?' | ';' | ':' | '\n');
            let next_is_space = chars.peek().is_none_or(|(_, n)| n.is_whitespace());
            if ends_sentence && next_is_space {
                end = i + c.len_utf8();
                break;
            }
        }
        let (sentence, tail) = trimmed.split_at(end);
        rest = tail;
        let sentence = sentence.trim();
        Some(sentence)
    })
    .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_halves_length_and_keeps_dc() {
        let input = vec![0.5; 48_000];
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resample_upsamples_with_interpolation() {
        let out = resample(&[0.0, 1.0], 8_000, 16_000);
        assert_eq!(out, vec![0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn splits_sentences_and_strips_markdown() {
        let chunks = split_for_speech("**Crase** é fusão. Use antes de horas! Certo?");
        assert_eq!(chunks, vec!["Crase é fusão. Use antes de horas! Certo?"]);
    }

    #[test]
    fn long_text_is_split_into_chunks() {
        let sentence = "Esta frase tem tamanho médio para o teste de divisão. ";
        let chunks = split_for_speech(&sentence.repeat(10));
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= MAX_CHUNK_CHARS));
    }

    #[test]
    fn decimal_points_do_not_split() {
        let chunks = split_for_speech("O valor é 3.5 por cento. Fim.");
        assert_eq!(chunks, vec!["O valor é 3.5 por cento. Fim."]);
    }

    #[test]
    fn rms_of_constant() {
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
    }
}
