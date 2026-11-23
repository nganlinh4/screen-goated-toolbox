use sha2::{Digest as _, Sha256};

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Lowercase hex SHA-256 of everything `reader` yields, read in fixed chunks
/// so a multi-gigabyte file never has to fit in memory.
pub(crate) fn sha256_reader(reader: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    #[test]
    fn content_hash_matches_standard_empty_and_ascii_vectors() {
        assert_eq!(
            super::sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            super::sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn streamed_hash_spans_chunks_and_matches_the_in_memory_hash() {
        let bytes: Vec<u8> = (0..600_000_u32).map(|value| value as u8).collect();
        let streamed = super::sha256_reader(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(streamed, super::sha256_hex(&bytes));
        assert_eq!(
            super::sha256_reader(&mut std::io::empty()).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
