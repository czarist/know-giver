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
    // Expand abbreviations first: the dot in "art." must not end a sentence.
    let cleaned: String = crate::speech_text::normalize_for_speech(text)
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

/// Low-pass corner for [`soften`]: keeps speech clear, removes the hiss.
const SOFTEN_CUTOFF_HZ: f32 = 6_500.0;
/// Peak level after normalization (about -2 dBFS).
const SOFTEN_PEAK: f32 = 0.8;
const FADE_IN_SECS: f32 = 0.008;
const FADE_OUT_SECS: f32 = 0.020;
/// Pause added after each spoken chunk, so sentences do not run together.
const TRAILING_PAUSE_SECS: f32 = 0.18;

/// Makes synthesized speech gentler: a mild low-pass to take the edge off
/// sibilants, peak normalization so every chunk has the same loudness, short
/// fades against clicks, and a small pause at the end.
pub fn soften(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() || sample_rate == 0 {
        return samples.to_vec();
    }
    let rate = sample_rate as f32;
    let alpha = 1.0 - (-std::f32::consts::TAU * SOFTEN_CUTOFF_HZ / rate).exp();
    let mut state = 0.0;
    let mut out: Vec<f32> = samples
        .iter()
        .map(|&x| {
            state += alpha * (x - state);
            state
        })
        .collect();

    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 1e-4 {
        let gain = SOFTEN_PEAK / peak;
        out.iter_mut().for_each(|s| *s *= gain);
    }

    let fade_in = ((FADE_IN_SECS * rate) as usize).min(out.len());
    for (i, s) in out.iter_mut().take(fade_in).enumerate() {
        *s *= i as f32 / fade_in as f32;
    }
    let fade_out = ((FADE_OUT_SECS * rate) as usize).min(out.len());
    let len = out.len();
    for (i, s) in out.iter_mut().skip(len - fade_out).enumerate() {
        *s *= 1.0 - i as f32 / fade_out as f32;
    }

    out.extend(std::iter::repeat_n(
        0.0,
        (TRAILING_PAUSE_SECS * rate) as usize,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soften_normalizes_fades_and_pads() {
        let rate = 22_050;
        let tone: Vec<f32> = (0..rate)
            .map(|i| 0.3 * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin())
            .collect();
        let out = soften(&tone, rate);
        let pad = (TRAILING_PAUSE_SECS * rate as f32) as usize;
        assert_eq!(out.len(), tone.len() + pad);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - SOFTEN_PEAK).abs() < 0.01, "peak {peak}");
        assert_eq!(out[0], 0.0);
        assert!(out[tone.len()..].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn soften_dampens_high_frequencies() {
        let rate = 22_050;
        // Alternating samples: the highest frequency the rate can hold.
        let hiss: Vec<f32> = (0..2_000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let low: Vec<f32> = (0..2_000)
            .map(|i| 0.5 * (i as f32 * 200.0 * std::f32::consts::TAU / rate as f32).sin())
            .collect();
        // Compare before normalization by looking at the filter alone.
        let alpha = 1.0 - (-std::f32::consts::TAU * SOFTEN_CUTOFF_HZ / rate as f32).exp();
        let filter = |x: &[f32]| {
            let mut s = 0.0;
            x.iter()
                .map(|&v| {
                    s += alpha * (v - s);
                    s
                })
                .map(f32::abs)
                .fold(0.0f32, f32::max)
        };
        assert!(filter(&hiss) < 0.5 * 0.9);
        assert!(filter(&low) > 0.5 * 0.95);
    }

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
