use std::ffi::{CStr, c_void};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::{mem::MaybeUninit, ptr::NonNull};

#[cfg(windows)]
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

use crate::input_buffer::InputBuffer;
use crate::safe::bitstream::BitStream;
use crate::sys::enums::{
    NVencBufferFormat, NVencBufferUsage, NVencInputResourceType, NVencMemoryHeap, NVencPicStruct,
    NVencPicType,
};
use crate::sys::structs::{
    Guid, NV_ENC_CREATE_INPUT_BUFFER_VER, NV_ENC_LOCK_INPUT_BUFFER_VER, NVencCodecPicParams,
    NVencCreateInputBuffer, NVencFencePointD3D12, NVencLockInputBuffer, NVencPicParamsH264,
    NVencRegisterResource,
};
use crate::sys::{
    enums::{NVencPicFlags, NVencTuningInfo},
    function_table::NVencFunctionList,
    result::NVencError,
    structs::{
        NV_ENC_CONFIG_VER, NV_ENC_CREATE_BITSTREAM_BUFFER_VER, NV_ENC_LOCK_BITSTREAM_VER,
        NV_ENC_MAP_INPUT_RESOURCE_VER, NV_ENC_PIC_PARAMS_VER, NV_ENC_PRESET_CONFIG_VER,
        NV_ENC_REGISTER_RESOURCE_VER, NVencCreateBitstreamBuffer, NVencInitializeParams,
        NVencLockBitStream, NVencPicParams, NVencPresetConfig,
    },
};

pub struct Encoder {
    pub(crate) encoder: Arc<EncoderInternal>,
    pub(crate) marker: PhantomData<*mut ()>,
}

pub trait EncoderInput {
    #[doc(hidden)]
    fn as_ptr(&self) -> *mut c_void;

    fn pitch(&self) -> u32;
    fn height(&self) -> u32;
    fn width(&self) -> u32;
}

impl EncoderInput for RegisteredResource {
    fn as_ptr(&self) -> *mut c_void {
        self.mapped
    }

    fn pitch(&self) -> u32 {
        self.pitch
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn width(&self) -> u32 {
        self.width
    }
}

impl EncoderInput for InputBuffer {
    fn as_ptr(&self) -> *mut c_void {
        self.buffer
    }

    fn pitch(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn width(&self) -> u32 {
        self.width
    }
}

impl Encoder {
    #[cfg(windows)]
    /// Registers a DX11 texture as an nvenc input resource
    pub fn register_resource_dx11(
        &self,
        resource: &ID3D11Texture2D,
        format: NVencBufferFormat,
        pitch: u32,
    ) -> Result<RegisteredResource, NVencError> {
        self.encoder.register_resource_dx(resource, format, pitch)
    }

    /// # Safety
    /// `resource` must not be null
    /// `ty` and `format` should match the original buffer
    pub unsafe fn register_resource_raw(
        &self,
        resource: *mut c_void,
        format: NVencBufferFormat,
        usage: NVencBufferUsage,
        ty: NVencInputResourceType,
        resolution: [u32; 2],
        pitch: u32,
    ) -> Result<*mut c_void, NVencError> {
        unsafe {
            self.encoder.register_resource_raw_with_fence(
                resource,
                format,
                usage,
                ty,
                resolution,
                pitch,
                None,
            )
        }
    }

    /// # Safety
    /// `resource` and optional `input_fence_point` must remain valid for the call.
    pub unsafe fn register_resource_raw_with_fence(
        &self,
        resource: *mut c_void,
        format: NVencBufferFormat,
        usage: NVencBufferUsage,
        ty: NVencInputResourceType,
        resolution: [u32; 2],
        pitch: u32,
        input_fence_point: Option<*mut NVencFencePointD3D12>,
    ) -> Result<*mut c_void, NVencError> {
        unsafe {
            self.encoder.register_resource_raw_with_fence(
                resource,
                format,
                usage,
                ty,
                resolution,
                pitch,
                input_fence_point,
            )
        }
    }

    /// # Safety
    /// `resource` must point to a valid backend texture/resource that matches
    /// `ty`, `format`, and `resolution` for the active NVENC session device.
    pub unsafe fn register_resource_raw_mapped(
        &self,
        resource: *mut c_void,
        format: NVencBufferFormat,
        usage: NVencBufferUsage,
        ty: NVencInputResourceType,
        resolution: [u32; 2],
        pitch: u32,
    ) -> Result<RegisteredResource, NVencError> {
        let register_format = match ty {
            // NVENC expects DX resources to register as Undefined, then map with the real format.
            NVencInputResourceType::DirectX => NVencBufferFormat::Undefined,
            _ => format,
        };
        let registered = unsafe {
            self.encoder
                .register_resource_raw(resource, register_format, usage, ty, resolution, pitch)?
        };
        let mapped = unsafe { self.encoder.map_input_resource_raw(registered, format)? };
        Ok(RegisteredResource {
            registered,
            mapped,
            pitch,
            width: resolution[0],
            height: resolution[1],
            encoder: self.encoder.clone(),
        })
    }

