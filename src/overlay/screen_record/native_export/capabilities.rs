pub fn get_export_capabilities() -> serde_json::Value {
    let dx12_ok = probe_dx12();
    let mf_hw = probe_mf_h264_hardware();
    serde_json::json!({
        // pipeline degrades to cpu_fallback when DX12/wgpu cannot initialise;
        // the wgpu compositor has no CPU fallback so the export would fail anyway.
        "pipeline": if dx12_ok { "zero_copy_gpu" } else { "cpu_fallback" },
        "mf_h264": true,       // MF H.264 software encoder ships with every Win10/11
        "mf_h264_hw": mf_hw,   // hardware-accelerated path (Intel QSV / AMD VCE / NVENC via MF)
    })
}

/// Probe whether a D3D12 device can be created (minimum feature level 11.0).
/// Creating and immediately dropping the device is the only reliable check.
fn probe_dx12() -> bool {
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
    use windows::Win32::Graphics::Direct3D12::{D3D12CreateDevice, ID3D12Device};
    let mut device: Option<ID3D12Device> = None;
    unsafe { D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device) }.is_ok()
}

/// Probe whether a hardware H.264 MFT encoder is registered on this machine.
/// Uses MFTEnumEx (enumerate-only, no instantiation) so it is cheap (<1 ms).
fn probe_mf_h264_hardware() -> bool {
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, MFMediaType_Video, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE,
        MFT_ENUM_FLAG_SORTANDFILTER, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
        MFVideoFormat_NV12,
    };
    use windows::Win32::System::Com::CoTaskMemFree;

    let input_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let output_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;
    let ok = unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input_info),
            Some(&output_info),
            &mut activates,
            &mut count,
        )
        .is_ok()
    };
    // Release the returned IMFActivate array (CoTaskMemAlloc'd by MF).
    if !activates.is_null() {
        for i in 0..count as usize {
            unsafe { (*activates.add(i)).take() };
        }
        unsafe { CoTaskMemFree(Some(activates as *const _)) };
    }
    ok && count > 0
}
