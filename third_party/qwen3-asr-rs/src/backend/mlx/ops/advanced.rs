use crate::backend::mlx::array::MlxArray;
use crate::backend::mlx::ffi;
use crate::backend::mlx::stream::default_stream;

pub fn fast_rms_norm(x: &MlxArray, weight: &MlxArray, eps: f32) -> MlxArray {
    let mut res = MlxArray::empty();
    unsafe {
        ffi::mlx_fast_rms_norm(&mut res.ptr, x.ptr, weight.ptr, eps, default_stream());
    }
    res
}

pub fn fast_layer_norm(
    x: &MlxArray,
    weight: &MlxArray,
    bias: Option<&MlxArray>,
    eps: f32,
) -> MlxArray {
    let mut res = MlxArray::empty();
    let bias_ptr = bias.map_or(std::ptr::null_mut(), |b| b.ptr);
    unsafe {
        ffi::mlx_fast_layer_norm(
            &mut res.ptr,
            x.ptr,
            weight.ptr,
            bias_ptr,
            eps,
            default_stream(),
        );
    }
    res
}

pub fn fast_rope(
    x: &MlxArray,
    dims: i32,
    traditional: bool,
    base: f32,
    scale: f32,
    offset: i32,
) -> MlxArray {
    let mut res = MlxArray::empty();
    let base_opt = ffi::mlx_optional_float {
        value: base,
        has_value: true,
    };
    let freqs = std::ptr::null_mut() as ffi::mlx_array;
    unsafe {
        ffi::mlx_fast_rope(
            &mut res.ptr,
            x.ptr,
            dims,
            traditional,
            base_opt,
            scale,
            offset,
            freqs,
            default_stream(),
        );
    }
    res
}

pub fn fast_sdpa(
    queries: &MlxArray,
    keys: &MlxArray,
    values: &MlxArray,
    scale: f32,
    mask: Option<&MlxArray>,
) -> MlxArray {
    let mut res = MlxArray::empty();
    let mask_ptr = mask.map_or(std::ptr::null_mut() as ffi::mlx_array, |m| m.ptr);
    let sinks = std::ptr::null_mut() as ffi::mlx_array;
    let mask_mode = if mask.is_some() {
        b"array\0".as_ptr() as *const std::os::raw::c_char
    } else {
        b"\0".as_ptr() as *const std::os::raw::c_char
    };
    unsafe {
        ffi::mlx_fast_scaled_dot_product_attention(
            &mut res.ptr,
            queries.ptr,
            keys.ptr,
            values.ptr,
            scale,
            mask_mode,
            mask_ptr,
            sinks,
            default_stream(),
        );
    }
    res
}

pub fn rfft(a: &MlxArray, n: i32, axis: i32) -> MlxArray {
    let mut res = MlxArray::empty();
    unsafe { ffi::mlx_fft_rfft(&mut res.ptr, a.ptr, n, axis, default_stream()) };
    res
}

pub fn topk(a: &MlxArray, k: i32, axis: i32) -> MlxArray {
    let mut res = MlxArray::empty();
    unsafe { ffi::mlx_topk_axis(&mut res.ptr, a.ptr, k, axis, default_stream()) };
    res
}

pub fn argsort(a: &MlxArray, axis: i32) -> MlxArray {
    let mut res = MlxArray::empty();
    unsafe { ffi::mlx_argsort_axis(&mut res.ptr, a.ptr, axis, default_stream()) };
    res
}
