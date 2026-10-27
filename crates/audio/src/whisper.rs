//! Speech-to-text with Whisper on candle (pure Rust, CPU).

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

        let device = Device::Cpu;
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
        tracing::info!(dir = %dir.display(), "whisper loaded");
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
        let pcm = resample(&clip.samples, clip.sample_rate, STT_SAMPLE_RATE);
        if pcm.len() < STT_SAMPLE_RATE as usize / 4 || rms(&pcm) < SILENCE_RMS {
            return Ok(String::new());
        }
        let started = std::time::Instant::now();
        let lang = self
            .tokenizer
            .special(&format!("<|{language}|>"))
            .ok_or_else(|| AudioError::Model(format!("unknown language {language}")))?;
        let prompt = [
            self.tokenizer.sot,
            lang,
            self.tokenizer.transcribe,
            self.tokenizer.no_timestamps,
        ];

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
        let text = text.trim().to_owned();
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

/// Decoding half of Whisper's byte-level BPE tokenizer. We never encode
/// text, so the merge rules are not needed.
struct Tokenizer {
    vocab: Vec<String>,
    specials: HashMap<String, u32>,
    byte_of: HashMap<char, u8>,
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
            vocab,
            byte_of: byte_decoder(),
            specials,
        })
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
    fn detects_repetition_loops() {
        let mut tokens: Vec<u32> = (0..8).collect();
        assert!(!is_looping(&tokens));
        tokens.extend(0..8);
        tokens.extend(0..8);
        assert!(is_looping(&tokens));
    }
}
