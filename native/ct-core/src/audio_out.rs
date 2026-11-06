//! The speakers claudeTalk plays on: the device picked in the gear panel
//! (`output_device`, its name), or the system default when that is empty
//! or the device is gone.

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{DeviceSinkBuilder, MixerDeviceSink};

/// Names of the output devices, as the panel lists them.
pub fn output_names() -> Vec<String> {
    match rodio::cpal::default_host().output_devices() {
        Ok(list) => list.filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect(),
        Err(_) => Vec::new(),
    }
}

/// Opens `name` (a name from output_names), else the default device.
pub fn open_sink(name: &str) -> Result<MixerDeviceSink, rodio::DeviceSinkError> {
    if !name.is_empty() {
        let found = rodio::cpal::default_host()
            .output_devices()
            .ok()
            .and_then(|mut list| list.find(|d| d.description().is_ok_and(|x| x.name() == name)));
        if let Some(Ok(sink)) = found.map(|d| DeviceSinkBuilder::from_device(d).and_then(|b| b.open_stream())) {
            return Ok(sink);
        }
    }
    DeviceSinkBuilder::open_default_sink()
}
