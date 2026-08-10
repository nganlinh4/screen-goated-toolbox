#[cfg(feature = "recorder-worker")]
use std::mem::ManuallyDrop;
use std::sync::OnceLock;

use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFStartup};
#[cfg(feature = "recorder-worker")]
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
#[cfg(feature = "recorder-worker")]
use windows::Win32::System::Variant::VT_I8;

static MF_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn mf_startup() -> Result<(), String> {
    MF_INIT
        .get_or_init(|| unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|error| format!("MFStartup: {error}"))
        })
        .clone()
}

#[cfg(feature = "recorder-worker")]
pub(super) fn make_i64_propvariant(value: i64) -> PROPVARIANT {
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_I8,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { hVal: value },
            }),
        },
    }
}
