//! Microphone capture in 30 ms blocks at 16 kHz mono, recording with an
//! automatic cutoff on silence, and the noise gate (voice-input/audio.py).
//!
//! Silence is judged against two references at once: the noise floor
//! measured during the first CALIBRATION_MS, and the typical speech level
//! heard so far in this recording (the user's own voice, closest to the
//! mic). Anything below `speech * peak_ratio` counts as silence even above
//! the floor: that keeps voices coming out of the speakers (a call, echo)
//! from holding the recording open. That speech level is the 75th
//! percentile of the loud blocks, not the loudest one, so a cough or a
//! knock on the desk doesn't make the user's normal voice count as silence.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub const SAMPLE_RATE: usize = 16_000;
pub const BLOCK_MS: usize = 30;
pub const BLOCK: usize = SAMPLE_RATE * BLOCK_MS / 1000;
const CALIBRATION_MS: f64 = 300.0;
const MIN_SPEECH_MS: usize = 400;
const MAX_RECORDING_S: f64 = 300.0;
const SPEECH_PERCENTILE: f64 = 75.0;

pub struct Cancelled;

/// An open microphone. Blocks of BLOCK samples arrive on `blocks`; the
/// device closes when this is dropped.
pub struct Mic {
    _stream: cpal::Stream,
    blocks: Receiver<Vec<f32>>,
}

