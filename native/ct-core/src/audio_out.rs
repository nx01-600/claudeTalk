//! The speakers claudeTalk plays on: the device picked in the gear panel
//! (`output_device`, its name), or the system default when that is empty
//! or the device is gone.

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{DeviceSinkBuilder, MixerDeviceSink};

/// The device's Windows name, e.g. "Microphone Array (Intel® Smart Sound
/// ...)". cpal 0.17 (rodio's) puts it in the description's extended lines
/// and keeps only "Microphone Array" as the name; the daemon's cpal 0.18
/// names devices by it, so both sides must use this one.
fn friendly_name(d: &rodio::cpal::Device) -> Option<String> {
    let desc = d.description().ok()?;
    Some(desc.extended().first().cloned().unwrap_or_else(|| desc.name().to_string()))
}

/// Names of the output devices, as the panel lists them.
pub fn output_names() -> Vec<String> {
    match rodio::cpal::default_host().output_devices() {
        Ok(list) => list.filter_map(|d| friendly_name(&d)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Names of the microphones (for `/talk set mic`; the daemon lists its own
/// with the same WASAPI names).
pub fn input_names() -> Vec<String> {
    match rodio::cpal::default_host().input_devices() {
        Ok(list) => list.filter_map(|d| friendly_name(&d)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Opens `name` (a name from output_names), else the default device.
pub fn open_sink(name: &str) -> Result<MixerDeviceSink, rodio::DeviceSinkError> {
    if !name.is_empty() {
        let found = rodio::cpal::default_host()
            .output_devices()
            .ok()
            .and_then(|mut list| list.find(|d| friendly_name(d).as_deref() == Some(name)));
        if let Some(Ok(sink)) = found.map(|d| DeviceSinkBuilder::from_device(d).and_then(|b| b.open_stream())) {
            return Ok(sink);
        }
    }
    DeviceSinkBuilder::open_default_sink()
}

