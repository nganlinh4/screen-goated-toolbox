//! Contains the nvenc version numbers, encoded as they are in C, for the version this crate was built against

pub const NVENC_MAJOR_VERSION: u16 = 13;
pub const NVENC_MINOR_VERSION: u8 = 0;

pub const NVENC_API_VERSION: u32 = NVENC_MAJOR_VERSION as u32 | (NVENC_MINOR_VERSION as u32) << 24;

pub const NVENC_API_LIST_VERSION: u32 = struct_version(2);

pub const fn struct_version(nvenc_api_version: u8) -> u32 {
    NVENC_API_VERSION | (nvenc_api_version as u32) << 16 | 0x7 << 28
}
