//! A few numbers the speaker and the dictation app share while Claude talks,
//! in a small named memory block (no files, no polling of disk):
//! - `level`: loudness of the audio playing right now (0..1), measured by the
//!   speaker as the samples reach the sound card; the talking pill's bars;
//! - `paused`: the pill's pause button; the speaker holds the player;
//! - `volume` + `volume_seq`: the pill's volume slider; the speaker applies a
//!   new value to the phrase already playing.

use crate::lock::named;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE};

const NAME: &str = "Local\\claudetalk_voice_link";

#[repr(C)]
pub struct VoiceLink {
    level: AtomicU32,
    paused: AtomicU32,
    volume: AtomicI32,
    volume_seq: AtomicU32,
}

struct Ptr(&'static VoiceLink);
unsafe impl Send for Ptr {}
unsafe impl Sync for Ptr {}

/// The shared block (created by whichever process asks first, zeroed).
/// None only if Windows refuses the mapping.
pub fn get() -> Option<&'static VoiceLink> {
    static LINK: OnceLock<Option<Ptr>> = OnceLock::new();
    LINK.get_or_init(|| unsafe {
        let size = std::mem::size_of::<VoiceLink>() as u32;
        let h = CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, size, named(NAME).as_ptr());
        if h.is_null() {
            return None;
        }
        // The handle stays open for the whole process: the block lives as
        // long as someone has it.
        let view = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, size as usize);
        if view.Value.is_null() {
            return None;
        }
        Some(Ptr(&*(view.Value as *const VoiceLink)))
    })
    .as_ref()
    .map(|p| p.0)
}

impl VoiceLink {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn set_level(&self, v: f32) {
        self.level.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed) != 0
    }

    pub fn set_paused(&self, on: bool) {
        self.paused.store(on as u32, Ordering::Relaxed);
    }

    /// A new volume (0-100) for what is playing now.
    pub fn push_volume(&self, v: i32) {
        self.volume.store(v.clamp(0, 100), Ordering::Relaxed);
        self.volume_seq.fetch_add(1, Ordering::Release);
    }

    /// (sequence, volume): the sequence changes on every push.
    pub fn volume(&self) -> (u32, i32) {
        let seq = self.volume_seq.load(Ordering::Acquire);
        (seq, self.volume.load(Ordering::Relaxed))
    }
}

/// Loudness of a block of samples as a 0..1 bar height: RMS in dB mapped
/// from -32 dB (between words) to -10 dB (loud syllable), then curved so
/// the syllables stand out. Edge's voices are normalized and sit around
/// -18 dB, which a linear scale would draw as nearly flat bars.
pub fn loudness(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    if rms <= 1e-6 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 32.0) / 22.0).clamp(0.0, 1.0).powf(1.4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_scale() {
        assert_eq!(loudness(&[]), 0.0);
        assert_eq!(loudness(&[0.0; 64]), 0.0);
        assert!(loudness(&[0.3; 64]) > 0.9);
        assert_eq!(loudness(&[0.01; 64]), 0.0);
        let speech = loudness(&[0.126; 64]); // -18 dB, typical
        assert!(speech > 0.4 && speech < 0.7, "{speech}");
    }

    #[test]
    fn shared_between_handles() {
        let a = get().unwrap();
        a.set_paused(true);
        assert!(get().unwrap().paused());
        a.set_paused(false);
        let (s0, _) = a.volume();
        a.push_volume(140);
        assert_eq!(a.volume(), (s0 + 1, 100));
    }
}
