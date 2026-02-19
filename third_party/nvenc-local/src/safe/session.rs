use std::{marker::PhantomData, sync::Arc};
use std::ffi::c_void;

use crate::{
    encoder::{Encoder, EncoderInternal},
    sys::{
        enums::{NVencBufferFormat, NVencDeviceType, NVencTuningInfo},
        result::NVencError,
        structs::{
            Guid, NV_ENC_INITIALIZE_PARAMS_VER, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
            NVencConfig, NVencInitializeParams, NVencOpenEncodeSessionExParams, NVencPresetConfig,
        },
        version::NVENC_API_VERSION,
    },
};

pub struct Session<T> {
    encoder: Arc<EncoderInternal>,
    p: PhantomData<T>,
}

pub struct NeedsConfig;

pub struct NeedsInit;

#[cfg(windows)]
use windows::core::Interface;

impl Session<NeedsConfig> {
    /// # Safety
    /// `device` must be a valid DirectX device pointer compatible with NVENC.
    pub unsafe fn open_dx_raw(device: *mut c_void) -> Result<Self, NVencError> {
        let lib = crate::nvenc_init().map_err(|_| NVencError::NoDevice)?;
        let function_list = lib.create_instance()?;
        let mut session_params: NVencOpenEncodeSessionExParams = unsafe { std::mem::zeroed() };
        session_params.version = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER;
        session_params.device_type = NVencDeviceType::DirectX;
        session_params.api_version = NVENC_API_VERSION;
        session_params.device = device;
        let mut encoder = std::ptr::null_mut();
        unsafe {
            (function_list.nvenc_open_encode_session_ex)(&raw mut session_params, &raw mut encoder)
                .into_error()
        }?;
        if encoder.is_null() {
            return Err(NVencError::InvalidDevice);
        }
        let encoder = EncoderInternal {
            encoder: unsafe { std::ptr::NonNull::new_unchecked(encoder) },
            function_list,
        };
        Ok(Self {
            encoder: Arc::new(encoder),
            p: PhantomData,
        })
    }

    #[cfg(windows)]
    pub fn open_dx(device: &impl Interface) -> Result<Self, NVencError> {
        unsafe { Self::open_dx_raw(device.as_raw()) }
    }

    pub fn open_gl() -> Result<Self, NVencError> {
        let lib = crate::nvenc_init().map_err(|_| NVencError::NoDevice)?;
        let function_list = lib.create_instance()?;
        let mut session_params: NVencOpenEncodeSessionExParams = unsafe { std::mem::zeroed() };
        session_params.version = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER;
        session_params.device_type = NVencDeviceType::OpenGL;
        session_params.api_version = NVENC_API_VERSION;
        session_params.device = std::ptr::null_mut();

        let mut encoder = std::ptr::null_mut();
        unsafe {
            (function_list.nvenc_open_encode_session_ex)(&raw mut session_params, &raw mut encoder)
                .into_error()
        }?;
        if encoder.is_null() {
            return Err(NVencError::InvalidDevice);
        }
        let encoder = EncoderInternal {
            encoder: unsafe { std::ptr::NonNull::new_unchecked(encoder) },
            function_list,
        };
        Ok(Self {
            encoder: Arc::new(encoder),
            p: PhantomData,
        })
    }

    pub fn get_encode_codecs(&self) -> Result<Box<[Guid]>, NVencError> {
        let mut count = 0;
        unsafe {
            (self.encoder.function_list.nvenc_get_encoder_guid_count)(
                self.encoder.encoder.as_ptr(),
                &raw mut count,
            )
            .into_error()
        }?;
        let mut array = vec![Guid::default(); count as usize];
        unsafe {
            (self.encoder.function_list.nvenc_get_encoder_guids)(
                self.encoder.encoder.as_ptr(),
                array.as_mut_ptr(),
                count,
                &raw mut count,
            )
        }
        .into_error()?;
        array.truncate(count as usize);
        Ok(array.into())
    }

