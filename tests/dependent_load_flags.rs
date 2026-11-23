//! The shipped exe must resolve its static imports only from System32, so a
//! planted DLL beside the portable exe is never loaded at process start.

const LOAD_LIBRARY_SEARCH_SYSTEM32: u16 = 0x800;
const LOAD_CONFIG_DIRECTORY: usize = 10;
const DEPENDENT_LOAD_FLAGS_OFFSET: usize = 0x4E;

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Maps a relative virtual address to its file offset through the section table.
fn file_offset(bytes: &[u8], sections: usize, count: usize, rva: u32) -> usize {
    (0..count)
        .map(|index| sections + index * 40)
        .find_map(|header| {
            let virtual_address = read_u32(bytes, header + 12);
            let raw_size = read_u32(bytes, header + 16);
            let raw_pointer = read_u32(bytes, header + 20);
            (virtual_address..virtual_address + raw_size)
                .contains(&rva)
                .then(|| (rva - virtual_address + raw_pointer) as usize)
        })
        .expect("load config RVA lies outside every section")
}

fn dependent_load_flags(bytes: &[u8]) -> u16 {
    let pe = read_u32(bytes, 0x3C) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
    let section_count = read_u16(bytes, pe + 6) as usize;
    let optional_size = read_u16(bytes, pe + 20) as usize;
    let optional = pe + 24;
    assert_eq!(read_u16(bytes, optional), 0x20B, "expected a PE32+ image");
    let directory = optional + 112 + LOAD_CONFIG_DIRECTORY * 8;
    let rva = read_u32(bytes, directory);
    let size = read_u32(bytes, directory + 4) as usize;
    assert!(
        size > DEPENDENT_LOAD_FLAGS_OFFSET,
        "the exe has no load config carrying DependentLoadFlags"
    );
    let config = file_offset(bytes, optional + optional_size, section_count, rva);
    read_u16(bytes, config + DEPENDENT_LOAD_FLAGS_OFFSET)
}

#[test]
fn static_imports_resolve_only_from_system32() {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_screen-goated-toolbox")).unwrap();
    assert_eq!(dependent_load_flags(&exe), LOAD_LIBRARY_SEARCH_SYSTEM32);
}