impl Mic {
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("no microphone")?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let channels = supported.channels() as usize;
        let rate = supported.sample_rate() as f64;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        // ~3 s of backlog; a stalled reader drops blocks instead of growing.
        let (tx, rx) = sync_channel::<Vec<f32>>(100);
        let mut resampler = Resampler::new(rate, SAMPLE_RATE as f64);
        let mut pending: Vec<f32> = Vec::with_capacity(BLOCK * 2);
        let mut mono: Vec<f32> = Vec::new();
        let mut push = move |mono: &[f32]| {
            resampler.process(mono, &mut pending);
            while pending.len() >= BLOCK {
                let block: Vec<f32> = pending.drain(..BLOCK).collect();
                let _ = tx.try_send(block);
            }
        };
        let err = |e| eprintln!("[mic] stream error: {e}");
        macro_rules! build {
            ($t:ty, $conv:expr) => {
                device.build_input_stream::<$t, _, _>(
                    config.clone(),
                    move |data: &[$t], _| {
                        mono.clear();
                        mono.extend(data.chunks(channels).map(|f| f.iter().map(|&s| $conv(s)).sum::<f32>() / channels as f32));
                        push(&mono);
                    },
                    err,
                    None,
                )
            };
        }
        let stream = match format {
            cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
            cpal::SampleFormat::I16 => build!(i16, |s: i16| s as f32 / 32768.0),
            cpal::SampleFormat::I32 => build!(i32, |s: i32| s as f32 / 2_147_483_648.0),
            cpal::SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
            other => return Err(format!("unsupported mic format {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, blocks: rx })
    }

    /// Next 30 ms block; None if the device stopped delivering.
    pub fn read(&self) -> Option<Vec<f32>> {
        match self.blocks.recv_timeout(Duration::from_secs(2)) {
            Ok(b) => Some(b),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }
}

/// Windowed-sinc resampler (Blackman window, 32 taps per side), streaming.
/// Low-passes at the lower of the two Nyquist frequencies.
pub struct Resampler {
    ratio: f64, // input samples per output sample
    pos: f64,   // position of the next output sample in `hist` coordinates
    hist: Vec<f32>,
    cutoff: f64,
}

const TAPS: isize = 32;

impl Resampler {
    pub fn new(from: f64, to: f64) -> Self {
        Self { ratio: from / to, pos: TAPS as f64, hist: vec![0.0; TAPS as usize], cutoff: (to / from).min(1.0) * 0.94 }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if (self.ratio - 1.0).abs() < 1e-9 {
            out.extend_from_slice(input);
            return;
        }
        self.hist.extend_from_slice(input);
        let half = TAPS as f64 * self.ratio.max(1.0);
        while self.pos + half < self.hist.len() as f64 {
            let center = self.pos;
            let lo = (center - half).ceil() as isize;
            let hi = (center + half).floor() as isize;
            let (mut acc, mut norm) = (0.0f64, 0.0f64);
            for i in lo.max(0)..=hi.min(self.hist.len() as isize - 1) {
                let x = i as f64 - center;
                let arg = x * self.cutoff;
                let sinc = if arg.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * arg).sin() / (std::f64::consts::PI * arg) };
                let w = 0.42 + 0.5 * (std::f64::consts::PI * x / half).cos() + 0.08 * (2.0 * std::f64::consts::PI * x / half).cos();
                let k = sinc * w;
                acc += self.hist[i as usize] as f64 * k;
                norm += k;
            }
            out.push(if norm.abs() > 1e-9 { (acc / norm) as f32 } else { 0.0 });
            self.pos += self.ratio;
        }
        // keep enough history for the next window
        let keep_from = (self.pos - half).floor().max(0.0) as usize;
        if keep_from > 0 {
            self.hist.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

pub fn rms(block: &[f32]) -> f32 {
    if block.is_empty() {
        return 0.0;
    }
    (block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32).sqrt()
}

/// numpy.percentile with linear interpolation.
pub fn percentile(values: &[f32], p: f64) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let rank = p / 100.0 * (v.len() - 1) as f64;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    let t = (rank - lo as f64) as f32;
    v[lo] + (v[hi] - v[lo]) * t
}

pub struct RecordParams {
    pub silence_hold_ms: usize,
    pub silence_margin: f32,
    pub peak_ratio: f32,
    /// Give up if no speech starts within this time (wake-word starts).
    pub start_timeout_ms: Option<f64>,
}

/// Records until sustained silence. `should_cancel` is polled every block
/// (second hotkey tap, Esc); `on_level` gets 0..1 for the level bars.
pub fn record_until_silence(
    mic: &Mic,
    p: &RecordParams,
    mut should_cancel: impl FnMut() -> bool,
    mut on_level: impl FnMut(f32),
) -> Result<Vec<f32>, Cancelled> {
    let mut pcm: Vec<f32> = Vec::new();
    let mut floor_samples: Vec<f32> = Vec::new();
    let mut floor: Option<f32> = None;
    let mut loud: Vec<f32> = Vec::new();
    let mut peak = 0.0f32;
    let (mut silence_ms, mut speech_ms) = (0usize, 0usize);
    let start = Instant::now();
    loop {
        if should_cancel() {
            return Err(Cancelled);
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        if elapsed > MAX_RECORDING_S * 1000.0 {
            break;
        }
        let Some(block) = mic.read() else { break };
        let level = rms(&block);
        pcm.extend_from_slice(&block);
        if elapsed < CALIBRATION_MS || floor_samples.is_empty() {
            // Keeps collecting past CALIBRATION_MS if the first block was
            // late (cold device): a mean over nothing would be NaN.
            floor_samples.push(level);
            on_level(0.0);
            continue;
        }
        let nf = *floor.get_or_insert_with(|| (floor_samples.iter().sum::<f32>() / floor_samples.len() as f32).max(1e-4));
        let floor_threshold = nf * p.silence_margin;
        if level > floor_threshold {
            loud.push(level);
            peak = percentile(&loud, SPEECH_PERCENTILE);
        }
        let threshold = floor_threshold.max(peak * p.peak_ratio);
        on_level((level / (threshold * 2.5)).min(1.0));
        if level > threshold {
            silence_ms = 0;
            speech_ms += BLOCK_MS;
        } else {
            silence_ms += BLOCK_MS;
        }
        if speech_ms >= MIN_SPEECH_MS && silence_ms >= p.silence_hold_ms {
            break;
        }
        if let Some(t) = p.start_timeout_ms {
            if speech_ms < MIN_SPEECH_MS && elapsed > t {
                return Err(Cancelled);
            }
        }
    }
    Ok(pcm)
}

/// Mutes every 30 ms block quieter than `peak_ratio` of the speech level,
/// keeping one block of context on each side so word edges survive.
pub fn noise_gate(pcm: &mut [f32], peak_ratio: f32) {
    if pcm.is_empty() {
        return;
    }
    let n = pcm.len().div_ceil(BLOCK);
    let levels: Vec<f32> = (0..n).map(|i| rms(&pcm[i * BLOCK..((i + 1) * BLOCK).min(pcm.len())]).max(0.0)).collect();
    // numpy pads the last block with zeros before the mean
    let levels: Vec<f32> = levels
        .iter()
        .enumerate()
        .map(|(i, &l)| {
            let len = ((i + 1) * BLOCK).min(pcm.len()) - i * BLOCK;
            if len < BLOCK {
                (l * l * len as f32 / BLOCK as f32).sqrt()
            } else {
                l
            }
        })
        .collect();
    let max = levels.iter().cloned().fold(0.0f32, f32::max);
    let voiced: Vec<f32> = levels.iter().cloned().filter(|&l| l > max * 0.02).collect();
    let peak = if voiced.is_empty() { 0.0 } else { percentile(&voiced, SPEECH_PERCENTILE) };
    if peak <= 0.0 {
        return;
    }
    let keep: Vec<bool> = levels.iter().map(|&l| l >= peak * peak_ratio).collect();
    for i in 0..n {
        // np.roll wraps around: the first and last blocks see each other
        let prev = keep[(i + n - 1) % n];
        let next = keep[(i + 1) % n];
        if !(keep[i] || prev || next) {
            let end = ((i + 1) * BLOCK).min(pcm.len());
            pcm[i * BLOCK..end].fill(0.0);
        }
    }
}

/// Mic sensitivity (0..100) -> (silence margin, peak ratio), interpolated.
pub fn sensitivity(value: f64) -> (f32, f32) {
    const POINTS: [(f64, f64, f64); 5] = [(0.0, 6.0, 0.30), (25.0, 5.0, 0.25), (50.0, 3.5, 0.12), (75.0, 2.5, 0.05), (100.0, 1.8, 0.02)];
    let v = value.clamp(0.0, 100.0);
    for w in POINTS.windows(2) {
        let ((x0, m0, r0), (x1, m1, r1)) = (w[0], w[1]);
        if v <= x1 {
            let t = (v - x0) / (x1 - x0);
            return ((m0 + (m1 - m0) * t) as f32, (r0 + (r1 - r0) * t) as f32);
        }
    }
    (1.8, 0.02)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_like_numpy() {
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 75.0), 3.25);
        assert_eq!(percentile(&[5.0], 75.0), 5.0);
    }

    #[test]
    fn sensitivity_points() {
        assert_eq!(sensitivity(50.0), (3.5, 0.12));
        let (m, r) = sensitivity(62.5);
        assert!((m - 3.0).abs() < 1e-5 && (r - 0.085).abs() < 1e-5);
    }

    #[test]
    fn resampler_keeps_a_tone() {
        let mut r = Resampler::new(48_000.0, 16_000.0);
        let input: Vec<f32> = (0..48_000).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin()).collect();
        let mut out = Vec::new();
        for chunk in input.chunks(441) {
            r.process(chunk, &mut out);
        }
        assert!((out.len() as i64 - 16_000).abs() < 80, "{}", out.len());
        let expect: Vec<f32> = (0..out.len()).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin()).collect();
        // skip the start-up window; compare amplitude, not phase
        assert!((rms(&out[400..]) - rms(&expect[400..])).abs() < 0.02);
    }

    #[test]
    fn gate_mutes_quiet_blocks() {
        let mut pcm = vec![0.001f32; BLOCK * 10];
        for s in &mut pcm[BLOCK * 4..BLOCK * 6] {
            *s = 0.5;
        }
        noise_gate(&mut pcm, 0.12);
        assert_eq!(pcm[0], 0.0);
        assert_eq!(pcm[BLOCK * 3], 0.001); // context block kept
        assert_eq!(pcm[BLOCK * 5], 0.5);
        assert_eq!(pcm[BLOCK * 8], 0.0);
    }
}
