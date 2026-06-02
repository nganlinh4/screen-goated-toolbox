use super::{make_kv_tensor_with_heads, max_abs_diff};
use crate::tensor::Tensor;
use crate::text_decoder::{KvCache, KvCacheEntry, KvCacheMode};

#[test]
fn compressed_kv_mutable_offload_handles_multi_head_repeated_appends() {
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

    let mut entry = KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant);
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
    assert_eq!(entry.dense_len(), 0);
    assert_eq!(entry.dense_capacity(), 0);
}

#[test]
fn compressed_kv_generation_ready_copy_handles_multi_head_repeated_offload_state() {
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

    let mut entry = KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant);
    entry.append(&append_key_1, &append_value_1);
    entry.append(&append_key_2, &append_value_2);
    let expected_compressed_prefix_bytes = entry.compressed_prefix_bytes();
    let expected_dense_prefix_equivalent_bytes = entry.dense_prefix_equivalent_bytes();
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let copy = cache.deep_copy_generation_ready(48);
    let copied_entry = copy.layers[0].as_ref().expect("copied layer must exist");

    assert_eq!(copied_entry.quantized_prefix_len(), 256);
    assert_eq!(
        copied_entry.compressed_prefix_bytes(),
        expected_compressed_prefix_bytes
    );
    assert_eq!(
        copied_entry.dense_prefix_equivalent_bytes(),
        expected_dense_prefix_equivalent_bytes
    );
    assert_eq!(copied_entry.dense_len(), 0);
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
fn compressed_kv_owned_generation_ready_preserves_multi_head_repeated_offload_state() {
    let key = make_kv_tensor_with_heads(2, 3, 128, 0.85);
    let value = make_kv_tensor_with_heads(2, 3, 128, -0.15);
    let append_key_1 = make_kv_tensor_with_heads(2, 3, 192, 0.85).narrow(2, 128, 64);
    let append_value_1 = make_kv_tensor_with_heads(2, 3, 192, -0.15).narrow(2, 128, 64);
    let append_key_2 = make_kv_tensor_with_heads(2, 3, 256, 0.85).narrow(2, 192, 64);
    let append_value_2 = make_kv_tensor_with_heads(2, 3, 256, -0.15).narrow(2, 192, 64);
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
    let mut entry = KvCacheEntry::from_tokens(key, value, KvCacheMode::ExperimentalTurboQuant);
    entry.append(&append_key_1, &append_value_1);
    entry.append(&append_key_2, &append_value_2);
    let expected_compressed_prefix_bytes = entry.compressed_prefix_bytes();
    let expected_dense_prefix_equivalent_bytes = entry.dense_prefix_equivalent_bytes();
    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(entry);
    let moved = cache.into_generation_ready(48);
    let moved_entry = moved.layers[0].as_ref().expect("moved layer must exist");

    assert_eq!(moved_entry.quantized_prefix_len(), 256);
    assert_eq!(
        moved_entry.compressed_prefix_bytes(),
        expected_compressed_prefix_bytes
    );
    assert_eq!(
        moved_entry.dense_prefix_equivalent_bytes(),
        expected_dense_prefix_equivalent_bytes
    );
    assert_eq!(moved_entry.dense_len(), 0);
    assert_eq!(moved_entry.dense_capacity(), 48);
    assert_eq!(moved_entry.len(), 256);
    assert_eq!(moved_entry.key_view().size(), expected_key.size());
    assert_eq!(moved_entry.value_view().size(), expected_value.size());
    assert!(max_abs_diff(&moved_entry.key_view(), &expected_key) < 0.2);
    assert!(max_abs_diff(&moved_entry.value_view(), &expected_value) < 0.2);
}

#[test]
fn compressed_kv_owned_generation_ready_handles_extreme_offload_transition() {
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
    assert_eq!(live_entry.dense_len(), 0);
    assert_eq!(live_entry.quantized_prefix_len(), 1056);

    let mut cache = KvCache::new(1, KvCacheMode::ExperimentalTurboQuant);
    cache.layers[0] = Some(KvCacheEntry::from_tokens(
        key,
        value,
        KvCacheMode::ExperimentalTurboQuant,
    ));
    let mut cache = cache.into_generation_ready(0);
    let entry = cache.layers[0].as_mut().expect("moved layer must exist");
    entry.append(&append_key, &append_value);
    let entry = cache.layers[0].as_ref().expect("moved layer must exist");

    assert_eq!(entry.dense_capacity(), 0);
    assert_eq!(entry.dense_len(), 0);
    assert_eq!(entry.quantized_prefix_len(), 1056);
    assert!(entry.compressed_prefix_bytes() > 0);
    assert_eq!(entry.len(), 1056);
    assert!(max_abs_diff(&entry.key_view(), &expected_key) < 0.2);
    assert!(max_abs_diff(&entry.value_view(), &expected_value) < 0.2);
}
