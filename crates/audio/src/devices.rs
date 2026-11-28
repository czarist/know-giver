//! Which microphone and which speakers the app uses.
//!
//! The choice is process-wide: recordings, speech playback and interface
//! sounds read it when they open a device and follow a change while they
//! run, so the student can switch to a headset in the middle of a lesson.
//! Devices are known by name; a name that is no longer connected falls back
//! to the system default.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait};

use crate::AudioError;

struct Choice {
    name: RwLock<Option<String>>,
    /// Bumped on every change, so open streams notice it cheaply.
    generation: AtomicU64,
}

impl Choice {
    const fn new() -> Self {
        Self {
            name: RwLock::new(None),
            generation: AtomicU64::new(0),
        }
    }

    fn set(&self, name: Option<String>) {
        let name = name.filter(|n| !n.trim().is_empty());
        if let Ok(mut current) = self.name.write()
            && *current != name
        {
            *current = name;
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn get(&self) -> Option<String> {
        self.name.read().ok().and_then(|n| n.clone())
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

static INPUT: Choice = Choice::new();
static OUTPUT: Choice = Choice::new();

/// Uses the microphone called `name`; `None` follows the system default.
pub fn set_input_device(name: Option<String>) {
    INPUT.set(name);
}

/// Plays through the device called `name`; `None` follows the system default.
pub fn set_output_device(name: Option<String>) {
    OUTPUT.set(name);
}

/// Names of the connected microphones.
pub fn input_devices() -> Vec<String> {
    names(cpal::default_host().input_devices().ok())
}

/// Names of the connected speakers and headphones.
pub fn output_devices() -> Vec<String> {
    names(cpal::default_host().output_devices().ok())
}

fn names(devices: Option<impl Iterator<Item = cpal::Device>>) -> Vec<String> {
    let mut names: Vec<String> = devices
        .into_iter()
        .flatten()
        .filter_map(|d| device_name(&d))
        .collect();
    names.dedup();
    names
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|d| d.name().to_string())
        .filter(|n| !n.is_empty())
}

pub(crate) fn input_generation() -> u64 {
    INPUT.generation()
}

pub(crate) fn output_generation() -> u64 {
    OUTPUT.generation()
}

/// The chosen microphone, or the default one when it is not connected.
pub(crate) fn input_device() -> Result<cpal::Device, AudioError> {
    let host = cpal::default_host();
    if let Some(wanted) = INPUT.get() {
        let found = host
            .input_devices()
            .ok()
            .into_iter()
            .flatten()
            .find(|d| device_name(d).as_deref() == Some(wanted.as_str()));
        match found {
            Some(device) => return Ok(device),
            None => tracing::warn!(device = %wanted, "microphone not found, using the default"),
        }
    }
    host.default_input_device().ok_or(AudioError::NoInputDevice)
}

/// Opens the chosen output, or the default one when it is not connected or
/// cannot be opened.
pub(crate) fn open_output() -> Result<rodio::MixerDeviceSink, AudioError> {
    if let Some(wanted) = OUTPUT.get() {
        let found = cpal::default_host()
            .output_devices()
            .ok()
            .into_iter()
            .flatten()
            .find(|d| device_name(d).as_deref() == Some(wanted.as_str()));
        match found.map(|device| {
            rodio::DeviceSinkBuilder::from_device(device).and_then(|b| b.open_sink_or_fallback())
        }) {
            Some(Ok(sink)) => return Ok(sink),
            Some(Err(err)) => {
                tracing::warn!(device = %wanted, %err, "output failed, using the default");
            }
            None => tracing::warn!(device = %wanted, "output not found, using the default"),
        }
    }
    rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|err| AudioError::NoOutputDevice(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::Choice;

    #[test]
    fn a_change_bumps_the_generation_and_blank_means_default() {
        let choice = Choice::new();
        choice.set(Some("Headset".into()));
        assert_eq!(choice.get().as_deref(), Some("Headset"));
        assert_eq!(choice.generation(), 1);
        choice.set(Some("Headset".into()));
        assert_eq!(choice.generation(), 1, "same device is not a change");
        choice.set(Some("  ".into()));
        assert_eq!(choice.get(), None);
        assert_eq!(choice.generation(), 2);
    }
}
