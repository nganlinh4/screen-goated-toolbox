use std::sync::{Arc, LazyLock, Mutex};

#[cfg(test)]
mod tests;

pub(super) static OUTPUT_VOLUME: LazyLock<Arc<VolumeControl>> =
    LazyLock::new(|| Arc::new(VolumeControl::default()));

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VolumeSnapshot {
    pub(super) percent: u32,
    pub(super) restore_percent: u32,
}

pub(super) struct VolumeControl(Mutex<VolumeSnapshot>);

impl Default for VolumeControl {
    fn default() -> Self {
        Self(Mutex::new(VolumeSnapshot {
            percent: 100,
            restore_percent: 100,
        }))
    }
}

impl VolumeControl {
    pub(super) fn percent(&self) -> u32 {
        self.snapshot().percent
    }

    pub(super) fn snapshot(&self) -> VolumeSnapshot {
        *self.0.lock().unwrap()
    }

    pub(super) fn set_percent(&self, percent: i64) {
        let mut state = self.0.lock().unwrap();
        state.percent = percent.clamp(0, 100) as u32;
        state.restore_percent = if state.percent > 0 {
            state.percent
        } else {
            restore_value(state.restore_percent)
        };
    }

    pub(super) fn toggle_mute(&self) {
        let mut state = self.0.lock().unwrap();
        if state.percent == 0 {
            state.percent = restore_value(state.restore_percent);
            state.restore_percent = state.percent;
        } else {
            state.restore_percent = restore_value(state.percent);
            state.percent = 0;
        }
    }

    pub(super) fn apply_pcm(&self, bytes: &mut [u8]) {
        let percent = self.percent() as i32;
        for pair in bytes.as_chunks_mut::<2>().0 {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
            pair.copy_from_slice(&((sample as i32 * percent / 100) as i16).to_le_bytes());
        }
    }
}

fn restore_value(percent: u32) -> u32 {
    if percent == 0 { 100 } else { percent.min(100) }
}
