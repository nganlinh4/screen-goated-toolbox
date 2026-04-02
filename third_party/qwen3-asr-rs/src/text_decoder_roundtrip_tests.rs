#![cfg(test)]

use crate::tensor::{DType, Device, Tensor};
use crate::text_decoder::{KvCache, KvCacheEntry, KvCacheMode, create_causal_mask};

fn make_kv_tensor(total_tokens: i64, offset: f32) -> Tensor {
    make_kv_tensor_with_heads(1, 1, total_tokens, offset)
}

fn make_kv_tensor_with_heads(batch: i64, heads: i64, total_tokens: i64, offset: f32) -> Tensor {
    let mut values = Vec::new();
    for batch_idx in 0..batch {
        for head_idx in 0..heads {
            for token in 0..total_tokens {
                for dim in 0..4 {
                    values.push(
                        offset
                            + batch_idx as f32 * 3.0
                            + head_idx as f32 * 0.5
                            + token as f32 * 0.25
                            + dim as f32 * 0.125,
                    );
                }
            }
        }
    }
    Tensor::from_slice_f32(&values)
        .reshape(&[batch, heads, total_tokens, 4])
        .to_device(Device::Cpu)
}

fn max_abs_diff(left: &Tensor, right: &Tensor) -> f32 {
    let left = left.to_dtype(DType::Float32).to_device(Device::Cpu).to_vec_f32();
    let right = right
        .to_dtype(DType::Float32)
        .to_device(Device::Cpu)
        .to_vec_f32();
    left.iter()
        .zip(right.iter())
        .map(|(l, r)| (l - r).abs())
        .fold(0.0f32, f32::max)
}

