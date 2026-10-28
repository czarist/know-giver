//! Speech-to-text with Whisper on candle (CPU by default, optional CUDA build).

use std::collections::HashMap;
use std::path::Path;

use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, Config, audio, model::Whisper};

use crate::signal::{resample, rms};
use crate::{AudioClip, AudioError, STT_SAMPLE_RATE};

/// Below this level the clip is treated as silence (Whisper hallucinates on
/// silence, so we skip it).
const SILENCE_RMS: f32 = 0.004;

/// Most context tokens put before the transcript, as in OpenAI's decoder
/// (half the decoder window minus one).
const MAX_CONTEXT_TOKENS: usize = 223;

/// A loaded Whisper model. Loading takes a few seconds; keep it around.
pub struct WhisperStt {
    model: Whisper,
    config: Config,
    mel_filters: Vec<f32>,
    tokenizer: Tokenizer,
    suppress: Tensor,
    device: Device,
}

impl std::fmt::Debug for WhisperStt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhisperStt").finish_non_exhaustive()
    }
}

fn model_err(e: impl std::fmt::Display) -> AudioError {
    AudioError::Model(e.to_string())
}

fn inference_device() -> Device {
    #[cfg(feature = "cuda")]
    if std::env::var("KG_WHISPER_DEVICE").as_deref() != Ok("cpu") {
        match Device::new_cuda(0) {
            Ok(device) => return device,
            Err(error) => tracing::warn!(%error, "CUDA unavailable; using CPU for Whisper"),
        }
    }
    Device::Cpu
}

impl WhisperStt {
    /// Loads `config.json`, `tokenizer.json` and `model.safetensors` from `dir`.
    pub fn load(dir: &Path) -> Result<Self, AudioError> {
        let read = |name: &str| {
            let path = dir.join(name);
            if !path.is_file() {
                return Err(AudioError::ModelMissing(path.display().to_string()));
            }
            std::fs::read_to_string(&path).map_err(|e| AudioError::io(path, e))
        };
        let config: Config = serde_json::from_str(&read("config.json")?).map_err(model_err)?;
        let tokenizer = Tokenizer::parse(&read("tokenizer.json")?)?;

        let device = inference_device();
        let weights = dir.join("model.safetensors");
        let tensors = candle_core::safetensors::load(&weights, &device).map_err(model_err)?;
        let vb = VarBuilder::from_tensors(tensors, m::DTYPE, &device);
        let model = Whisper::load(&vb, config.clone()).map_err(model_err)?;

        let suppress: Vec<f32> = (0..config.vocab_size as u32)
            .map(|id| {
                let special = id > tokenizer.eot;
                if special || config.suppress_tokens.contains(&id) {
                    f32::NEG_INFINITY
                } else {
                    0.0
                }
            })
            .collect();
        let suppress = Tensor::new(suppress.as_slice(), &device).map_err(model_err)?;
        let mel_filters = mel_filters(config.num_mel_bins, m::N_FFT, m::SAMPLE_RATE as f64);
        tracing::info!(dir = %dir.display(), ?device, "whisper loaded");
        Ok(Self {
            model,
            config,
            mel_filters,
            tokenizer,
            suppress,
            device,
        })
    }

    /// Transcribes a clip. `language` is a Whisper code like `"pt"`.
    pub fn transcribe(&mut self, clip: &AudioClip, language: &str) -> Result<String, AudioError> {
        self.transcribe_with_context(clip, language, "")
    }

