use std::{ffi::c_void, sync::Arc};

use crate::{safe::encoder::EncoderInternal, sys::result::NVencError};

pub struct BitStream {
    pub(crate) buffer: *mut c_void,
    pub(crate) encoder: Arc<EncoderInternal>,
}

impl Drop for BitStream {
    fn drop(&mut self) {
        println!("Dropping bitstream buffer");
        let _ = self.encoder.destroy_bitstream_buffer(self.buffer);
    }
}

unsafe impl Send for BitStream {}

impl BitStream {
    pub fn raw_ptr(&self) -> *mut c_void {
        self.buffer
    }

    /// Attempts to lock the bit stream, if `wait` is true it will wait
    /// otherwise a `LockBusy` Error may be returned, in which case the
    /// client should retry in a few milliseconds
    pub fn try_lock(&self, wait: bool) -> Result<BitStreamLockGuard<'_>, NVencError> {
        let lock = self.encoder.lock_bit_stream_buffer(self.buffer, wait)?;
        Ok(BitStreamLockGuard {
            buffer: self,
            data_ptr: lock.bitstream_buffer,
            data_len: lock.bitstream_size_in_bytes,
        })
    }
}

/// Holds a reference to the `BitStream` and holds the data and associated fields
pub struct BitStreamLockGuard<'a> {
    buffer: &'a BitStream,
    data_ptr: *mut c_void,
    data_len: u32,
}

impl BitStreamLockGuard<'_> {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data_ptr as _, self.data_len as _) }
    }
}

impl<'a> Drop for BitStreamLockGuard<'a> {
    fn drop(&mut self) {
        let _ = self
            .buffer
            .encoder
            .unlock_bit_stream_buffer(self.buffer.buffer);
    }
}
