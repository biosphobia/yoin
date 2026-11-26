use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    /// Stable id to pass back to [`crate::Engine::set_devices`].
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceList {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
}

pub(crate) fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| device.to_string())
}

fn device_id(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.to_string())
}

/// All input and output devices on the default host (WASAPI on Windows).
pub fn list_devices() -> DeviceList {
    let host = cpal::default_host();
    let default_in = host.default_input_device().and_then(|d| device_id(&d));
    let default_out = host.default_output_device().and_then(|d| device_id(&d));

    let describe = |device: cpal::Device, default: &Option<String>| {
        let id = device_id(&device)?;
        Some(DeviceInfo {
            is_default: default.as_deref() == Some(id.as_str()),
            name: device_name(&device),
            id,
        })
    };

    DeviceList {
        inputs: host
            .input_devices()
            .map(|it| it.filter_map(|d| describe(d, &default_in)).collect())
            .unwrap_or_default(),
        outputs: host
            .output_devices()
            .map(|it| it.filter_map(|d| describe(d, &default_out)).collect())
            .unwrap_or_default(),
    }
}

/// Finds a device by id, falling back to the system default if the id is
/// missing or no longer present (e.g. headphones unplugged).
pub(crate) fn find_device(
    host: &cpal::Host,
    id: Option<&str>,
    input: bool,
) -> Option<cpal::Device> {
    if let Some(id) = id.and_then(|s| s.parse::<cpal::DeviceId>().ok()) {
        if let Some(d) = host.device_by_id(&id) {
            return Some(d);
        }
    }
    if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
}