    /// Transcribes a clip, conditioning Whisper on `context`, the text that
    /// came just before (for example the tutor's last sentence). Without it,
    /// short answers like "sim" are often heard as video-ending phrases such
    /// as "Tchau!", which Whisper learned from subtitles.
    pub fn transcribe_with_context(
        &mut self,
        clip: &AudioClip,
        language: &str,
        context: &str,
    ) -> Result<String, AudioError> {
        let pcm = resample(&clip.samples, clip.sample_rate, STT_SAMPLE_RATE);
        if pcm.len() < STT_SAMPLE_RATE as usize / 4 || rms(&pcm) < SILENCE_RMS {
            return Ok(String::new());
        }
        let started = std::time::Instant::now();
        let lang = self
            .tokenizer
            .special(&format!("<|{language}|>"))
            .ok_or_else(|| AudioError::Model(format!("unknown language {language}")))?;
        let mut prompt = Vec::new();
        if let Some(prev) = self.tokenizer.start_of_prev {
            let context = self.tokenizer.encode(context.trim());
            if !context.is_empty() {
                prompt.push(prev);
                prompt.extend_from_slice(
                    &context[context.len().saturating_sub(MAX_CONTEXT_TOKENS)..],
                );
            }
        }
        prompt.extend([
            self.tokenizer.sot,
            lang,
            self.tokenizer.transcribe,
            self.tokenizer.no_timestamps,
        ]);

        let mel = audio::pcm_to_mel(&self.config, &pcm, &self.mel_filters);
        let n_mels = self.config.num_mel_bins;
        let total_frames = mel.len() / n_mels;
        let mel =
            Tensor::from_vec(mel, (1, n_mels, total_frames), &self.device).map_err(model_err)?;
        let content_frames = pcm.len() / m::HOP_LENGTH;

        let mut text = String::new();
        let mut seek = 0;
        while seek < content_frames {
            let size = m::N_FRAMES.min(total_frames - seek);
            let segment = mel.narrow(2, seek, size).map_err(model_err)?;
            let tokens = self.decode(&segment, &prompt).map_err(model_err)?;
            text.push_str(&self.tokenizer.decode(&tokens));
            seek += m::N_FRAMES;
        }
        let mut text = text.trim().to_owned();
        if is_known_hallucination(&text) {
            // Better to ask the student to repeat than to answer "Tchau!".
            tracing::info!(%text, "dropped a typical Whisper hallucination");
            text.clear();
        }
        tracing::info!(
            audio_secs = clip.duration_secs(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            chars = text.len(),
            "transcribed"
        );
        Ok(text)
    }

    /// Greedy decoding of one 30-second segment.
    fn decode(&mut self, mel: &Tensor, prompt: &[u32]) -> candle_core::Result<Vec<u32>> {
        let features = self.model.encoder.forward(mel, true)?;
        let mut tokens = prompt.to_vec();
        let max_len = self.config.max_target_positions / 2;
        for i in 0..max_len {
            let input = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let ys = self.model.decoder.forward(&input, &features, i == 0)?;
            let (_, seq_len, _) = ys.dims3()?;
            let logits = self
                .model
                .decoder
                .final_linear(&ys.i((..1, seq_len - 1..))?)?
                .i(0)?
                .i(0)?;
            let logits = logits.broadcast_add(&self.suppress)?;
            let next = logits.argmax(0)?.to_scalar::<u32>()?;
            if next == self.tokenizer.eot {
                break;
            }
            tokens.push(next);
            if is_looping(&tokens[prompt.len()..]) {
                break;
            }
        }
        Ok(tokens[prompt.len()..].to_vec())
    }
}

/// Phrases Whisper invents for short or unclear clips, learned from the end
/// of subtitled videos. A student does not answer the tutor with them.
const HALLUCINATIONS: [&str; 10] = [
    "tchau",
    "até a próxima",
    "até mais",
    "até logo",
    "obrigado por assistir",
    "obrigada por assistir",
    "inscreva-se",
    "se inscreva",
    "legendas pela comunidade",
    "amara.org",
];

/// True when the whole transcript is one of [`HALLUCINATIONS`] (ignoring
/// punctuation and case), or contains a subtitle credit.
fn is_known_hallucination(text: &str) -> bool {
    let normalized = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '.'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = normalized.trim_end_matches('.');
    !normalized.is_empty()
        && HALLUCINATIONS
            .iter()
            .any(|h| normalized == *h || (h.len() > 12 && normalized.contains(h)))
}

/// Greedy decoding sometimes repeats a phrase forever; stop when the last
/// 8 tokens repeat the 8 before them.
fn is_looping(tokens: &[u32]) -> bool {
    const N: usize = 8;
    tokens.len() >= 3 * N
        && tokens[tokens.len() - N..] == tokens[tokens.len() - 2 * N..tokens.len() - N]
}

/// Mel filterbank as `librosa.filters.mel(htk=False, norm="slaney")`, which
/// is what Whisper was trained with. Row-major `[n_mels][n_fft / 2 + 1]`.
fn mel_filters(n_mels: usize, n_fft: usize, sample_rate: f64) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let fft_freqs: Vec<f64> = (0..n_freqs)
        .map(|j| j as f64 * sample_rate / n_fft as f64)
        .collect();
    let mel_max = hz_to_mel(sample_rate / 2.0);
    let mel_points: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(mel_max * i as f64 / (n_mels + 1) as f64))
        .collect();

    let mut weights = vec![0f32; n_mels * n_freqs];
    for i in 0..n_mels {
        let (left, center, right) = (mel_points[i], mel_points[i + 1], mel_points[i + 2]);
        let norm = 2.0 / (right - left);
        for (j, &f) in fft_freqs.iter().enumerate() {
            let lower = (f - left) / (center - left);
            let upper = (right - f) / (right - center);
            weights[i * n_freqs + j] = (lower.min(upper).max(0.0) * norm) as f32;
        }
    }
    weights
}

const F_SP: f64 = 200.0 / 3.0;
const MIN_LOG_HZ: f64 = 1000.0;
const MIN_LOG_MEL: f64 = MIN_LOG_HZ / F_SP;