fn expand_kv_heads_for_test(tensor: &Tensor, num_query_heads: i64) -> Tensor {
    let num_kv_heads = tensor.size()[1];
    if num_query_heads == num_kv_heads {
        return tensor.shallow_clone();
    }
    let n_rep = num_query_heads / num_kv_heads;
    let (batch, _, seq_len, head_dim) = (
        tensor.size()[0],
        tensor.size()[1],
        tensor.size()[2],
        tensor.size()[3],
    );
    tensor
        .unsqueeze(2)
        .expand(&[batch, num_kv_heads, n_rep, seq_len, head_dim], false)
        .reshape(&[batch, num_query_heads, seq_len, head_dim])
}
#[test]
fn experimental_turboquant_round_trips_recent_tail_and_prefix_reasonably() {
    let key = make_kv_tensor(128, 1.0);
    let value = make_kv_tensor(128, -2.0);
    let append_key = make_kv_tensor(256, 1.0).narrow(2, 128, 128);
    let append_value = make_kv_tensor(256, -2.0).narrow(2, 128, 128);
    let expected_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let expected_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);

    let mut entry = KvCacheEntry::from_tokens(
        key.shallow_clone(),
        value.shallow_clone(),
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    assert!(entry.quantized_prefix_len() > 0);

    let rebuilt_key = entry.key_view();
    let rebuilt_value = entry.value_view();

    assert_eq!(rebuilt_key.size(), expected_key.size());
    assert_eq!(rebuilt_value.size(), expected_value.size());
    assert_eq!(entry.len(), 256);

    let key_diff = max_abs_diff(&rebuilt_key, &expected_key);
    let value_diff = max_abs_diff(&rebuilt_value, &expected_value);
    assert!(key_diff < 0.2, "key round-trip diff too large: {key_diff}");
    assert!(
        value_diff < 0.2,
        "value round-trip diff too large: {value_diff}"
    );
    assert!(entry.compressed_prefix_bytes() > 0);
    assert!(
        entry.compressed_prefix_bytes() < entry.dense_prefix_equivalent_bytes(),
        "TurboQuant prefix should be smaller than dense storage"
    );
    assert!(
        entry.total_cache_bytes() < (256 * 4 * std::mem::size_of::<f32>() * 2),
        "TurboQuant cache should reduce total key/value bytes"
    );
}
#[test]
fn dense_only_attention_now_flows_through_cache_attention_path_for_both_modes() {
    let query = Tensor::from_slice_f32(&[0.2, -0.1, 0.4, 0.05])
        .reshape(&[1, 1, 1, 4])
        .to_device(Device::Cpu);
    let scale = 1.0 / (4.0f64).sqrt();

    for mode in [KvCacheMode::DenseAppend, KvCacheMode::ExperimentalTurboQuant] {
        let key = make_kv_tensor(32, 0.3);
        let value = make_kv_tensor(32, -0.7);
        let entry = KvCacheEntry::from_tokens(key.shallow_clone(), value.shallow_clone(), mode);
        let cached = entry
            .attend_with_quantized_prefix(&query, scale, None)
            .expect("dense-only cache should now produce cached attention output");
        let dense = Tensor::scaled_dot_product_attention(&query, &key, &value, scale, None);

        assert_eq!(
            max_abs_diff(&cached, &dense),
            0.0,
            "dense-only cached attention drifted in mode {:?}",
            mode
        );
    }
}
#[test]
fn experimental_turboquant_decode_attention_stays_reasonably_close_to_dense() {
    let key = make_kv_tensor(128, 0.75);
    let value = make_kv_tensor(128, -1.25);
    let append_key = make_kv_tensor(256, 0.75).narrow(2, 128, 128);
    let append_value = make_kv_tensor(256, -1.25).narrow(2, 128, 128);
    let full_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let full_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);
    let query = Tensor::from_slice_f32(&[0.1, -0.2, 0.3, 0.4])
        .reshape(&[1, 1, 1, 4])
        .to_device(Device::Cpu);

    let mut entry = KvCacheEntry::from_tokens(
        key.shallow_clone(),
        value.shallow_clone(),
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    assert!(entry.quantized_prefix_len() > 0);
    let scale = 1.0 / (4.0f64).sqrt();
    let approx = entry
        .attend_with_quantized_prefix(&query, scale, None)
        .expect("experimental TurboQuant path should be active");
    let dense = Tensor::scaled_dot_product_attention(&query, &full_key, &full_value, scale, None);

    assert!(
        max_abs_diff(&approx, &dense) < 0.35,
        "TurboQuant attention drifted too far from dense attention"
    );
}
#[test]
fn experimental_turboquant_multi_head_decode_attention_stays_reasonably_close_to_dense() {
    let key = make_kv_tensor_with_heads(1, 2, 128, 0.6);
    let value = make_kv_tensor_with_heads(1, 2, 128, -1.4);
    let append_key = make_kv_tensor_with_heads(1, 2, 192, 0.6).narrow(2, 128, 64);
    let append_value = make_kv_tensor_with_heads(1, 2, 192, -1.4).narrow(2, 128, 64);
    let full_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let full_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);
    let query = Tensor::from_slice_f32(&[
        0.1, -0.2, 0.3, 0.4, -0.4, 0.5, -0.6, 0.7, 0.2, 0.1, -0.3, 0.8, -0.7, 0.6, 0.5,
        -0.1,
    ])
    .reshape(&[1, 4, 1, 4])
    .to_device(Device::Cpu);

    let mut entry = KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant);
    entry.append(&append_key, &append_value);
    let scale = 1.0 / (4.0f64).sqrt();
    let approx = entry
        .attend_with_quantized_prefix(&query, scale, None)
        .expect("experimental TurboQuant path should be active");
    let dense = Tensor::scaled_dot_product_attention(
        &query,
        &expand_kv_heads_for_test(&full_key, query.size()[1]),
        &expand_kv_heads_for_test(&full_value, query.size()[1]),
        scale,
        None,
    );

    assert!(
        max_abs_diff(&approx, &dense) < 0.35,
        "multi-head TurboQuant attention drifted too far from dense attention"
    );
}
#[test]
fn experimental_turboquant_state_copy_keeps_compressed_stats() {
    let key = make_kv_tensor(128, 0.25);
    let value = make_kv_tensor(128, -0.75);
    let append_key = make_kv_tensor(256, 0.25).narrow(2, 128, 128);
    let append_value = make_kv_tensor(256, -0.75).narrow(2, 128, 128);

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    let original_prefix_len = entry.quantized_prefix_len();
    let original_prefix_bytes = entry.compressed_prefix_bytes();
    let original_dense_prefix_bytes = entry.dense_prefix_equivalent_bytes();
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let copy = cache.deep_copy_with_reserve(16);
    let copy = copy.layers[0].as_ref().expect("copied layer must exist");

    assert_eq!(copy.quantized_prefix_len(), original_prefix_len);
    assert_eq!(copy.compressed_prefix_bytes(), original_prefix_bytes);
    assert_eq!(copy.dense_prefix_equivalent_bytes(), original_dense_prefix_bytes);
}
#[test]
fn experimental_turboquant_copy_does_not_reserve_dense_space_for_compressed_prefix() {
    let key = make_kv_tensor(128, 0.4);
    let value = make_kv_tensor(128, -0.6);
    let append_key = make_kv_tensor(256, 0.4).narrow(2, 128, 128);
    let append_value = make_kv_tensor(256, -0.6).narrow(2, 128, 128);

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let copy = cache.deep_copy_with_reserve(16);
    let copied_entry = copy.layers[0].as_ref().expect("copied layer must exist");

    assert_eq!(copied_entry.quantized_prefix_len(), 256);
    assert_eq!(copied_entry.dense_len, 0);
    assert_eq!(copied_entry.dense_capacity(), 16);
}
#[test]
fn experimental_turboquant_can_precompress_without_append() {
    let key = make_kv_tensor(256, 0.9);
    let value = make_kv_tensor(256, -0.9);
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    ));

    let original = cache.layers[0].as_ref().expect("original layer must exist");
    assert_eq!(original.quantized_prefix_len(), 0);

    let copy = cache.deep_copy_generation_ready(0);
    let copied_entry = copy.layers[0].as_ref().expect("copied layer must exist");
    assert_eq!(copied_entry.quantized_prefix_len(), 256);
    assert_eq!(copied_entry.dense_len, 0);
    assert!(copied_entry.compressed_prefix_bytes() > 0);
    assert_eq!(copied_entry.dense_capacity(), 0);
    let query = make_kv_tensor(4, 0.11);
    let scale = 1.0 / (4.0f64).sqrt();
    let original_attention = original
        .attend_with_quantized_prefix(&query, scale, None)
        .expect("original TurboQuant attention must exist");
    let copied_attention = copied_entry
        .attend_with_quantized_prefix(&query, scale, None)
        .expect("copied TurboQuant attention must exist");
    assert!(max_abs_diff(&copied_attention, &original_attention) < 0.0001);
}
#[test]
fn experimental_turboquant_force_compresses_remaining_dense_tail_page_on_generation_ready_copy() {
    let key = make_kv_tensor(128, 0.7);
    let value = make_kv_tensor(128, -0.3);
    let append_key = make_kv_tensor(192, 0.7).narrow(2, 128, 64);
    let append_value = make_kv_tensor(192, -0.3).narrow(2, 128, 64);
    let expected_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let expected_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);

    let mut entry = KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant);
    entry.append(&append_key, &append_value);

    assert_eq!(entry.len(), 192);
    assert_eq!(entry.quantized_prefix_len(), 192);
    assert_eq!(entry.dense_len, 0);
    assert_eq!(entry.dense_capacity(), 0);
    assert!(entry.compressed_prefix_bytes() > 0);
    assert!(entry.dense_prefix_equivalent_bytes() > 0);
    assert!(max_abs_diff(&entry.key_view(), &expected_key) < 0.2);
    assert!(max_abs_diff(&entry.value_view(), &expected_value) < 0.2);

    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let copy = cache.deep_copy_generation_ready(0);
    let copied_entry = copy.layers[0].as_ref().expect("copied layer must exist");

    assert_eq!(copied_entry.quantized_prefix_len(), 192);
    assert_eq!(copied_entry.compressed_prefix_bytes(), entry.compressed_prefix_bytes());
    assert_eq!(copied_entry.dense_prefix_equivalent_bytes(), entry.dense_prefix_equivalent_bytes());
    assert_eq!(copied_entry.dense_len, 0);
    assert_eq!(copied_entry.dense_capacity(), 0);
    assert_eq!(copied_entry.len(), 192);
    assert!(max_abs_diff(&copied_entry.key_view(), &expected_key) < 0.2);
    assert!(max_abs_diff(&copied_entry.value_view(), &expected_value) < 0.2);
}
#[test]
fn experimental_turboquant_mutable_offload_preserves_multi_head_stride() {
    let key = make_kv_tensor_with_heads(1, 2, 128, 0.15);
    let value = make_kv_tensor_with_heads(1, 2, 128, -0.35);
    let append_key = make_kv_tensor_with_heads(1, 2, 192, 0.15).narrow(2, 128, 64);
    let append_value = make_kv_tensor_with_heads(1, 2, 192, -0.35).narrow(2, 128, 64);
    let expected_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let expected_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);

    assert_eq!(entry.len(), 192);
    assert_eq!(entry.dense_capacity(), 0);
    assert_eq!(entry.quantized_prefix_len(), 192);
    assert_eq!(entry.dense_len, 0);
    assert_eq!(entry.key_view().size(), expected_key.size());
    assert_eq!(entry.value_view().size(), expected_value.size());
    assert!(
        max_abs_diff(&entry.key_view(), &expected_key) < 0.2,
        "TurboQuant multi-head mutable offload drifted too far on key cache"
    );
    assert!(
        max_abs_diff(&entry.value_view(), &expected_value) < 0.2,
        "TurboQuant multi-head mutable offload drifted too far on value cache"
    );
    assert!(entry.compressed_prefix_bytes() > 0);
    assert!(entry.dense_prefix_equivalent_bytes() > 0);
}
#[test]
fn experimental_turboquant_masked_attention_stays_reasonably_close_to_dense() {
    let key = make_kv_tensor(128, 1.5);
    let value = make_kv_tensor(128, -0.25);
    let append_key = make_kv_tensor(192, 1.5).narrow(2, 128, 64);
    let append_value = make_kv_tensor(192, -0.25).narrow(2, 128, 64);
    let full_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let full_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);
    let query = Tensor::from_slice_f32(&[
        0.2, -0.1, 0.3, 0.5,
        -0.4, 0.2, 0.1, 0.6,
        0.3, 0.2, -0.2, 0.4,
        0.1, -0.3, 0.5, 0.2,
    ])
    .reshape(&[1, 1, 4, 4])
    .to_device(Device::Cpu);

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    let scale = 1.0 / (4.0f64).sqrt();
    let mask = create_causal_mask(4, 128 + 64 - 4, Device::Cpu);
    let approx = entry
        .attend_with_quantized_prefix(&query, scale, Some(&mask))
        .expect("experimental TurboQuant masked path should be active");
    let dense = Tensor::scaled_dot_product_attention(&query, &full_key, &full_value, scale, Some(&mask));

    assert!(
        max_abs_diff(&approx, &dense) < 0.4,
        "TurboQuant masked attention drifted too far from dense attention"
    );
}
#[test]
fn experimental_turboquant_multi_head_attention_stays_reasonably_close_to_dense() {
    let key = make_kv_tensor_with_heads(1, 2, 96, 0.2);
    let value = make_kv_tensor_with_heads(1, 2, 96, -0.4);
    let append_key = make_kv_tensor_with_heads(1, 2, 160, 0.2).narrow(2, 96, 64);
    let append_value = make_kv_tensor_with_heads(1, 2, 160, -0.4).narrow(2, 96, 64);
    let full_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let full_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);
    let query = make_kv_tensor_with_heads(1, 2, 3, 1.1);

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    let scale = 1.0 / (4.0f64).sqrt();
    let approx = entry
        .attend_with_quantized_prefix(&query, scale, None)
        .expect("experimental TurboQuant multi-head path should be active");
    let dense = Tensor::scaled_dot_product_attention(&query, &full_key, &full_value, scale, None);

    assert!(
        max_abs_diff(&approx, &dense) < 0.35,
        "TurboQuant multi-head attention drifted too far from dense attention"
    );
}
#[test]
fn experimental_turboquant_generation_ready_copy_preserves_multi_head_mixed_state() {
    let key = make_kv_tensor_with_heads(1, 2, 128, 0.45);
    let value = make_kv_tensor_with_heads(1, 2, 128, -0.15);
    let append_key = make_kv_tensor_with_heads(1, 2, 224, 0.45).narrow(2, 128, 96);
    let append_value = make_kv_tensor_with_heads(1, 2, 224, -0.15).narrow(2, 128, 96);

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key, &append_value);
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let copy = cache.deep_copy_generation_ready(32);
    let copied_entry = copy.layers[0].as_ref().expect("copied layer must exist");

    assert_eq!(copied_entry.quantized_prefix_len(), 224);
    assert_eq!(copied_entry.compressed_prefix_bytes(), entry.compressed_prefix_bytes());
    assert_eq!(copied_entry.dense_prefix_equivalent_bytes(), entry.dense_prefix_equivalent_bytes());
    assert_eq!(copied_entry.dense_capacity(), 32);
    assert_eq!(copied_entry.dense_len, 0);
    assert_eq!(copied_entry.len(), 224);
}


