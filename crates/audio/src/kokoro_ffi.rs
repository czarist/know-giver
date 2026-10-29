//! Kokoro kept loaded in the app, through the sherpa-onnx C library.
//!
//! Starting the sherpa-onnx program for every sentence loads the 300 MB model
//! each time, about two seconds before any sound. Here the library that ships
//! with that program (`sherpa-onnx-c-api.dll`, pinned v1.12.20) is loaded once
//! and the model stays in memory. Nothing is compiled: the DLL is the one
//! already downloaded with the runtime.
//!
//! This is the only module with `unsafe` code. The structs mirror
//! `include/sherpa-onnx/c-api/c-api.h` of v1.12.20 field by field; a different
//! runtime version is refused by the download pin, not checked here. If the
//! library cannot be loaded the caller falls back to the program.
#![allow(unsafe_code)]

use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex};

use crate::{AudioClip, AudioError};

#[repr(C)]
struct VitsConfig {
    model: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    noise_scale_w: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
struct MatchaConfig {
    acoustic_model: *const c_char,
    vocoder: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
struct KokoroConfig {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
    dict_dir: *const c_char,
    lexicon: *const c_char,
    lang: *const c_char,
}

#[repr(C)]
struct KittenConfig {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
}

#[repr(C)]
struct ZipvoiceConfig {
    tokens: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    vocoder: *const c_char,
    data_dir: *const c_char,
    lexicon: *const c_char,
    feat_scale: f32,
    t_shift: f32,
    target_rms: f32,
    guidance_scale: f32,
}

#[repr(C)]
struct ModelConfig {
    vits: VitsConfig,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    matcha: MatchaConfig,
    kokoro: KokoroConfig,
    kitten: KittenConfig,
    zipvoice: ZipvoiceConfig,
}

#[repr(C)]
struct TtsConfig {
    model: ModelConfig,
    rule_fsts: *const c_char,
    max_num_sentences: i32,
    rule_fars: *const c_char,
    silence_scale: f32,
}

#[repr(C)]
struct GeneratedAudio {
    samples: *const f32,
    n: i32,
    sample_rate: i32,
}

/// Opaque `SherpaOnnxOfflineTts`.
#[repr(C)]
struct OfflineTts {
    _private: [u8; 0],
}

type ProgressCallback = unsafe extern "C" fn(*const f32, i32, f32, *mut c_void) -> i32;
type CreateFn = unsafe extern "C" fn(*const TtsConfig) -> *const OfflineTts;
type DestroyFn = unsafe extern "C" fn(*const OfflineTts);
type GenerateFn = unsafe extern "C" fn(
    *const OfflineTts,
    *const c_char,
    i32,
    f32,
    ProgressCallback,
    *mut c_void,
) -> *const GeneratedAudio;
type DestroyAudioFn = unsafe extern "C" fn(*const GeneratedAudio);

/// A loaded Portuguese Kokoro model.
pub(crate) struct LoadedKokoro {
    tts: *const OfflineTts,
    generate: GenerateFn,
    destroy: DestroyFn,
    destroy_audio: DestroyAudioFn,
    /// Keeps the DLL mapped while `tts` and the function pointers live.
    _library: libloading::Library,
}

// SAFETY: the handle is only used behind the `Mutex` in `CACHE`, one call at
// a time; sherpa-onnx does not tie it to the thread that created it.
unsafe impl Send for LoadedKokoro {}

impl std::fmt::Debug for LoadedKokoro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedKokoro").finish_non_exhaustive()
    }
}

impl Drop for LoadedKokoro {
    fn drop(&mut self) {
        // SAFETY: `tts` came from `SherpaOnnxCreateOfflineTts` of the library
        // still held in `_library`, and is destroyed exactly once.
        unsafe { (self.destroy)(self.tts) }
    }
}

/// What a loaded model was made from; a change means loading again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    library: PathBuf,
    model: PathBuf,
    threads: usize,
}

type Cache = Mutex<Option<(Key, Arc<Mutex<LoadedKokoro>>)>>;
static CACHE: Cache = Mutex::new(None);

