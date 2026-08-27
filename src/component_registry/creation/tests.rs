use super::*;

#[test]
fn embedded_contract_has_one_complete_product_inventory() {
    let delivery = CREATION_DELIVERY.as_ref().unwrap();
    assert_eq!(delivery.files.len(), 4);
    assert!(delivery.files.iter().any(|file| file.path == RUNTIME_PATH));
    assert_eq!(
        delivery
            .files
            .iter()
            .map(|file| file.size_bytes)
            .sum::<u64>(),
        delivery.unpacked_size_bytes
    );
}

#[test]
fn web_reader_cannot_escape_its_archive_partition() {
    assert!(validate_relative_path(Path::new("../bin/sgt_creation_runtime.exe")).is_err());
}