    /// Takes an encoder inout, and outputs a bit stream that can be read back
    pub fn encode_picture<I: EncoderInput>(
        &self,
        input: &I,
        output: &BitStream,
        frame_count: usize,
        timestamp: u64,
        format: NVencBufferFormat,
        pic_struct: NVencPicStruct,
        ty: NVencPicType,
        codec_params: Option<CodecPicParams>,
    ) -> Result<(), NVencError> {
        self.encoder.encode_picture_raw(
            input.as_ptr(),
            input.width(),
            input.height(),
            input.pitch(),
            output.buffer,
            frame_count,
            timestamp,
            format,
            pic_struct,
            ty,
            codec_params,
        )
    }

    /// Ends the encoder session
    pub fn end_encode(&self) -> Result<(), NVencError> {
        self.encoder.end_encode()
    }

    /// Creates a bitstream bfufer that can be used for output storage
    pub fn create_bitstream_buffer(&self) -> Result<BitStream, NVencError> {
        self.encoder.create_bitstream_buffer()
    }

    pub fn encode_picture_raw(
        &self,
        input_buffer: *mut c_void,
        input_width: u32,
        input_height: u32,
        input_pitch: u32,
        output_bitstream: *mut c_void,
        frame_count: usize,
        timestamp: u64,
        format: NVencBufferFormat,
        pic_struct: NVencPicStruct,
        ty: NVencPicType,
        codec_params: Option<CodecPicParams>,
    ) -> Result<(), NVencError> {
        self.encoder.encode_picture_raw(input_buffer, input_width, input_height, input_pitch, output_bitstream, frame_count, timestamp, format, pic_struct, ty, codec_params)
    }

    pub fn lock_bitstream_raw(&self, output_bitstream: *mut c_void, wait: bool) -> Result<RawBitStreamLockGuard, NVencError> {
        let lock = self.encoder.lock_bit_stream_buffer(output_bitstream, wait)?;
        Ok(RawBitStreamLockGuard {
            encoder: self.encoder.clone(),
            output_ptr: output_bitstream,
            data_ptr: lock.bitstream_buffer,
            data_len: lock.bitstream_size_in_bytes,
        })
    }

    pub unsafe fn map_input_resource_raw(&self, registered_resource: *mut c_void, format: NVencBufferFormat) -> Result<*mut c_void, NVencError> {
        unsafe { self.encoder.map_input_resource_raw(registered_resource, format) }
    }

    pub unsafe fn unmap_input_resource_raw(&self, mapped_resource: *mut c_void) -> Result<(), NVencError> {
        unsafe { self.encoder.unmap_input_resource(mapped_resource) }
    }

    pub unsafe fn unregister_resource_raw(&self, registered_resource: *mut c_void) -> Result<(), NVencError> {
        unsafe { self.encoder.unregister_resource(registered_resource) }
    }

    pub fn create_input_buffer(
        &self,
        width: u32,
        height: u32,
        memory_heap: NVencMemoryHeap,
        format: NVencBufferFormat,
    ) -> Result<InputBuffer, NVencError> {
        Ok(InputBuffer {
            encoder: self.encoder.clone(),
            buffer: self
                .encoder
                .create_input_buffer(width, height, memory_heap, format)?,
            width,
            height,
        })
    }
}

pub(crate) struct EncoderInternal {
    pub(super) encoder: NonNull<c_void>,
    pub(super) function_list: NVencFunctionList,
}

/// The Nvidia encoder is technically thread-safe
unsafe impl Send for EncoderInternal {}

/// The Nvidia encoder handles syncing via `lock_bit_stream` which is all that is allowed to be Send
unsafe impl Sync for EncoderInternal {}

impl Drop for EncoderInternal {
    fn drop(&mut self) {
        println!("Dropping encoder, this should happen last");
        unsafe { (self.function_list.nvenc_destroy_encoder)(self.encoder.as_ptr()) };
    }
}

impl EncoderInternal {
    pub(crate) fn get_encode_preset_config_ex(
        &self,
        codec: Guid,
        preset: Guid,
        tuning_info: NVencTuningInfo,
    ) -> Result<NVencPresetConfig, NVencError> {
        let mut config: std::mem::MaybeUninit<NVencPresetConfig> = std::mem::MaybeUninit::zeroed();
        unsafe { &mut *config.as_mut_ptr() }.version = NV_ENC_PRESET_CONFIG_VER;
        unsafe { &mut *config.as_mut_ptr() }.preset_cfg.version = NV_ENC_CONFIG_VER;
        unsafe {
            (self.function_list.nvenc_get_encode_preset_config_ex)(
                self.encoder.as_ptr(),
                codec,
                preset,
                tuning_info,
                &raw mut config,
            )
            .into_error()?
        };

        Ok(unsafe { config.assume_init() })
    }

