use std::{ffi::c_char, mem::MaybeUninit, os::raw::c_void};

use crate::{
    stdcall,
    sys::{
        enums::{NVencBufferFormat, NVencTuningInfo},
        result::NVencResult,
        structs::{
            Guid, NVencCapsParam, NVencCreateBitstreamBuffer, NVencCreateInputBuffer,
            NVencInitializeParams, NVencLockBitStream, NVencLockInputBuffer, NVencMapInputResource,
            NVencOpenEncodeSessionExParams, NVencPicParams, NVencPresetConfig,
            NVencRegisterResource,
        },
    },
};

#[repr(C)]
#[derive(Clone)]
pub struct NVencFunctionList {
    pub version: u32,
    rsvd: u32,
    #[deprecated]
    nvenc_open_encode_session: NvencOpenEncodeSession,
    pub nvenc_get_encoder_guid_count: NvencGetEncodeGuidCount,
    nvenc_get_encoder_profile_guid_count: NvencGetEncodeProfileGuidCount,
    nvenc_get_encoder_profile_guids: NvencGetEncodeProfileGuids,
    pub nvenc_get_encoder_guids: NvencGetEncodeGuids,
    nvenc_get_input_format_count: NvencGetInputFormatCount,
    nvenc_get_input_formats: NvencGetInputFormats,
    nvenc_get_encode_caps: NvencGetEncodeCaps,
    pub nvenc_get_encode_preset_count: NvencGetEncodePresetCount,
    pub nvenc_get_encode_preset_guids: NvencGetEncodePresetGuids,
    nvenc_get_encode_preset_config: NvencGetEncodePresetConfig,
    pub nvenc_initialize_encoder: NvencInitializeEncoder,
    pub nvenc_create_input_buffer: NvencCreateInputBuffer,
    pub nvenc_destroy_input_buffer: NvencDestroyInputBuffer,
    pub nvenc_create_bit_stream_buffer: NvencCreateBitStreamBuffer,
    pub nvenc_destory_bit_stream_buffer: NvencDestroyBitStreamBuffer,
    pub nvenc_encode_picture: NvencEncodePicture,
    pub nvenc_lock_bit_stream: NvencLockBitStream,
    pub nvenc_unlock_bit_stream: NvencUnlockBitStream,
    pub nvenc_lock_input_buffer: NvencLockInputBuffer,
    pub nvenc_unlock_input_buffer: NvencUnlockInputBuffer,
    nvenc_get_encode_stats: NvencGetEncodeStats,
    nvenc_get_sequence_params: NvencGetSequenceParams,
    nvenc_register_async_event: NvencRegisterAsyncEvent,
    nvenc_unregister_async_event: NvencUnregisterAsyncEvent,
    pub nvenc_map_input_resource: NvencMapInputResource,
    pub nvenc_unmap_input_resource: NvencUnmapInputResource,
    pub nvenc_destroy_encoder: NvencDestroyEncoder,
    nvenc_invalidate_ref_frames: NvencInvalidateRefFrames,
    pub nvenc_open_encode_session_ex: NvencOpenEncodeSessionEx,
    pub nvenc_register_resource: NvencRegisterResource,
    pub nvenc_unregister_resource: NvencUnRegisterResource,
    nvenc_reconfigure_encoder: NvencReconfigureEncoder,
    rsvd1: *mut c_void,
    nvenc_create_mv_buffer: NvencCreateMVBuffer,
    nvenc_destory_mv_buffer: NvencDestoryMVBuffer,
    nvenc_run_motion_estimation_only: NvencRunMotionEstimationOnly,
    pub nvenc_get_last_error: NvencGetLastError,
    nvenc_set_io_cuda_streams: NvencSetIOCudaStreams,
    pub nvenc_get_encode_preset_config_ex: NvencGetEncodePresetConfigEx,
    nvenc_get_sequence_params_ex: NvencGetSequenceParamsEx,
    nvenc_store_encoder_state: NvencStoreEncoderState,
    nvenc_look_ahead_picture: NvencLookAheadPicture,
    rsvd2: [*mut c_void; 275],
}

type NvencOpenEncodeSession =
    stdcall!(fn(device: *mut c_void, device_type: i32, encoder: *mut *mut c_void) -> NVencResult);
type NvencGetEncodeGuidCount =
    stdcall!(fn(encoder: *mut c_void, encoder_guid_count: *mut u32) -> NVencResult);