#[test]
fn experimental_turboquant_mutable_offload_handles_multi_head_repeated_appends() {
    let key = make_kv_tensor_with_heads(2, 3, 128, 0.15);
    let value = make_kv_tensor_with_heads(2, 3, 128, -0.35);
    let append_key_1 = make_kv_tensor_with_heads(2, 3, 192, 0.15).narrow(2, 128, 64);
    let append_value_1 = make_kv_tensor_with_heads(2, 3, 192, -0.35).narrow(2, 128, 64);
    let append_key_2 = make_kv_tensor_with_heads(2, 3, 256, 0.15).narrow(2, 192, 64);
    let append_value_2 = make_kv_tensor_with_heads(2, 3, 256, -0.35).narrow(2, 192, 64);
    let expected_key = Tensor::cat(
        &[
            key.shallow_clone(),
            append_key_1.shallow_clone(),
            append_key_2.shallow_clone(),
        ],
        2,
    );
    let expected_value = Tensor::cat(
        &[
            value.shallow_clone(),
            append_value_1.shallow_clone(),
            append_value_2.shallow_clone(),
        ],
        2,
    );

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key_1, &append_value_1);
    let first_prefix_len = entry.quantized_prefix_len();
    let first_prefix_bytes = entry.compressed_prefix_bytes();
    assert!(first_prefix_len > 0);
    assert!(first_prefix_bytes > 0);

    entry.append(&append_key_2, &append_value_2);

    assert_eq!(entry.len(), 256);
    assert_eq!(entry.key_view().size(), expected_key.size());
    assert_eq!(entry.value_view().size(), expected_value.size());
    assert!(
        max_abs_diff(&entry.key_view(), &expected_key) < 0.2,
        "TurboQuant repeated offload drifted too far on key cache"
    );
    assert!(
        max_abs_diff(&entry.value_view(), &expected_value) < 0.2,
        "TurboQuant repeated offload drifted too far on value cache"
    );
    assert_eq!(entry.quantized_prefix_len(), 256);
    assert!(entry.quantized_prefix_len() >= first_prefix_len);
    assert!(entry.compressed_prefix_bytes() >= first_prefix_bytes);
    assert_eq!(entry.dense_len, 0);
    assert_eq!(entry.dense_capacity(), 0);
}
#[test]
fn experimental_turboquant_generation_ready_copy_handles_multi_head_repeated_offload_state() {
    let key = make_kv_tensor_with_heads(2, 3, 128, 0.55);
    let value = make_kv_tensor_with_heads(2, 3, 128, -0.25);
    let append_key_1 = make_kv_tensor_with_heads(2, 3, 192, 0.55).narrow(2, 128, 64);
    let append_value_1 = make_kv_tensor_with_heads(2, 3, 192, -0.25).narrow(2, 128, 64);
    let append_key_2 = make_kv_tensor_with_heads(2, 3, 256, 0.55).narrow(2, 192, 64);
    let append_value_2 = make_kv_tensor_with_heads(2, 3, 256, -0.25).narrow(2, 192, 64);
    let expected_key = Tensor::cat(
        &[
            key.shallow_clone(),
            append_key_1.shallow_clone(),
            append_key_2.shallow_clone(),
        ],
        2,
    );
    let expected_value = Tensor::cat(
        &[
            value.shallow_clone(),
            append_value_1.shallow_clone(),
            append_value_2.shallow_clone(),
        ],
        2,
    );

    let mut entry = KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    );
    entry.append(&append_key_1, &append_value_1);
    entry.append(&append_key_2, &append_value_2);
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let copy = cache.deep_copy_generation_ready(48);
    let copied_entry = copy.layers[0].as_ref().expect("copied layer must exist");

    assert_eq!(copied_entry.quantized_prefix_len(), 256);
    assert_eq!(copied_entry.compressed_prefix_bytes(), entry.compressed_prefix_bytes());
    assert_eq!(copied_entry.dense_prefix_equivalent_bytes(), entry.dense_prefix_equivalent_bytes());
    assert_eq!(copied_entry.dense_len, 0);
    assert_eq!(copied_entry.len(), 256);
    assert_eq!(copied_entry.key_view().size(), expected_key.size());
    assert_eq!(copied_entry.value_view().size(), expected_value.size());
    assert!(
        max_abs_diff(&copied_entry.key_view(), &expected_key) < 0.2,
        "TurboQuant generation-ready copy drifted too far on key cache"
    );
    assert!(
        max_abs_diff(&copied_entry.value_view(), &expected_value) < 0.2,
        "TurboQuant generation-ready copy drifted too far on value cache"
    );
    assert_eq!(copied_entry.dense_capacity(), 48);
}
#[test]
fn experimental_turboquant_owned_generation_ready_preserves_multi_head_repeated_offload_state() {
    let key = make_kv_tensor_with_heads(2, 3, 128, 0.85);
    let value = make_kv_tensor_with_heads(2, 3, 128, -0.15);
    let append_key_1 = make_kv_tensor_with_heads(2, 3, 192, 0.85).narrow(2, 128, 64);
    let append_value_1 = make_kv_tensor_with_heads(2, 3, 192, -0.15).narrow(2, 128, 64);
    let append_key_2 = make_kv_tensor_with_heads(2, 3, 256, 0.85).narrow(2, 192, 64);
    let append_value_2 = make_kv_tensor_with_heads(2, 3, 256, -0.15).narrow(2, 192, 64);
    let expected_key = Tensor::cat(&[key.shallow_clone(), append_key_1.shallow_clone(), append_key_2.shallow_clone()], 2);
    let expected_value = Tensor::cat(&[value.shallow_clone(), append_value_1.shallow_clone(), append_value_2.shallow_clone()], 2);
    let mut entry = KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant);
    entry.append(&append_key_1, &append_value_1);
    entry.append(&append_key_2, &append_value_2);
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let moved = cache.into_generation_ready(48);
    let moved_entry = moved.layers[0].as_ref().expect("moved layer must exist");

    assert_eq!(moved_entry.quantized_prefix_len(), 256);
    assert_eq!(moved_entry.compressed_prefix_bytes(), entry.compressed_prefix_bytes());
    assert_eq!(moved_entry.dense_prefix_equivalent_bytes(), entry.dense_prefix_equivalent_bytes());
    assert_eq!(moved_entry.dense_len, 0);
    assert_eq!(moved_entry.dense_capacity(), 48);
    assert_eq!(moved_entry.len(), 256);
    assert_eq!(moved_entry.key_view().size(), expected_key.size());
    assert_eq!(moved_entry.value_view().size(), expected_value.size());
    assert!(max_abs_diff(&moved_entry.key_view(), &expected_key) < 0.2);
    assert!(max_abs_diff(&moved_entry.value_view(), &expected_value) < 0.2);
}
#[test]
fn experimental_turboquant_owned_generation_ready_handles_extreme_offload_transition() {
    let key = make_kv_tensor_with_heads(1, 2, 1024, 0.9);
    let value = make_kv_tensor_with_heads(1, 2, 1024, -0.9);
    let append_key = make_kv_tensor_with_heads(1, 2, 1056, 0.9).narrow(2, 1024, 32);
    let append_value = make_kv_tensor_with_heads(1, 2, 1056, -0.9).narrow(2, 1024, 32);
    let expected_key = Tensor::cat(&[key.shallow_clone(), append_key.shallow_clone()], 2);
    let expected_value = Tensor::cat(&[value.shallow_clone(), append_value.shallow_clone()], 2);

    let mut live_entry = KvCacheEntry::from_tokens(
        key.shallow_clone(),
        value.shallow_clone(),
        KvCacheMode::ExperimentalTurboQuant,
    );
    live_entry.append(&append_key, &append_value);
    assert_eq!(live_entry.dense_capacity(), 0);
    assert_eq!(live_entry.dense_len, 0);
    assert_eq!(live_entry.quantized_prefix_len(), 1056);

    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant));
    let mut cache = cache.into_generation_ready(0);
    let entry = cache.layers[0].as_mut().expect("moved layer must exist");
    entry.append(&append_key, &append_value);
    let entry = cache.layers[0].as_ref().expect("moved layer must exist");

    assert_eq!(entry.dense_capacity(), 0);
    assert_eq!(entry.dense_len, 0);
    assert_eq!(entry.quantized_prefix_len(), 1056);
    assert!(entry.compressed_prefix_bytes() > 0);
    assert_eq!(entry.len(), 1056);
    assert!(max_abs_diff(&entry.key_view(), &expected_key) < 0.2);
    assert!(max_abs_diff(&entry.value_view(), &expected_value) < 0.2);
}
