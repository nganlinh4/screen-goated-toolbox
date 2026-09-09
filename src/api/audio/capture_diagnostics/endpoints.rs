//! Read-only endpoint inventory, default roles, mute and level metadata.

use cpal::traits::{DeviceTrait, HostTrait};
use serde_json::{Value, json};
use windows::Win32::Media::Audio::{
    Endpoints::{IAudioEndpointVolume, IAudioMeterInformation},
    IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications, eConsole, eMultimedia,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::PCWSTR;

pub(super) fn snapshot(selected: Option<&str>, inventory: bool) -> String {
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED);
        if let Err(error) = com.ok() {
            return json!({"com_error": format!("{error:?}")}).to_string();
        }
        let result = snapshot_inner(selected, inventory);
        CoUninitialize();
        result
            .unwrap_or_else(|error| json!({"error": format!("{error:#}")}))
            .to_string()
    }
}

unsafe fn snapshot_inner(selected: Option<&str>, inventory: bool) -> anyhow::Result<Value> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let mut defaults = serde_json::Map::new();
        for (name, role) in [
            ("console", eConsole),
            ("multimedia", eMultimedia),
            ("communications", eCommunications),
        ] {
            let value = match enumerator.GetDefaultAudioEndpoint(eCapture, role) {
                Ok(device) => {
                    let pointer = device.GetId()?;
                    let id = pointer.to_string();
                    CoTaskMemFree(Some(pointer.0.cast()));
                    json!({"id": id.ok()})
                }
                Err(error) => json!({"error": format!("{error:?}")}),
            };
            defaults.insert(name.into(), value);
        }
        let selected_state = selected.map(|id| -> anyhow::Result<Value> {
            let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
            let device = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            let meter = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None)
                .and_then(|meter| meter.GetPeakValue());
            Ok(json!({"id": id, "state": device.GetState()?.0,
                "os_peak": meter.as_ref().ok(), "os_peak_error": meter.as_ref().err().map(|error| format!("{error:?}")),
                "muted": volume.GetMute()?.as_bool(), "volume": volume.GetMasterVolumeLevelScalar()?}))
        }).map(|result| result.unwrap_or_else(|error| json!({"error": format!("{error:#}")})));
        let mut value = json!({"defaults": defaults, "selected": selected_state});
        if inventory {
            value["inputs"] = match cpal::default_host().input_devices() {
                Ok(devices) => Value::Array(
                    devices
                        .take(32)
                        .map(|device| {
                            json!({
                                "id": device.id().ok().map(|id| id.id().to_string()),
                                "name": device.description().ok().map(|d| d.name().to_string()),
                                "config": format!("{:?}", device.default_input_config()),
                            })
                        })
                        .collect(),
                ),
                Err(error) => json!({"error": format!("{error:?}")}),
            };
        }
        Ok(value)
    }
}
