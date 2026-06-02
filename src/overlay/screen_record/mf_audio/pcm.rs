pub(super) fn pcm_integer_bytes_to_f32le_bytes(bytes: &[u8], bits_per_sample: u32) -> Vec<u8> {
    match bits_per_sample {
        8 => pcm8_bytes_to_f32le_bytes(bytes),
        16 => pcm16_bytes_to_f32le_bytes(bytes),
        24 => pcm24_bytes_to_f32le_bytes(bytes),
        32 => pcm32_bytes_to_f32le_bytes(bytes),
        _ => Vec::new(),
    }
}

fn pcm8_bytes_to_f32le_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut floats = Vec::with_capacity(bytes.len() * 4);
    for byte in bytes {
        let centered = (*byte as f32 - 128.0) / 128.0;
        floats.extend_from_slice(&centered.clamp(-1.0, 1.0).to_le_bytes());
    }
    floats
}

fn pcm16_bytes_to_f32le_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut floats = Vec::with_capacity((bytes.len() / 2) * 4);
    for chunk in bytes.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
        floats.extend_from_slice(&sample.clamp(-1.0, 1.0).to_le_bytes());
    }
    floats
}

fn pcm24_bytes_to_f32le_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut floats = Vec::with_capacity((bytes.len() / 3) * 4);
    for chunk in bytes.chunks_exact(3) {
        let sample =
            ((chunk[2] as i32) << 24 | (chunk[1] as i32) << 16 | (chunk[0] as i32) << 8) >> 8;
        let normalized = sample as f32 / 8_388_608.0;
        floats.extend_from_slice(&normalized.clamp(-1.0, 1.0).to_le_bytes());
    }
    floats
}

fn pcm32_bytes_to_f32le_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut floats = Vec::with_capacity(bytes.len());
    for chunk in bytes.chunks_exact(4) {
        let sample =
            i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f32 / i32::MAX as f32;
        floats.extend_from_slice(&sample.clamp(-1.0, 1.0).to_le_bytes());
    }
    floats
}
