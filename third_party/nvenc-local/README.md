### If you like my work and want to support what I do, support me on Ko-Fi 💜!
[![Ko-Fi](https://img.shields.io/badge/Ko--fi-F16061?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/cyberite)
---

![Crates.io MSRV](https://img.shields.io/crates/msrv/nvenc?style=for-the-badge) ![Crates.io License](https://img.shields.io/crates/l/nvenc?style=for-the-badge) ![GitHub Repo stars](https://img.shields.io/github/stars/AlsoSylv/nvenc?style=for-the-badge)

The following example is for Linux GLX

```rust
use nvenc::{session::Session, encoder::Encoder};

fn main() {
    // Setup GLX context
    let session: Session<NeedsConfig> = Session::open_gl();
    let (session, config): (Session<NeedsInit>, NVencPresetConfig) 
        = session.get_preset_config(
            NV_ENC_CODEC_H264_GUID, 
            NV_ENC_PRESET_P3_GUID, 
            NVencTuningInfo::LowLatency
        );
    let init_params = nvenc::session::InitParams {
        encode_guid: NV_ENC_H264_GUID,
        preset_guid: NV_ENC_PRESET_P3_GUID,
        resolution: [1920, 1080],
        aspect_ratio: [16, 9],
        frame_rate: [30, 1],
        tuning_info: NVencTuningInfo::LowLatency,
        buffer_format: NVencBufferFormat::ARGB,
        encode_config: &mut config.preset_cfg,
        enable_ptd: true,
    };
    let encoder: Encoder = session.init(init_params);
}
```