//! Whisper large-v3-turbo (q8_0) resident on the GPU through whisper.cpp
//! with Vulkan, unloaded after 30 idle minutes (voice-input/stt.py). See
//! native/bench/RESULTS.md for why this model and backend.
//!
//! The Silero VAD model does two jobs: it trims silence before Whisper,
//! like faster-whisper's vad_filter did (fewer hallucinations on quiet
//! audio), and it tells the wake listener whether a burst of sound is a
//! voice at all before the GPU sees it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext,
    WhisperVadContextParams, WhisperVadParams,
};

pub const WHISPER_FILE: &str = "ggml-large-v3-turbo-q8_0.bin";
pub const VAD_FILE: &str = "ggml-silero-v6.2.0.bin";
const IDLE_UNLOAD: Duration = Duration::from_secs(1800);

pub fn models_dir() -> PathBuf {
    ct_core::paths::local_dir().join("models")
}

pub fn prompt_for(language: &str) -> &'static str {
    match language {
        "es" => "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress.",
        "en" => "Dictation in English for Claude Code: commit, repository, hook, pull request, branch, terminal, script, TypeScript, Docker, WordPress.",
        _ => "",
    }
}

struct Loaded {
    ctx: WhisperContext,
    vad_path: Option<String>,
}

struct Inner {
    model: Option<Loaded>,
    last_use: Instant,
    gpu_failed: bool,
}

/// Shared transcriber. `transcribe` is serialized: the wake listener and a
/// dictation never run the model at the same time (v0.5 could).
#[derive(Clone)]
pub struct Transcriber(Arc<Mutex<Inner>>);

impl Transcriber {
    pub fn new() -> Self {
        let t = Self(Arc::new(Mutex::new(Inner { model: None, last_use: Instant::now(), gpu_failed: false })));
        let weak = Arc::downgrade(&t.0);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            let Some(inner) = weak.upgrade() else { return };
            let mut g = inner.lock().unwrap();
            if g.model.is_some() && g.last_use.elapsed() > IDLE_UNLOAD {
                g.model = None;
                println!("[model] unloaded");
            }
        });
        t
    }

    fn ensure_loaded(g: &mut Inner) -> Result<(), String> {
        if g.model.is_some() {
            return Ok(());
        }
        let path = models_dir().join(WHISPER_FILE);
        if !path.exists() {
            return Err("model not downloaded yet".into());
        }
        println!("[model] loading");
        let load = |gpu: bool| {
            let mut p = WhisperContextParameters::default();
            p.use_gpu(gpu);
            WhisperContext::new_with_params(&path, p)
        };
        let ctx = match (!g.gpu_failed).then(|| load(true)) {
            Some(Ok(c)) => c,
            _ => {
                // No usable GPU: CPU works everywhere, just slower.
                g.gpu_failed = true;
                println!("[model] gpu unavailable, using cpu");
                load(false).map_err(|e| e.to_string())?
            }
        };
        let vad = models_dir().join(VAD_FILE);
        g.model = Some(Loaded { ctx, vad_path: vad.exists().then(|| vad.to_string_lossy().into_owned()) });
        println!("[model] loaded");
        Ok(())
    }

    /// Loads the model and runs it once on silence, so the first real
    /// dictation doesn't pay for the upload or the Vulkan pipelines.
    pub fn warm_up(&self) {
        let _ = self.transcribe(&vec![0.0; 16_000], Some("es"), "");
    }

    /// `language` None = detect. An empty prompt skips the vocabulary hint
    /// (the wake check: a prompt that says "Claude" nudges Whisper into it).
    pub fn transcribe(&self, pcm: &[f32], language: Option<&str>, prompt: &str) -> Result<String, String> {
        let mut g = self.0.lock().unwrap();
        Self::ensure_loaded(&mut g)?;
        g.last_use = Instant::now();
        let loaded = g.model.as_ref().unwrap();
        let mut state = loaded.ctx.create_state().map_err(|e| e.to_string())?;
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(Some(language.unwrap_or("auto")));
        if !prompt.is_empty() {
            p.set_initial_prompt(prompt);
        }
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        p.set_no_context(true);
        p.set_suppress_blank(true);
        if let Some(vad) = &loaded.vad_path {
            p.set_vad_model_path(Some(vad));
            p.enable_vad(true);
            p.set_vad_params(WhisperVadParams::default());
        }
        state.full(p, pcm).map_err(|e| e.to_string())?;
        let mut text = String::new();
        for seg in state.as_iter() {
            text.push_str(&seg.to_string());
        }
        g.last_use = Instant::now();
        Ok(text.trim().to_string())
    }
}

/// Silero VAD on the CPU, for the wake listener's bursts.
pub struct Vad(Option<WhisperVadContext>);

unsafe impl Send for Vad {}

impl Vad {
    pub fn load() -> Self {
        let path = models_dir().join(VAD_FILE);
        if !path.exists() {
            return Self(None);
        }
        let mut p = WhisperVadContextParams::new();
        p.set_use_gpu(false);
        p.set_n_threads(1);
        Self(WhisperVadContext::new(&path.to_string_lossy(), p).ok())
    }

    /// True when the audio holds speech. Without the model every burst
    /// counts as speech (v0.5 behavior).
    pub fn has_speech(&mut self, pcm: &[f32]) -> bool {
        let Some(ctx) = self.0.as_mut() else { return true };
        if ctx.detect_speech(pcm).is_err() {
            return true;
        }
        // ~32 ms frames: a syllable or more above 0.5.
        ctx.probabilities().iter().filter(|&&p| p > 0.5).count() >= 3
    }
}
