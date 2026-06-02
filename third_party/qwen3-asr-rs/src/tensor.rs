//! Unified tensor abstraction over tch (libtorch) and MLX backends.
//!
//! All neural network modules use these types instead of importing `tch` directly.

// ---------------------------------------------------------------------------
// DType — data type abstraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    Float32,
    Float16,
    BFloat16,
    Int8,
    Int64,
    Int32,
    Bool,
}

#[cfg(feature = "tch-backend")]
impl From<DType> for tch::Kind {
    fn from(dt: DType) -> Self {
        match dt {
            DType::Float32 => tch::Kind::Float,
            DType::Float16 => tch::Kind::Half,
            DType::BFloat16 => tch::Kind::BFloat16,
            DType::Int8 => tch::Kind::Int8,
            DType::Int64 => tch::Kind::Int64,
            DType::Int32 => tch::Kind::Int,
            DType::Bool => tch::Kind::Bool,
        }
    }
}

#[cfg(feature = "tch-backend")]
impl From<tch::Kind> for DType {
    fn from(kind: tch::Kind) -> Self {
        match kind {
            tch::Kind::Float => DType::Float32,
            tch::Kind::Half => DType::Float16,
            tch::Kind::BFloat16 => DType::BFloat16,
            tch::Kind::Int8 => DType::Int8,
            tch::Kind::Int64 => DType::Int64,
            tch::Kind::Int => DType::Int32,
            tch::Kind::Bool => DType::Bool,
            _ => DType::Float32,
        }
    }
}

#[cfg(feature = "mlx")]
impl From<DType> for crate::backend::mlx::ffi::mlx_dtype {
    fn from(dt: DType) -> Self {
        use crate::backend::mlx::ffi::mlx_dtype::*;
        match dt {
            DType::Float32 => MLX_FLOAT32,
            DType::Float16 => MLX_FLOAT16,
            DType::BFloat16 => MLX_BFLOAT16,
            DType::Int8 => MLX_INT8,
            DType::Int64 => MLX_INT64,
            DType::Int32 => MLX_INT32,
            DType::Bool => MLX_BOOL,
        }
    }
}

#[cfg(feature = "mlx")]
impl From<crate::backend::mlx::ffi::mlx_dtype> for DType {
    fn from(dt: crate::backend::mlx::ffi::mlx_dtype) -> Self {
        use crate::backend::mlx::ffi::mlx_dtype::*;
        match dt {
            MLX_FLOAT32 | MLX_FLOAT64 => DType::Float32,
            MLX_FLOAT16 => DType::Float16,
            MLX_BFLOAT16 => DType::BFloat16,
            MLX_INT8 => DType::Int8,
            MLX_INT64 => DType::Int64,
            MLX_INT32 | MLX_INT16 => DType::Int32,
            MLX_BOOL => DType::Bool,
            _ => DType::Float32,
        }
    }
}

// ---------------------------------------------------------------------------
// Device — compute device abstraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Cpu,
    Gpu(usize),
}

impl Device {
    pub fn gpu() -> Self {
        Device::Gpu(0)
    }
}

#[cfg(feature = "tch-backend")]
impl From<Device> for tch::Device {
    fn from(d: Device) -> Self {
        match d {
            Device::Cpu => tch::Device::Cpu,
            Device::Gpu(i) => tch::Device::Cuda(i),
        }
    }
}

#[cfg(feature = "tch-backend")]
impl From<tch::Device> for Device {
    fn from(d: tch::Device) -> Self {
        match d {
            tch::Device::Cpu => Device::Cpu,
            tch::Device::Cuda(i) => Device::Gpu(i),
            _ => Device::Cpu,
        }
    }
}

// ---------------------------------------------------------------------------
// Tensor — unified tensor type
// ---------------------------------------------------------------------------

pub struct Tensor {
    #[cfg(feature = "tch-backend")]
    pub(crate) inner: tch::Tensor,

    #[cfg(feature = "mlx")]
    pub(crate) inner: crate::backend::mlx::array::MlxArray,
}

pub struct StftConfig<'a> {
    pub n_fft: i64,
    pub hop_length: i64,
    pub win_length: i64,
    pub window: &'a Tensor,
    pub normalized: bool,
    pub onesided: bool,
    pub return_complex: bool,
}

impl std::fmt::Debug for Tensor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tensor(shape={:?}, dtype={:?})",
            self.size(),
            self.kind()
        )
    }
}

impl Clone for Tensor {
    fn clone(&self) -> Self {
        #[cfg(feature = "tch-backend")]
        {
            Tensor {
                inner: self.inner.shallow_clone(),
            }
        }
        #[cfg(feature = "mlx")]
        {
            Tensor {
                inner: self.inner.clone(),
            }
        }
    }
}


#[cfg(feature = "tch-backend")]
mod tch_backend;
#[cfg(feature = "mlx")]
mod mlx_backend;
mod ops;
