//! `claudetalk speaker`: drains the speech queue (native/SPEC.md §4). One at a
//! time, guarded by `Local\claudetalk_speaker`. Unlike v0.5 it stays resident
//! for a while after the queue empties, waiting on a directory change
//! notification, so the next phrase starts without launching anything.

use ct_core::lock::{named, wide, NamedMutex};
use ct_core::queue::{self, Item, CUT_EVENT, DUCKING_EVENT, SPEAKER_MUTEX, SPEAKING_EVENT};
use ct_core::{log::log, paths, voice_link};
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    FILE_NOTIFY_CHANGE_FILE_NAME,
};
use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, WaitForSingleObject};

/// Volume while the user dictates, as a fraction of the phrase's volume,
/// and how fast it moves there (per 20 ms), so the dip is a fade.
const DUCK_LEVEL: f32 = 0.3;
const DUCK_STEP: f32 = 0.047;

struct Handle(HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

/// Manual-reset events shared with the dictation daemon.
fn named_event(name: &str, cell: &'static std::sync::OnceLock<Handle>) -> HANDLE {
    cell.get_or_init(|| Handle(unsafe { CreateEventW(std::ptr::null(), 1, 0, named(name).as_ptr()) })).0
}

fn speaking_event() -> HANDLE {
    static E: std::sync::OnceLock<Handle> = std::sync::OnceLock::new();
    named_event(SPEAKING_EVENT, &E)
}

fn ducked() -> bool {
    static E: std::sync::OnceLock<Handle> = std::sync::OnceLock::new();
    let h = named_event(DUCKING_EVENT, &E);
    (!h.is_null() && unsafe { WaitForSingleObject(h, 0) } == WAIT_OBJECT_0) || paths::ducking_flag().exists()
}

/// Moves the player's volume one step toward full or ducked, takes a new
/// volume from the talking pill's slider, and holds the player while the
/// pill's pause button is on.
struct Duck {
    base: f32,
    now: f32,
    volume_seq: u32,
}

impl Duck {
    fn step(&mut self, player: &Player) {
        if let Some(link) = voice_link::get() {
            let (seq, vol) = link.volume();
            if seq != self.volume_seq {
                self.volume_seq = seq;
                self.base = vol as f32 / 100.0;
            }
            if link.paused() != player.is_paused() {
                if link.paused() {
                    player.pause();
                    link.set_level(0.0);
                } else {
                    player.play();
                }
            }
        }
        let target = if ducked() { self.base * DUCK_LEVEL } else { self.base };
        if (self.now - target).abs() > 0.001 {
            self.now += (target - self.now).clamp(-DUCK_STEP, DUCK_STEP);
            player.set_volume(self.now);
        }
    }
}

/// How long the speaker waits for more phrases before exiting.
const IDLE_EXIT: Duration = Duration::from_secs(120);

/// Fixed phrases worth caching on disk (they play instantly and offline):
/// the current language's own lines.
fn cacheable(text: &str) -> bool {
    let p = ct_core::lang::current();
    [&p.left_on_screen, &p.sample_voice, &p.sample_volume].iter().any(|t| t.as_str() == text)
}

pub fn run() {
    let Some(mutex) = NamedMutex::new(SPEAKER_MUTEX) else { return };
    let cut = unsafe { CreateEventW(std::ptr::null(), 0, 0, named(CUT_EVENT).as_ptr()) };
    let mut audio: Option<(rodio::MixerDeviceSink, Player)> = None;
    loop {
        let Some(guard) = mutex.acquire(0) else { return }; // another speaker drains
        drain(&mut audio, cut);
        wait_for_more(&mut audio, cut);
        drop(guard);
        // A phrase may have landed between the last check and the release.
        if queue::pending().is_empty() {
            break;
        }
    }
    unsafe { CloseHandle(cut) };
}

fn drain(audio: &mut Option<(rodio::MixerDeviceSink, Player)>, cut: HANDLE) {
    while let Some(next) = queue::pending().into_iter().next() {
        if let Some(item) = queue::read_item(&next) {
            play_item(audio, &item, &next, cut);
        }
        let _ = fs::remove_file(&next);
    }
}

/// Waits up to IDLE_EXIT for new queue files, draining them as they come.
fn wait_for_more(audio: &mut Option<(rodio::MixerDeviceSink, Player)>, cut: HANDLE) {
    let dir = paths::queue_dir();
    let _ = fs::create_dir_all(&dir);
    let h = unsafe {
        FindFirstChangeNotificationW(wide(&dir.to_string_lossy()).as_ptr(), 0, FILE_NOTIFY_CHANGE_FILE_NAME)
    };
    if h.is_null() || h as isize == -1 {
        return;
    }
    let mut deadline = Instant::now() + IDLE_EXIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let r = unsafe { WaitForSingleObject(h, left.as_millis() as u32) };
        if r != WAIT_OBJECT_0 {
            break;
        }
        if !queue::pending().is_empty() {
            drain(audio, cut);
            deadline = Instant::now() + IDLE_EXIT;
        }
        if unsafe { FindNextChangeNotification(h) } == 0 {
            break;
        }
    }
    unsafe { FindCloseChangeNotification(h) };
    // Leaving: let the audio device go.
    *audio = None;
}

