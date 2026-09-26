//! The start chime: two soft overlapping notes (E5 -> A5), a sine with a
//! light second harmonic and an attack/decay envelope (voice-input/sounds.py).

use rodio::buffer::SamplesBuffer;
use std::num::NonZero;

const RATE: u32 = 44_100;
const VOLUME: f32 = 0.16;

fn tone(freq: f32, seconds: f32) -> Vec<f32> {
    let n = (RATE as f32 * seconds) as usize;
    let decay = seconds / 3.0;
    let attack = ((RATE as f32 * 0.008) as usize).max(1);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let mut env = (-t / decay).exp();
            if i < attack {
                env *= i as f32 / (attack - 1).max(1) as f32;
            }
            let w = (2.0 * std::f32::consts::PI * freq * t).sin() + 0.22 * (2.0 * std::f32::consts::PI * freq * 2.0 * t).sin();
            w * env * VOLUME
        })
        .collect()
}

pub fn chime_samples() -> Vec<f32> {
    let first = tone(659.25, 0.16);
    let second = tone(880.0, 0.28);
    let overlap = (RATE as f32 * 0.06) as usize;
    let mut out = vec![0.0f32; first.len() + second.len() - overlap];
    for (i, s) in first.iter().enumerate() {
        out[i] += s;
    }
    let off = first.len() - overlap;
    for (i, s) in second.iter().enumerate() {
        out[off + i] += s;
    }
    out
}

/// Plays the chime without blocking the caller.
pub fn chime_start() {
    std::thread::spawn(|| {
        let Ok(mut sink) = rodio::DeviceSinkBuilder::open_default_sink() else { return };
        sink.log_on_drop(false);
        let player = rodio::Player::connect_new(sink.mixer());
        player.append(SamplesBuffer::new(NonZero::new(1).unwrap(), NonZero::new(RATE).unwrap(), chime_samples()));
        player.sleep_until_end();
    });
}