fn log_step() -> f64 {
    6.4f64.ln() / 27.0
}

fn hz_to_mel(hz: f64) -> f64 {
    if hz >= MIN_LOG_HZ {
        MIN_LOG_MEL + (hz / MIN_LOG_HZ).ln() / log_step()
    } else {
        hz / F_SP
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    if mel >= MIN_LOG_MEL {
        MIN_LOG_HZ * (log_step() * (mel - MIN_LOG_MEL)).exp()
    } else {
        mel * F_SP
    }
}

/// Whisper's byte-level BPE tokenizer. Encoding is only used for the short
/// context prompt, so a plain merge loop is fast enough.
struct Tokenizer {
    vocab: Vec<String>,
    ids: HashMap<String, u32>,
    merge_rank: HashMap<(String, String), usize>,
    specials: HashMap<String, u32>,
    byte_of: HashMap<char, u8>,
    char_of: [char; 256],
    pieces: regex::Regex,
    start_of_prev: Option<u32>,
    sot: u32,
    eot: u32,
    transcribe: u32,
    no_timestamps: u32,
}

impl Tokenizer {
    fn parse(json: &str) -> Result<Self, AudioError> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(model_err)?;
        let vocab_map = value["model"]["vocab"]
            .as_object()
            .ok_or_else(|| AudioError::Model("tokenizer has no vocab".into()))?;
        let size = vocab_map
            .values()
            .filter_map(serde_json::Value::as_u64)
            .max()
            .unwrap_or(0) as usize
            + 1;
        let mut vocab = vec![String::new(); size];
        for (token, id) in vocab_map {
            if let Some(slot) = id.as_u64().and_then(|id| vocab.get_mut(id as usize)) {
                *slot = token.clone();
            }
        }
        let specials: HashMap<String, u32> = value["added_tokens"]
            .as_array()
            .map(|tokens| {
                tokens
                    .iter()
                    .filter_map(|t| {
                        Some((t["content"].as_str()?.to_owned(), t["id"].as_u64()? as u32))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let merge_rank = value["model"]["merges"]
            .as_array()
            .map(|merges| {
                merges
                    .iter()
                    .enumerate()
                    .filter_map(|(rank, merge)| {
                        let (a, b) = match merge {
                            serde_json::Value::String(s) => s.split_once(' ')?,
                            serde_json::Value::Array(pair) => {
                                (pair.first()?.as_str()?, pair.get(1)?.as_str()?)
                            }
                            _ => return None,
                        };
                        Some(((a.to_owned(), b.to_owned()), rank))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ids = vocab_map
            .iter()
            .filter_map(|(token, id)| Some((token.clone(), u32::try_from(id.as_u64()?).ok()?)))
            .collect();
        let byte_of = byte_decoder();
        let mut char_of = [' '; 256];
        for (&c, &b) in &byte_of {
            char_of[usize::from(b)] = c;
        }
        // GPT-2's pre-tokenizer pattern without the look-ahead the regex
        // crate lacks; it only changes how runs of spaces are split.
        let pieces =
            regex::Regex::new(r"'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+")
                .map_err(model_err)?;
        let find = |name: &str| {
            specials
                .get(name)
                .copied()
                .ok_or_else(|| AudioError::Model(format!("tokenizer lacks {name}")))
        };
        Ok(Self {
            sot: find(m::SOT_TOKEN)?,
            eot: find(m::EOT_TOKEN)?,
            transcribe: find(m::TRANSCRIBE_TOKEN)?,
            no_timestamps: find(m::NO_TIMESTAMPS_TOKEN)?,
            start_of_prev: specials.get("<|startofprev|>").copied(),
            vocab,
            ids,
            merge_rank,
            byte_of,
            char_of,
            pieces,
            specials,
        })
    }

    /// Encodes plain text into token ids (no special tokens).
    fn encode(&self, text: &str) -> Vec<u32> {
        let mut out = Vec::new();
        for piece in self.pieces.find_iter(text) {
            let mut parts: Vec<String> = piece
                .as_str()
                .bytes()
                .map(|b| self.char_of[usize::from(b)].to_string())
                .collect();
            loop {
                let best = parts
                    .windows(2)
                    .enumerate()
                    .filter_map(|(i, w)| {
                        self.merge_rank
                            .get(&(w[0].clone(), w[1].clone()))
                            .map(|rank| (*rank, i))
                    })
                    .min();
                let Some((_, i)) = best else { break };
                let right = parts.remove(i + 1);
                parts[i].push_str(&right);
            }
            out.extend(parts.iter().filter_map(|p| self.ids.get(p).copied()));
        }
        out
    }

    fn special(&self, name: &str) -> Option<u32> {
        self.specials.get(name).copied()
    }

    fn decode(&self, tokens: &[u32]) -> String {
        let bytes: Vec<u8> = tokens
            .iter()
            .filter(|&&id| id < self.eot)
            .filter_map(|&id| self.vocab.get(id as usize))
            .flat_map(|token| token.chars())
            .filter_map(|c| self.byte_of.get(&c).copied())
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// Inverse of GPT-2's `bytes_to_unicode`.
fn byte_decoder() -> HashMap<char, u8> {
    let printable = |b: u8| matches!(b, b'!'..=b'~' | 0xA1..=0xAC | 0xAE..=0xFF);
    let mut map = HashMap::with_capacity(256);
    let mut shifted = 0u32;
    for b in 0..=255u8 {
        let code = if printable(b) {
            u32::from(b)
        } else {
            shifted += 1;
            255 + shifted
        };
        if let Some(c) = char::from_u32(code) {
            map.insert(c, b);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_scale_round_trips() {
        for hz in [0.0, 440.0, 1000.0, 4000.0, 8000.0] {
            assert!((mel_to_hz(hz_to_mel(hz)) - hz).abs() < 1e-6);
        }
    }

    #[test]
    fn mel_filters_shape_and_triangles() {
        let filters = mel_filters(80, 400, 16_000.0);
        assert_eq!(filters.len(), 80 * 201);
        assert!(filters.iter().all(|w| *w >= 0.0));
        // Each filter peaks at a higher frequency bin than the previous one.
        let peaks: Vec<usize> = filters
            .chunks(201)
            .map(|row| {
                row.iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(i, _)| i)
                    .unwrap()
            })
            .collect();
        assert!(peaks.windows(2).all(|w| w[0] <= w[1]));
        assert!(filters.chunks(201).all(|row| row.iter().any(|w| *w > 0.0)));
    }

    #[test]
    fn byte_decoder_covers_all_bytes() {
        let map = byte_decoder();
        assert_eq!(map.len(), 256);
        assert_eq!(map.get(&'Ġ'), Some(&b' '));
        assert_eq!(map.get(&'a'), Some(&b'a'));
    }

    #[test]
    fn tokenizer_decodes_byte_level_tokens() {
        let json = r#"{
            "added_tokens": [
                {"id": 3, "content": "<|endoftext|>"},
                {"id": 4, "content": "<|startoftranscript|>"},
                {"id": 5, "content": "<|transcribe|>"},
                {"id": 6, "content": "<|notimestamps|>"},
                {"id": 7, "content": "<|pt|>"}
            ],
            "model": {"vocab": {"Ol": 0, "Ã¡": 1, "Ġmundo": 2}}
        }"#;
        let tok = Tokenizer::parse(json).unwrap();
        assert_eq!(tok.decode(&[0, 1, 2, 3]), "Olá mundo");
        assert_eq!(tok.special("<|pt|>"), Some(7));
    }

    #[test]
    fn tokenizer_encodes_with_merges_and_round_trips() {
        let json = r#"{
            "added_tokens": [
                {"id": 20, "content": "<|endoftext|>"},
                {"id": 21, "content": "<|startoftranscript|>"},
                {"id": 22, "content": "<|transcribe|>"},
                {"id": 23, "content": "<|notimestamps|>"},
                {"id": 24, "content": "<|startofprev|>"}
            ],
            "model": {
                "vocab": {"s": 0, "i": 1, "m": 2, "Ġ": 3, "si": 4, "sim": 5, "Ġsim": 6, "Ã": 7, "£": 8, "Ã£": 9, "o": 10, "n": 11},
                "merges": ["s i", "si m", "Ġ sim", "Ã £"]
            }
        }"#;
        let tok = Tokenizer::parse(json).unwrap();
        assert_eq!(tok.encode("sim sim"), vec![5, 6]);
        assert_eq!(tok.decode(&tok.encode("não sim")), "não sim");
        assert_eq!(tok.start_of_prev, Some(24));
    }

    #[test]
    fn recognizes_subtitle_hallucinations_only() {
        for text in [
            "Tchau!",
            " Até a próxima! ",
            "Obrigado por assistir.",
            "Legendas pela comunidade Amara.org",
        ] {
            assert!(is_known_hallucination(text), "{text}");
        }
        for text in [
            "Sim.",
            "Obrigado",
            "Até a vírgula muda o sentido",
            "Tchau, professor, até amanhã",
            "",
        ] {
            assert!(!is_known_hallucination(text), "{text}");
        }
    }

    #[test]
    fn detects_repetition_loops() {
        let mut tokens: Vec<u32> = (0..8).collect();
        assert!(!is_looping(&tokens));
        tokens.extend(0..8);
        tokens.extend(0..8);
        assert!(is_looping(&tokens));
    }
}