/// The C library next to the sherpa-onnx program: `bin/x.exe` → `lib/`.
pub(crate) fn library_for(exe: &Path) -> Option<PathBuf> {
    let root = exe.parent()?.parent()?;
    let name = if cfg!(windows) {
        "sherpa-onnx-c-api.dll"
    } else if cfg!(target_os = "macos") {
        "libsherpa-onnx-c-api.dylib"
    } else {
        "libsherpa-onnx-c-api.so"
    };
    [root.join("lib").join(name), exe.with_file_name(name)]
        .into_iter()
        .find(|path| path.is_file())
}

/// The loaded model for `model`, loading it the first time (about two
/// seconds). Errors mean the library is unusable here.
pub(crate) fn loaded(
    library: &Path,
    model: &Path,
    threads: usize,
) -> Result<Arc<Mutex<LoadedKokoro>>, AudioError> {
    let key = Key {
        library: library.to_path_buf(),
        model: model.to_path_buf(),
        threads,
    };
    let mut cache = CACHE
        .lock()
        .map_err(|_| AudioError::Tts("Kokoro cache poisoned".into()))?;
    if let Some((cached, kokoro)) = cache.as_ref()
        && *cached == key
    {
        return Ok(Arc::clone(kokoro));
    }
    // Free the old model before loading another.
    *cache = None;
    let started = std::time::Instant::now();
    let kokoro = Arc::new(Mutex::new(load(library, model, threads)?));
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Kokoro loaded in the app"
    );
    *cache = Some((key, Arc::clone(&kokoro)));
    Ok(kokoro)
}

/// Frees the model kept in memory (when another voice is chosen).
pub fn release() {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = None;
    }
}

fn c_path(path: &Path) -> Result<CString, AudioError> {
    let text = path
        .to_str()
        .ok_or_else(|| AudioError::Tts(format!("path is not UTF-8: {}", path.display())))?;
    CString::new(text).map_err(|_| AudioError::Tts("path contains a NUL byte".into()))
}

fn c_text(text: &str) -> Result<CString, AudioError> {
    CString::new(text).map_err(|_| AudioError::Tts("text contains a NUL byte".into()))
}

fn load(library: &Path, model: &Path, threads: usize) -> Result<LoadedKokoro, AudioError> {
    let fail = |what: &str, e: libloading::Error| AudioError::Tts(format!("{what}: {e}"));
    // SAFETY: loading runs the DLL's initializers. It is the sherpa-onnx
    // library downloaded and pinned by the app, the same code the program
    // already runs. Its own dependencies (onnxruntime) sit in the same folder.
    let library = unsafe { open(library) }.map_err(|e| fail("could not load sherpa-onnx", e))?;
    // SAFETY: the symbol types match the C declarations of v1.12.20.
    let (create, destroy, generate, destroy_audio) = unsafe {
        (
            *library
                .get::<CreateFn>(b"SherpaOnnxCreateOfflineTts\0")
                .map_err(|e| fail("missing symbol", e))?,
            *library
                .get::<DestroyFn>(b"SherpaOnnxDestroyOfflineTts\0")
                .map_err(|e| fail("missing symbol", e))?,
            *library
                .get::<GenerateFn>(b"SherpaOnnxOfflineTtsGenerateWithProgressCallbackWithArg\0")
                .map_err(|e| fail("missing symbol", e))?,
            *library
                .get::<DestroyAudioFn>(b"SherpaOnnxDestroyOfflineTtsGeneratedAudio\0")
                .map_err(|e| fail("missing symbol", e))?,
        )
    };

    let model_file = c_path(&model.join("model.onnx"))?;
    let voices = c_path(&model.join("voices.bin"))?;
    let tokens = c_path(&model.join("tokens.txt"))?;
    let data_dir = c_path(&model.join("espeak-ng-data"))?;
    let lang = c_text("pt-br")?;
    let provider = c_text("cpu")?;
    let empty = c_text("")?;
    let config = TtsConfig {
        model: ModelConfig {
            vits: VitsConfig {
                model: ptr::null(),
                lexicon: ptr::null(),
                tokens: ptr::null(),
                data_dir: ptr::null(),
                noise_scale: 0.0,
                noise_scale_w: 0.0,
                length_scale: 0.0,
                dict_dir: ptr::null(),
            },
            num_threads: i32::try_from(threads).unwrap_or(2),
            debug: 0,
            provider: provider.as_ptr(),
            matcha: MatchaConfig {
                acoustic_model: ptr::null(),
                vocoder: ptr::null(),
                lexicon: ptr::null(),
                tokens: ptr::null(),
                data_dir: ptr::null(),
                noise_scale: 0.0,
                length_scale: 0.0,
                dict_dir: ptr::null(),
            },
            kokoro: KokoroConfig {
                model: model_file.as_ptr(),
                voices: voices.as_ptr(),
                tokens: tokens.as_ptr(),
                data_dir: data_dir.as_ptr(),
                // Speed is given per sentence instead.
                length_scale: 1.0,
                dict_dir: ptr::null(),
                lexicon: empty.as_ptr(),
                lang: lang.as_ptr(),
            },
            kitten: KittenConfig {
                model: ptr::null(),
                voices: ptr::null(),
                tokens: ptr::null(),
                data_dir: ptr::null(),
                length_scale: 0.0,
            },
            zipvoice: ZipvoiceConfig {
                tokens: ptr::null(),
                encoder: ptr::null(),
                decoder: ptr::null(),
                vocoder: ptr::null(),
                data_dir: ptr::null(),
                lexicon: ptr::null(),
                feat_scale: 0.0,
                t_shift: 0.0,
                target_rms: 0.0,
                guidance_scale: 0.0,
            },
        },
        rule_fsts: empty.as_ptr(),
        max_num_sentences: 1,
        rule_fars: empty.as_ptr(),
        silence_scale: 0.2,
    };
    // SAFETY: `config` and every string it points to outlive the call; the
    // library copies what it keeps. A null result means the model failed.
    let tts = unsafe { create(&config) };
    if tts.is_null() {
        return Err(AudioError::Tts("sherpa-onnx could not load Kokoro".into()));
    }
    Ok(LoadedKokoro {
        tts,
        generate,
        destroy,
        destroy_audio,
        _library: library,
    })
}

