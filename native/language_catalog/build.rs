use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use reference_isolang::Language;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=THIRD-PARTY-NOTICES.txt");

    let notice = fs::read_to_string("THIRD-PARTY-NOTICES.txt").unwrap();
    assert!(notice.contains("isolang 2.4.0"));
    assert!(notice.contains("Apache License, Version 2.0"));

    let languages = (0..=u16::MAX as usize)
        .map_while(Language::from_usize)
        .collect::<Vec<_>>();
    assert!(!languages.is_empty());
    assert!(languages.len() <= u16::MAX as usize);

    let mut payload = Vec::new();
    payload.extend_from_slice(&(languages.len() as u16).to_le_bytes());
    let mut previous_code = None;
    for language in languages {
        let code_3 = language.to_639_3().as_bytes();
        assert_eq!(code_3.len(), 3);
        if let Some(previous) = previous_code {
            assert!(previous < code_3);
        }
        previous_code = Some(code_3);
        payload.extend_from_slice(code_3);

        match language.to_639_1() {
            Some(code_1) => {
                assert_eq!(code_1.len(), 2);
                payload.extend_from_slice(code_1.as_bytes());
            }
            None => payload.extend_from_slice(&[0, 0]),
        }

        let name = language.to_name().as_bytes();
        let name_len = u16::try_from(name.len()).expect("ISO language name is too long");
        payload.extend_from_slice(&name_len.to_le_bytes());
        payload.extend_from_slice(name);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&payload).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < payload.len());

    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("languages.zlib");
    fs::write(output, compressed).unwrap();
}
