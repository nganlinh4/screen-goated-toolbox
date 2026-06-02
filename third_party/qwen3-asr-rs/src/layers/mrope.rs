use crate::tensor::{Device, Tensor};

/// Compute MRoPE cos/sin tensors for the given 3D position IDs.
pub fn compute_mrope_cos_sin(
    position_ids: &[Vec<i64>; 3],
    head_dim: usize,
    rope_theta: f64,
    mrope_section: &[usize],
    interleaved: bool,
    device: Device,
) -> (Tensor, Tensor) {
    let half_dim = head_dim / 2;
    let seq_len = position_ids[0].len();

    let inv_freq: Vec<f64> = (0..half_dim)
        .map(|i| 1.0 / rope_theta.powf(2.0 * i as f64 / head_dim as f64))
        .collect();

    let dim_map = if interleaved {
        build_interleaved_dim_map(mrope_section, half_dim)
    } else {
        build_contiguous_dim_map(mrope_section, half_dim)
    };

    let mut cos_vals = vec![0.0f32; seq_len * head_dim];
    let mut sin_vals = vec![0.0f32; seq_len * head_dim];

    for t in 0..seq_len {
        for j in 0..half_dim {
            let dim = dim_map[j];
            let pos = position_ids[dim][t] as f64;
            let angle = pos * inv_freq[j];
            let c = angle.cos() as f32;
            let s = angle.sin() as f32;
            cos_vals[t * head_dim + j] = c;
            sin_vals[t * head_dim + j] = s;
            cos_vals[t * head_dim + j + half_dim] = c;
            sin_vals[t * head_dim + j + half_dim] = s;
        }
    }

    let cos = Tensor::from_slice_f32(&cos_vals)
        .reshape(&[seq_len as i64, head_dim as i64])
        .to_device(device);
    let sin = Tensor::from_slice_f32(&sin_vals)
        .reshape(&[seq_len as i64, head_dim as i64])
        .to_device(device);

    (cos, sin)
}

fn build_contiguous_dim_map(sections: &[usize], total: usize) -> Vec<usize> {
    let mut map = Vec::with_capacity(total);
    for (dim, &size) in sections.iter().enumerate() {
        for _ in 0..size {
            if map.len() >= total {
                break;
            }
            map.push(dim);
        }
    }
    while map.len() < total {
        map.push(sections.len() - 1);
    }
    map
}

fn build_interleaved_dim_map(sections: &[usize], total: usize) -> Vec<usize> {
    let n_dims = sections.len();
    let mut map = Vec::with_capacity(total);
    let mut counts = vec![0usize; n_dims];

    while map.len() < total {
        let prev_len = map.len();
        for dim in 0..n_dims {
            if map.len() >= total {
                break;
            }
            if counts[dim] < sections[dim] {
                map.push(dim);
                counts[dim] += 1;
            }
        }
        if map.len() == prev_len {
            break;
        }
    }

    map
}
