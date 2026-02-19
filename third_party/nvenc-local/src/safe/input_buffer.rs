use std::{ffi::c_void, sync::Arc};

use crate::{encoder::EncoderInternal, sys::result::NVencError};

pub struct InputBuffer {
    pub(crate) encoder: Arc<EncoderInternal>,
    pub(crate) buffer: *mut c_void,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl InputBuffer {
    pub fn lock(&self) -> Result<InputBufferLock<'_>, NVencError> {
        let (lock_ptr, pitch) = self.encoder.lock_input_buffer(self.buffer)?;
        Ok(InputBufferLock {
            input_buffer: self,
            lock_ptr,
            pitch,
        })
    }
}

impl Drop for InputBuffer {
    fn drop(&mut self) {
        let _ = self.encoder.destroy_input_buffer(self.buffer);
    }
}

pub struct InputBufferLock<'a> {
    input_buffer: &'a InputBuffer,
    lock_ptr: *mut c_void,
    pitch: u32,
}

impl InputBufferLock<'_> {
    pub unsafe fn data_ptr(&self) -> *mut u8 {
        self.lock_ptr as _
    }

    pub fn width(&self) -> u32 {
        self.input_buffer.width
    }

    pub fn height(&self) -> u32 {
        self.input_buffer.height
    }

    pub fn pitch(&self) -> u32 {
        self.pitch
    }
}

impl Drop for InputBufferLock<'_> {
    fn drop(&mut self) {
        // NVENC expects the original input-buffer handle for unlock,
        // not the mapped data pointer returned by lock_input_buffer.
        let _ = self
            .input_buffer
            .encoder
            .unlock_input_buffer(self.input_buffer.buffer);
    }
}