type NvencGetEncodeGuids = stdcall!(
    fn(
        encoder: *mut c_void,
        guids: *mut Guid,
        guid_array_size: u32,
        guid_count: *mut u32,
    ) -> NVencResult
);
type NvencGetEncodeProfileGuidCount = stdcall!(
    fn(
        encoder: *mut c_void,
        encode_guid: Guid,
        encoder_profile_guid_count: *mut u32,
    ) -> NVencResult
);
type NvencGetEncodeProfileGuids = stdcall!(
    fn(
        encoder: *mut c_void,
        profile_guids: *mut Guid,
        guid_array_size: u32,
        guid_count: *mut u32,
    ) -> NVencResult
);
type NvencGetInputFormatCount = stdcall!(
    fn(encoder: *mut c_void, encoder_guid: Guid, input_fmt_count: *mut u32) -> NVencResult
);
type NvencGetInputFormats = stdcall!(
    fn(
        encoder: *mut c_void,
        input_formats: *mut NVencBufferFormat,
        input_format_array_size: u32,
        input_format_count: *mut u32,
    ) -> NVencResult
);
type NvencGetEncodeCaps = stdcall!(
    fn(
        encoder: *mut c_void,
        encoder_guid: Guid,
        caps_param: *mut NVencCapsParam,
        caps_val: *mut i32,
    ) -> NVencResult
);
type NvencGetEncodePresetCount = stdcall!(
    fn(
        encoder: *mut c_void,
        encoder_guid: Guid,
        encoder_preset_guid_count: *mut u32,
    ) -> NVencResult
);
type NvencGetEncodePresetGuids = stdcall!(
    fn(
        encoder: *mut c_void,
        encode_guid: Guid,
        preset_guids: *mut Guid,
        guid_array_size: u32,
        encoder_preset_guid_count: *mut u32,
    ) -> NVencResult
);
type NvencGetEncodePresetConfig = stdcall!(
    fn(
        encoder: *mut c_void,
        encode_guid: Guid,
        preset_guid: Guid,
        preset_config: *mut NVencPresetConfig,
    ) -> NVencResult
);
type NvencGetEncodePresetConfigEx = stdcall!(
    fn(
        encoder: *mut c_void,
        encode_guid: Guid,
        preset_guid: Guid,
        tuning_info: NVencTuningInfo,
        preset_config: *mut MaybeUninit<NVencPresetConfig>,
    ) -> NVencResult
);
type NvencInitializeEncoder = stdcall!(
    fn(encoder: *mut c_void, create_encode_params: *mut NVencInitializeParams) -> NVencResult
);
type NvencCreateInputBuffer = stdcall!(
    fn(
        encoder: *mut c_void,
        create_input_buffer_params: *mut NVencCreateInputBuffer,
    ) -> NVencResult
);
type NvencDestroyInputBuffer =
    stdcall!(fn(encoder: *mut c_void, input_buffer: *mut c_void) -> NVencResult);
type NvencCreateBitStreamBuffer = stdcall!(
    fn(
        encoder: *mut c_void,
        create_bit_stream_buffer_params: *mut NVencCreateBitstreamBuffer,
    ) -> NVencResult
);
type NvencDestroyBitStreamBuffer =
    stdcall!(fn(encoder: *mut c_void, bitstream_buffer: *mut c_void) -> NVencResult);
type NvencEncodePicture = stdcall!(
    fn(encoder: *mut c_void, encode_pic_params: *mut MaybeUninit<NVencPicParams>) -> NVencResult
);
type NvencLockBitStream = stdcall!(
    fn(
        encoder: *mut c_void,
        lock_bitstream_buffer_params: *mut MaybeUninit<NVencLockBitStream>,
    ) -> NVencResult
);
type NvencUnlockBitStream =
    stdcall!(fn(encoder: *mut c_void, bitstream_buffer: *mut c_void) -> NVencResult);
type NvencLockInputBuffer = stdcall!(
    fn(encoder: *mut c_void, lock_input_buffer_params: *mut NVencLockInputBuffer) -> NVencResult
);
type NvencUnlockInputBuffer =
    stdcall!(fn(encoder: *mut c_void, input_buffer: *mut c_void) -> NVencResult);
type NvencGetEncodeStats = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencGetSequenceParams = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencRegisterAsyncEvent = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencUnregisterAsyncEvent = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencMapInputResource = stdcall!(
    fn(encoder: *mut c_void, map_input_resource_params: *mut NVencMapInputResource) -> NVencResult
);
type NvencUnmapInputResource =
    stdcall!(fn(encoder: *mut c_void, mapped_input_buffer: *mut c_void) -> NVencResult);
type NvencDestroyEncoder = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencInvalidateRefFrames = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencOpenEncodeSessionEx = stdcall!(
    fn(
        open_session_ex_params: *mut NVencOpenEncodeSessionExParams,
        encoder: *mut *mut c_void,
    ) -> NVencResult
);
type NvencRegisterResource = stdcall!(
    fn(encoder: *mut c_void, register_res_params: *mut NVencRegisterResource) -> NVencResult
);
type NvencUnRegisterResource =
    stdcall!(fn(encoder: *mut c_void, registered_res: *mut c_void) -> NVencResult);
type NvencReconfigureEncoder = stdcall!(fn(encoder: *mut c_void) -> NVencResult);

type NvencCreateMVBuffer = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencDestoryMVBuffer = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencRunMotionEstimationOnly = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencGetLastError = stdcall!(fn(encoder: *mut c_void) -> *const c_char);
type NvencSetIOCudaStreams = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencGetSequenceParamsEx = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencStoreEncoderState = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
type NvencLookAheadPicture = stdcall!(fn(encoder: *mut c_void) -> NVencResult);