    pub(crate) fn init_encoder(
        &self,
        mut init_params: NVencInitializeParams,
    ) -> Result<(), NVencError> {
        match unsafe {
            (self.function_list.nvenc_initialize_encoder)(
                self.encoder.as_ptr(),
                &raw mut init_params,
            )
        }
        .into_error()
        {
            Err(err) => {
                let ptr = unsafe { (self.function_list.nvenc_get_last_error)(self.encoder.as_ptr()) };
                if !ptr.is_null() {
                    println!("{:?}", unsafe { CStr::from_ptr(ptr) });
                }
                return Err(err);
            }
            Ok(()) => {}
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn register_resource_dx(
        self: &Arc<EncoderInternal>,
        resource: &ID3D11Texture2D,
        format: NVencBufferFormat,
        pitch: u32,
    ) -> Result<RegisteredResource, NVencError> {
        use windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC;
        use windows::core::Interface;

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { resource.GetDesc(&raw mut desc) };

        let registered = unsafe {
            self.register_resource_raw(
                resource.as_raw(),
                NVencBufferFormat::Undefined,
                NVencBufferUsage::Image,
                NVencInputResourceType::DirectX,
                [desc.Width, desc.Height],
                pitch,
            )?
        };

        let mapped = unsafe { self.map_input_resource_raw(registered, format)? };

        Ok(RegisteredResource {
            encoder: self.clone(),
            pitch,
            width: desc.Width,
            height: desc.Height,
            registered,
            mapped,
        })
    }

    pub unsafe fn map_input_resource_raw(
        &self,
        resource: *mut c_void,
        format: NVencBufferFormat,
    ) -> Result<*mut c_void, NVencError> {
        let mut map: crate::sys::structs::NVencMapInputResource = unsafe { std::mem::zeroed() };
        map.version = NV_ENC_MAP_INPUT_RESOURCE_VER;
        map.registered_resource = resource;
        map.mapped_buffer_format = format;

        unsafe {
            (self.function_list.nvenc_map_input_resource)(self.encoder.as_ptr(), &raw mut map)
        }
        .into_error()?;

        Ok(map.mapped_resource)
    }

    pub unsafe fn register_resource_raw(
        &self,
        resource: *mut c_void,
        format: NVencBufferFormat,
        usage: NVencBufferUsage,
        ty: NVencInputResourceType,
        resolution: [u32; 2],
        pitch: u32,
    ) -> Result<*mut c_void, NVencError> {
        unsafe {
            self.register_resource_raw_with_fence(
                resource,
                format,
                usage,
                ty,
                resolution,
                pitch,
                None,
            )
        }
    }

    pub unsafe fn register_resource_raw_with_fence(
        &self,
        resource: *mut c_void,
        format: NVencBufferFormat,
        usage: NVencBufferUsage,
        ty: NVencInputResourceType,
        resolution: [u32; 2],
        pitch: u32,
        input_fence_point: Option<*mut NVencFencePointD3D12>,
    ) -> Result<*mut c_void, NVencError> {
        let mut register_resource: NVencRegisterResource = unsafe { std::mem::zeroed() };
        register_resource.buffer_format = format;
        register_resource.buffer_usage = usage;
        register_resource.resource_type = ty;
        register_resource.version = NV_ENC_REGISTER_RESOURCE_VER;
        register_resource.width = resolution[0];
        register_resource.height = resolution[1];
        register_resource.pitch = pitch;
        register_resource.sub_resource_index = 0;
        register_resource.resource_to_register = resource;
        register_resource.input_fence_point = input_fence_point.unwrap_or(std::ptr::null_mut());
        unsafe {
            (self.function_list.nvenc_register_resource)(
                self.encoder.as_ptr(),
                &raw mut register_resource,
            )
            .into_error()
        }?;
        Ok(register_resource.registered_resource)
    }

    pub unsafe fn unmap_input_resource(
        &self,
        mapped_resource: *mut c_void,
    ) -> Result<(), NVencError> {
        unsafe {
            (self.function_list.nvenc_unmap_input_resource)(self.encoder.as_ptr(), mapped_resource)
        }
        .into_error()
    }

    pub unsafe fn unregister_resource(
        &self,
        registered_resource: *mut c_void,
    ) -> Result<(), NVencError> {
        unsafe {
            (self.function_list.nvenc_unregister_resource)(
                self.encoder.as_ptr(),
                registered_resource,
            )
        }
        .into_error()
    }

    pub fn create_input_buffer(
        &self,
        width: u32,
        height: u32,
        memory_heap: NVencMemoryHeap,
        format: NVencBufferFormat,
    ) -> Result<*mut c_void, NVencError> {
        let mut input = NVencCreateInputBuffer {
            version: NV_ENC_CREATE_INPUT_BUFFER_VER,
            width: width,
            height: height,
            memory_heap,
            buffer_fmt: format,
            rsvd: 0,
            input_buffer: std::ptr::null_mut(),
            p_sys_mem_buffer: std::ptr::null_mut(),
            rsvd1: unsafe { std::mem::zeroed() },
            rsvd2: unsafe { std::mem::zeroed() },
        };
        unsafe {
            (self.function_list.nvenc_create_input_buffer)(self.encoder.as_ptr(), &raw mut input)
                .into_error()?
        };

        Ok(input.input_buffer)
    }

    pub fn destroy_input_buffer(&self, buffer: *mut c_void) -> Result<(), NVencError> {
        unsafe { (self.function_list.nvenc_destroy_input_buffer)(self.encoder.as_ptr(), buffer) }
            .into_error()
    }

    pub fn lock_input_buffer(&self, buffer: *mut c_void) -> Result<(*mut c_void, u32), NVencError> {
        let mut lock = NVencLockInputBuffer {
            version: NV_ENC_LOCK_INPUT_BUFFER_VER,
            bitflags: 0,
            input_buffer: buffer,
            buffer_data_ptr: std::ptr::null_mut(),
            pitch: 0,
            rsvd1: unsafe { std::mem::zeroed() },
            rsvd2: unsafe { std::mem::zeroed() },
        };

        unsafe {
            (self.function_list.nvenc_lock_input_buffer)(self.encoder.as_ptr(), &raw mut lock)
                .into_error()?;
        }

        Ok((lock.buffer_data_ptr, lock.pitch))
    }

    pub fn unlock_input_buffer(&self, locked_buffer: *mut c_void) -> Result<(), NVencError> {
        unsafe {
            (self.function_list.nvenc_unlock_input_buffer)(self.encoder.as_ptr(), locked_buffer)
                .into_error()
        }
    }

    pub fn encode_picture_raw(
        &self,
        input_buffer: *mut c_void,
        input_width: u32,
        input_height: u32,
        input_pitch: u32,
        output_bitstream: *mut c_void,
        frame_count: usize,
        timestamp: u64,
        format: NVencBufferFormat,
        pic_struct: NVencPicStruct,
        ty: NVencPicType,
        codec_params: Option<CodecPicParams>,
    ) -> Result<(), NVencError> {
        let mut params: MaybeUninit<NVencPicParams> = MaybeUninit::zeroed();
        let sys_codec = match codec_params {
            None => NVencCodecPicParams { rsvd: [0; 256] },
            Some(CodecPicParams::H264(codec)) => NVencCodecPicParams {
                h264_pic_params: ManuallyDrop::new(codec),
            },
        };
        unsafe { &mut *params.as_mut_ptr() }.version = NV_ENC_PIC_PARAMS_VER;
        unsafe { &mut *params.as_mut_ptr() }.input_width = input_width;
        unsafe { &mut *params.as_mut_ptr() }.input_height = input_height;
        unsafe { &mut *params.as_mut_ptr() }.input_pitch = input_pitch;
        unsafe { &mut *params.as_mut_ptr() }.output_bitstream = output_bitstream;
        unsafe { &mut *params.as_mut_ptr() }.input_buffer = input_buffer;
        unsafe { &mut *params.as_mut_ptr() }.buffer_format = format;
        unsafe { &mut *params.as_mut_ptr() }.picture_struct = pic_struct;
        unsafe { &mut *params.as_mut_ptr() }.picture_type = ty;
        unsafe { &mut *params.as_mut_ptr() }.frame_idx = frame_count as u32;
        unsafe { &mut *params.as_mut_ptr() }.input_time_stamp = timestamp;
        unsafe { &mut *params.as_mut_ptr() }.codec_pic_params = sys_codec;
        match unsafe {
            (self.function_list.nvenc_encode_picture)(self.encoder.as_ptr(), &raw mut params)
                .into_error()
        } {
            Err(err) => {
                let ptr =
                    unsafe { (self.function_list.nvenc_get_last_error)(self.encoder.as_ptr()) };
                println!("{:?}", unsafe { CStr::from_ptr(ptr) });
                Err(err)
            }
            Ok(()) => Ok(()),
        }
    }

    pub fn end_encode(&self) -> Result<(), NVencError> {
        let mut params: MaybeUninit<NVencPicParams> = MaybeUninit::zeroed();
        unsafe { &mut *params.as_mut_ptr() }.version = NV_ENC_PIC_PARAMS_VER;
        unsafe { &mut *params.as_mut_ptr() }.encode_pic_flags = NVencPicFlags::Eos as u32;
        unsafe {
            (self.function_list.nvenc_encode_picture)(self.encoder.as_ptr(), &raw mut params)
                .into_error()
        }?;
        Ok(())
    }

    pub(crate) fn lock_bit_stream_buffer(
        &self,
        buffer: *mut c_void,
        wait: bool,
    ) -> Result<NVencLockBitStream, NVencError> {
        let mut lock: MaybeUninit<NVencLockBitStream> = MaybeUninit::zeroed();
        unsafe { &mut *lock.as_mut_ptr() }.version = NV_ENC_LOCK_BITSTREAM_VER;
        unsafe { &mut *lock.as_mut_ptr() }.output_bit_stream = buffer;
        unsafe { &mut *lock.as_mut_ptr() }
            .bit_fields
            .set_do_not_wait(!wait);
        unsafe { (self.function_list.nvenc_lock_bit_stream)(self.encoder.as_ptr(), &raw mut lock) }
            .into_error()?;
        Ok(unsafe { lock.assume_init() })
    }

    pub(crate) fn unlock_bit_stream_buffer(&self, buffer: *mut c_void) -> Result<(), NVencError> {
        unsafe { (self.function_list.nvenc_unlock_bit_stream)(self.encoder.as_ptr(), buffer) }
            .into_error()
    }

    pub fn create_bitstream_buffer(self: &Arc<Self>) -> Result<BitStream, NVencError> {
        let mut params: NVencCreateBitstreamBuffer = unsafe { std::mem::zeroed() };
        params.version = NV_ENC_CREATE_BITSTREAM_BUFFER_VER;
        match unsafe {
            (self.function_list.nvenc_create_bit_stream_buffer)(
                self.encoder.as_ptr(),
                &raw mut params,
            )
        }
        .into_error()
        {
            Err(err) => {
                let ptr = unsafe { (self.function_list.nvenc_get_last_error)(self.encoder.as_ptr()) };
                if !ptr.is_null() {
                    println!("{:?}", unsafe { CStr::from_ptr(ptr) });
                }
                return Err(err);
            }
            Ok(()) => {}
        }
        Ok(BitStream {
            buffer: params.bitstream_buffer,
            encoder: self.clone(),
        })
    }

    pub(crate) fn destroy_bitstream_buffer(&self, buffer: *mut c_void) -> Result<(), NVencError> {
        unsafe {
            (self.function_list.nvenc_destory_bit_stream_buffer)(self.encoder.as_ptr(), buffer)
        }
        .into_error()
    }
}

pub enum CodecPicParams {
    H264(NVencPicParamsH264),
}

pub struct RawBitStreamLockGuard {
    encoder: Arc<EncoderInternal>,
    output_ptr: *mut c_void,
    data_ptr: *mut c_void,
    data_len: u32,
}

impl RawBitStreamLockGuard {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data_ptr as _, self.data_len as usize) }
    }
}

impl Drop for RawBitStreamLockGuard {
    fn drop(&mut self) {
        let _ = self.encoder.unlock_bit_stream_buffer(self.output_ptr);
    }
}

pub struct RegisteredResource {
    registered: *mut c_void,
    mapped: *mut c_void,
    pitch: u32,
    width: u32,
    height: u32,
    encoder: Arc<EncoderInternal>,
}

impl Drop for RegisteredResource {
    fn drop(&mut self) {
        let _ = unsafe { self.encoder.unmap_input_resource(self.mapped) };
        let _ = unsafe { self.encoder.unregister_resource(self.registered) };
    }
}