fn cache_path(voice: &str, rate: &str, text: &str) -> Option<PathBuf> {
    if !cacheable(text) {
        return None;
    }
    let key: String = Sha256::digest(format!("{voice}|{rate}|{text}").as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect();
    Some(paths::tts_cache_dir().join(format!("{key}.mp3")))
}

fn play_item(audio: &mut Option<(rodio::MixerDeviceSink, Player)>, item: &Item, file: &std::path::Path, cut: HANDLE) {
    // The user is dictating (voice-input/duck.py): the phrase goes a bit slower.
    let mut rate = if item.rate.is_empty() { "+0%".to_string() } else { item.rate.clone() };
    if paths::ducking_flag().exists() {
        rate = queue::ducked_rate(&rate);
    }
    let fallback = ct_core::settings::default_voice();
    let voice = if item.voice.is_empty() { &fallback } else { &item.voice };
    // Cut while we were getting ready: Stop-Speech deleted the item.
    if !file.exists() {
        return;
    }
    if audio.is_none() {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let player = Player::connect_new(sink.mixer());
                *audio = Some((sink, player));
            }
            Err(e) => {
                log(&format!("speaker: no audio device: {e}"));
                return;
            }
        }
    }
    let (_, player) = audio.as_ref().unwrap();
    let base = item.volume.clamp(0, 100) as f32 / 100.0;
    let volume_seq = voice_link::get().map(|l| l.volume().0).unwrap_or(0);
    // A phrase that starts mid-dictation starts already ducked.
    let mut duck = Duck { base, now: if ducked() { base * DUCK_LEVEL } else { base }, volume_seq };
    player.set_volume(duck.now);
    unsafe {
        ResetEvent(cut);
        SetEvent(speaking_event());
    }
    let _ = fs::write(paths::player_pid_file(), std::process::id().to_string());
    let _ = fs::write(paths::player_session_file(), &item.session);

    let (tx, rx) = channel::<Vec<u8>>();
    let cached = cache_path(voice, &rate, &item.text);
    match cached.as_ref().filter(|p| p.exists()) {
        Some(p) => {
            let _ = tx.send(fs::read(p).unwrap_or_default());
            drop(tx);
        }
        None => {
            let (text, voice2, rate2, cache) = (item.text.clone(), voice.to_string(), rate.clone(), cached.clone());
            std::thread::spawn(move || {
                // Tee the bytes into the cache when this is a fixed phrase.
                let (inner_tx, inner_rx) = channel::<Vec<u8>>();
                let keep = cache.is_some();
                let fwd = std::thread::spawn(move || {
                    let mut all = Vec::new();
                    for chunk in inner_rx {
                        if keep {
                            all.extend_from_slice(&chunk);
                        }
                        if tx.send(chunk).is_err() {
                            return None;
                        }
                    }
                    Some(all)
                });
                let result = crate::edge::synthesize(&text, &voice2, &rate2, &inner_tx);
                drop(inner_tx);
                let bytes = fwd.join().ok().flatten();
                match result {
                    Ok(()) => {
                        if let (Some(path), Some(bytes)) = (cache, bytes) {
                            if !bytes.is_empty() {
                                let _ = fs::create_dir_all(paths::tts_cache_dir());
                                let _ = ct_core::fsutil::write_atomic(&path, &bytes);
                            }
                        }
                    }
                    Err(e) => log(&format!("speaker: edge tts failed: {e}")),
                }
            });
        }
    }
    stream_to_player(player, rx, cut, &mut duck);
    if let Some(link) = voice_link::get() {
        link.set_level(0.0);
    }
    unsafe { ResetEvent(speaking_event()) };
    let _ = fs::remove_file(paths::player_pid_file());
    let _ = fs::remove_file(paths::player_session_file());
}

