//! ct-bench MODEL CORPUS_DIR [--cpu] [--runs N]
//! Transcribes corpus/NN.wav with whisper.cpp and reports WER against
//! corpus/sentences.txt, per-file latency and model load time.

use std::time::Instant;
use unicode_normalization::UnicodeNormalization;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const PROMPT: &str = "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress.";

fn words(s: &str) -> Vec<String> {
    let plain: String = s
        .to_lowercase()
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    plain.split_whitespace().map(str::to_string).collect()
}

fn edits(a: &[String], b: &[String]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(x != y)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model = &args[1];
    let dir = std::path::Path::new(&args[2]);
    let cpu = args.iter().any(|a| a == "--cpu");
    let refs: Vec<String> = std::fs::read_to_string(dir.join("sentences.txt")).unwrap().lines().map(str::to_string).collect();

    let t0 = Instant::now();
    let mut cp = WhisperContextParameters::default();
    cp.use_gpu(!cpu);
    let ctx = WhisperContext::new_with_params(model, cp).expect("model");
    let mut state = ctx.create_state().unwrap();
    println!("load: {:.2}s", t0.elapsed().as_secs_f64());

    let (mut errs, mut total, mut time, mut audio) = (0usize, 0usize, 0f64, 0f64);
    for (i, reference) in refs.iter().enumerate() {
        let path = dir.join(format!("{i:02}.wav"));
        let mut r = hound::WavReader::open(&path).unwrap();
        let pcm: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(Some("es"));
        p.set_initial_prompt(PROMPT);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        p.set_no_context(true);
        p.set_single_segment(false);
        let t = Instant::now();
        state.full(p, &pcm).unwrap();
        let dt = t.elapsed().as_secs_f64();
        let mut text = String::new();
        for seg in state.as_iter() {
            text.push_str(&seg.to_string());
        }
        let (h, rf) = (words(&text), words(reference));
        let e = edits(&rf, &h);
        errs += e;
        total += rf.len();
        time += dt;
        audio += pcm.len() as f64 / 16000.0;
        println!("{i:02} {:.0}ms e={e} | {}", dt * 1000.0, text.trim());
    }
    println!("WER {:.2}% ({errs}/{total}) | total {:.2}s for {:.1}s audio (RTF {:.3})", 100.0 * errs as f64 / total as f64, time, audio, time / audio);
}