    pub fn get_encode_presets(&self, codec: Guid) -> Result<Box<[Guid]>, NVencError> {
        let mut count = 0;
        unsafe {
            (self.encoder.function_list.nvenc_get_encode_preset_count)(
                self.encoder.encoder.as_ptr(),
                codec.clone(),
                &raw mut count,
            )
            .into_error()
        }?;
        let mut array = vec![Guid::default(); count as usize];
        unsafe {
            (self.encoder.function_list.nvenc_get_encode_preset_guids)(
                self.encoder.encoder.as_ptr(),
                codec,
                array.as_mut_ptr(),
                count,
                &raw mut count,
            )
        }
        .into_error()?;
        array.truncate(count as usize);
        Ok(array.into())
    }

    pub fn get_encode_preset_config_ex(
        self,
        codec: Guid,
        preset: Guid,
        tuning_info: NVencTuningInfo,
    ) -> Result<(Session<NeedsInit>, NVencPresetConfig), NVencError> {
        self.encoder
            .get_encode_preset_config_ex(codec, preset, tuning_info)
            .map(|config| {
                (
                    Session {
                        encoder: self.encoder,
                        p: PhantomData,
                    },
                    config,
                )
            })
    }
}

pub struct InitParams<'a> {
    pub encode_guid: Guid,
    pub preset_guid: Guid,
    pub resolution: [u32; 2],
    pub aspect_ratio: [u32; 2],
    pub frame_rate: [u32; 2],
    pub tuning_info: NVencTuningInfo,
    pub buffer_format: NVencBufferFormat,
    pub encode_config: &'a mut NVencConfig,
    pub enable_ptd: bool,
    pub enable_output_in_vidmem: bool,
    pub max_encoder_resolution: [u32; 2],
    // TODO: Support for async encoding and bit fields
}

impl Session<NeedsInit> {
    pub fn init_encoder(self, init_params: InitParams) -> Result<Encoder, NVencError> {
        let mut bit_flags = crate::sys::structs::NVencInitializeParamsBitfields::new();
        if init_params.enable_output_in_vidmem {
            // Output-in-vidmem is not compatible with split-frame encoding on some drivers.
            // Force split encode off for this path.
            bit_flags.set_split_encode_mode(15);
        }
        bit_flags.set_enable_output_in_vid_mem(init_params.enable_output_in_vidmem);
        let bit_flags_raw: u32 = unsafe { std::mem::transmute_copy(&bit_flags) };
        println!(
            "[Export][SDK] init flags: output_in_vidmem={} split_encode_mode={} raw=0x{:08x}",
            init_params.enable_output_in_vidmem,
            bit_flags.split_encode_mode(),
            bit_flags_raw
        );

        // This sucks, lol
        let raw_init_params = NVencInitializeParams {
            encode_guid: init_params.encode_guid,
            preset_guid: init_params.preset_guid,
            encode_width: init_params.resolution[0],
            encode_height: init_params.resolution[1],
            dar_width: init_params.aspect_ratio[0],
            dar_height: init_params.aspect_ratio[1],
            frame_rate_num: init_params.frame_rate[0],
            frame_rate_den: init_params.frame_rate[1],
            tuning_info: init_params.tuning_info,
            buffer_format: init_params.buffer_format,
            encode_config: init_params.encode_config as _,
            enable_ptd: init_params.enable_ptd as u32,
            version: NV_ENC_INITIALIZE_PARAMS_VER,
            enable_encode_async: 0,
            bit_flags,
            priv_data_size: 0,
            rsvd: 0,
            priv_data: std::ptr::null_mut(),
            max_encode_width: init_params.max_encoder_resolution[0],
            max_encode_height: init_params.max_encoder_resolution[1],
            max_me_hint_counts_per_block: [Default::default(), Default::default()],
            num_state_buffers: 0,
            output_stats_level: crate::sys::enums::NVencOutputStatsLevel::None,
            rsvd1: unsafe { std::mem::zeroed() },
            rsvd2: unsafe { std::mem::zeroed() },
        };

        self.encoder.init_encoder(raw_init_params)?;
        Ok(Encoder {
            encoder: self.encoder,
            marker: PhantomData,
        })
    }
}