/// Decodes mp3 as it arrives and queues it on the player in small blocks,
/// so playback starts with the first chunk. Returns when it finished playing
/// or was cut.
fn stream_to_player(player: &Player, rx: Receiver<Vec<u8>>, cut: HANDLE, duck: &mut Duck) {
    let is_cut = || unsafe { WaitForSingleObject(cut, 0) } == WAIT_OBJECT_0;
    let reader = ChunkReader { rx: std::sync::Mutex::new(rx), buf: Vec::new(), pos: 0, total: 0 };
    let decoder = match Decoder::builder()
        .with_data(reader)
        .with_hint("mp3")
        .with_mime_type("audio/mpeg")
        .with_seekable(false)
        .build()
    {
        Ok(d) => d,
        Err(_) => return, // nothing arrived (network error, logged) or cut
    };
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let block = (rate.get() as usize * channels.get() as usize) / 10; // 100 ms
    let mut buf = Vec::with_capacity(block);
    for sample in decoder {
        buf.push(sample);
        if buf.len() >= block {
            player.append(Meter::new(SamplesBuffer::new(channels, rate, std::mem::take(&mut buf))));
            duck.step(player);
            if is_cut() {
                player.clear();
                return;
            }
        }
    }
    if !buf.is_empty() {
        player.append(Meter::new(SamplesBuffer::new(channels, rate, buf)));
    }
    if player.is_paused() {
        player.play();
    }
    while !player.empty() {
        duck.step(player);
        if unsafe { WaitForSingleObject(cut, 20) } == WAIT_OBJECT_0 {
            player.clear();
            return;
        }
    }
}

/// Passes samples through to the sound card and publishes their loudness
/// every ~25 ms (voice_link::level): the talking pill's bars follow what is
/// actually heard, not what was decoded ahead.
struct Meter<S: Source> {
    inner: S,
    window: Vec<f32>,
    size: usize,
}

impl<S: Source> Meter<S> {
    fn new(inner: S) -> Self {
        let size = (inner.sample_rate().get() as usize * inner.channels().get() as usize / 40).max(64);
        Self { inner, window: Vec::with_capacity(size), size }
    }
}

impl<S: Source> Iterator for Meter<S> {
    type Item = S::Item;
    fn next(&mut self) -> Option<Self::Item> {
        let s = self.inner.next()?;
        self.window.push(s);
        if self.window.len() >= self.size {
            if let Some(link) = voice_link::get() {
                link.set_level(voice_link::loudness(&self.window));
            }
            self.window.clear();
        }
        Some(s)
    }
}

impl<S: Source> Source for Meter<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// Read over the chunks the network thread sends. Seek only reports the
/// position: the decoder is told the stream is not seekable.
struct ChunkReader {
    // Mutex only to make it Sync, which the decoder requires.
    rx: std::sync::Mutex<Receiver<Vec<u8>>>,
    buf: Vec<u8>,
    pos: usize,
    total: u64,
}

impl Read for ChunkReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.pos >= self.buf.len() {
            match self.rx.lock().unwrap().recv() {
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        self.total += n as u64;
        Ok(n)
    }
}

impl Seek for ChunkReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.total),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "stream")),
        }
    }
}