#[cfg(windows)]
unsafe fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
    use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};
    // Look for onnxruntime.dll next to the library, not next to the app.
    // SAFETY: see `load`.
    unsafe { Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }.map(Into::into)
}

#[cfg(not(windows))]
unsafe fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
    // SAFETY: see `load`.
    unsafe { libloading::Library::new(path) }
}

/// Stops generation (returns 0) once `arg`'s check says so.
unsafe extern "C" fn keep_going(_: *const f32, _: i32, _: f32, arg: *mut c_void) -> i32 {
    if arg.is_null() {
        return 1;
    }
    // SAFETY: `arg` points at the `&dyn Fn` living on `speak`'s stack for
    // the whole call.
    let cancelled = unsafe { &*(arg as *const &dyn Fn() -> bool) };
    i32::from(!cancelled())
}

impl LoadedKokoro {
    /// Speaks `text` with speaker `sid` at `speed` (1.0 is normal).
    pub(crate) fn speak(
        &self,
        text: &str,
        sid: u32,
        speed: f32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AudioClip, AudioError> {
        let text = c_text(text)?;
        let sid = i32::try_from(sid).map_err(|_| AudioError::Tts("bad speaker".into()))?;
        let mut check: &dyn Fn() -> bool = cancelled;
        let arg = (&raw mut check).cast::<c_void>();
        // SAFETY: `self.tts` is live (dropped only with `self`); `text` and
        // `check` outlive the call, which does not keep them.
        let audio =
            unsafe { (self.generate)(self.tts, text.as_ptr(), sid, speed, keep_going, arg) };
        if cancelled() {
            if !audio.is_null() {
                // SAFETY: returned by this library and freed once.
                unsafe { (self.destroy_audio)(audio) };
            }
            return Err(AudioError::Cancelled);
        }
        if audio.is_null() {
            return Err(AudioError::Tts("Kokoro produced no audio".into()));
        }
        // SAFETY: a non-null result holds `n` samples until destroyed below.
        let (samples, rate) = unsafe {
            let a = &*audio;
            let samples = if a.samples.is_null() || a.n <= 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(a.samples, a.n as usize).to_vec()
            };
            let rate = a.sample_rate;
            (self.destroy_audio)(audio);
            (samples, rate)
        };
        let sample_rate =
            u32::try_from(rate).map_err(|_| AudioError::Tts("bad Kokoro sample rate".into()))?;
        if samples.is_empty() || samples.iter().any(|s| !s.is_finite()) {
            return Err(AudioError::Tts("Kokoro produced empty audio".into()));
        }
        Ok(AudioClip {
            samples: crate::soften(&samples, sample_rate),
            sample_rate,
        })
    }
}
