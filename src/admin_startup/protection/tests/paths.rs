use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use super::super::{Input, Namespace, reserved_name, same_path, valid_guid};

const GUID: &str = "12345678-9abc-def0-1234-56789abcdef0";

fn rejected(raw: &str) {
    assert!(
        Input::parse(Path::new(raw)).is_err(),
        "ambiguous path was accepted: {raw:?}"
    );
}

#[test]
fn drive_roots_and_targets_share_one_extended_namespace() {
    for raw in [r"C:\", r"\\?\C:\"] {
        let input = Input::parse(Path::new(raw)).unwrap();
        assert!(matches!(input.namespace, Namespace::Drive));
        assert_eq!(input.root, Path::new(r"\\?\C:\"));
        assert_eq!(input.path, input.root);
        assert!(input.parts.is_empty());
    }
    for raw in [r"C:\tools\target.exe", r"\\?\C:\tools\target.exe"] {
        let input = Input::parse(Path::new(raw)).unwrap();
        assert!(matches!(input.namespace, Namespace::Drive));
        assert_eq!(input.root, Path::new(r"\\?\C:\"));
        assert_eq!(input.path, Path::new(r"\\?\C:\tools\target.exe"));
        assert_eq!(
            input.parts,
            vec!["tools".to_owned(), "target.exe".to_owned()]
        );
    }
    let input = Input::parse(Path::new(r"z:\tools\target.exe")).unwrap();
    assert!(same_path(
        &input.path,
        Path::new(r"\\?\Z:\TOOLS\TARGET.EXE")
    ));
}

#[test]
fn volume_roots_and_targets_preserve_their_namespace() {
    for guid in [GUID.to_owned(), GUID.to_ascii_uppercase()] {
        let root = format!(r"\\?\Volume{{{guid}}}\");
        let input = Input::parse(Path::new(&root)).unwrap();
        assert!(matches!(input.namespace, Namespace::Volume));
        assert_eq!(input.root, Path::new(&root));
        assert_eq!(input.path, input.root);
        assert!(input.parts.is_empty());

        let raw = format!(r"{root}tools\target.exe");
        let input = Input::parse(Path::new(&raw)).unwrap();
        assert!(matches!(input.namespace, Namespace::Volume));
        assert_eq!(input.root, Path::new(&root));
        assert_eq!(input.path, Path::new(&raw));
        assert_eq!(
            input.parts,
            vec!["tools".to_owned(), "target.exe".to_owned()]
        );
    }
}

#[test]
fn guid_validation_requires_exact_ascii_hex_and_hyphen_positions() {
    assert!(valid_guid(GUID));
    assert!(valid_guid(&GUID.to_ascii_uppercase()));
    assert!(valid_guid("00000000-0000-0000-0000-000000000000"));
    for value in [
        "",
        "12345678-9abc-def0-1234-56789abcdef",
        "12345678-9abc-def0-1234-56789abcdef00",
        "123456789-abc-def0-1234-56789abcdef0",
        "12345678_9abc-def0-1234-56789abcdef0",
        "12345678-9abc-def0-1234-56789abcdeg0",
        "{12345678-9abc-def0-1234-56789abcdef0}",
        "１２３４５６７８-9abc-def0-1234-56789abcdef0",
    ] {
        assert!(!valid_guid(value), "invalid GUID was accepted: {value:?}");
    }
    for raw in [
        r"\\?\Volume{}\target.exe",
        r"\\?\Volume{12345678_9abc-def0-1234-56789abcdef0}\target.exe",
        r"\\?\Volume{12345678-9abc-def0-1234-56789abcdeg0}\target.exe",
        r"\\?\Volume{12345678-9abc-def0-1234-56789abcdef0}target.exe",
    ] {
        rejected(raw);
    }
}

#[test]
fn only_absolute_drive_and_volume_namespaces_are_accepted_lexically() {
    for raw in [
        "",
        r"target.exe",
        r"C:target.exe",
        r"\tools\target.exe",
        r"\\server\share\target.exe",
        r"\\?\UNC\server\share\target.exe",
        r"\\.\C:\tools\target.exe",
        r"\\.\PhysicalDrive0",
        r"\??\C:\tools\target.exe",
        r"\Device\HarddiskVolume1\tools\target.exe",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\target.exe",
        r"1:\tools\target.exe",
        r"C:/tools/target.exe",
        r"C:\tools/target.exe",
    ] {
        rejected(raw);
    }
}

#[test]
fn alternate_streams_empty_components_and_dot_aliases_are_rejected() {
    for tail in [
        r"tools\target.exe:stream",
        r"tools\target.exe::$DATA",
        r"tools:stream\target.exe",
        r"tools\\target.exe",
        r"tools\target.exe\",
        r"\target.exe",
        r".\target.exe",
        r"..\target.exe",
        r"tools\.\target.exe",
        r"tools\..\target.exe",
        r"tools \target.exe",
        r"tools.\target.exe",
        r"tools\target.exe ",
        r"tools\target.exe.",
    ] {
        for root in [r"C:\".to_owned(), format!(r"\\?\Volume{{{GUID}}}\")] {
            rejected(&format!("{root}{tail}"));
        }
    }
}

#[test]
fn invalid_filename_characters_and_ascii_controls_are_rejected() {
    for character in ['<', '>', ':', '"', '/', '|', '?', '*'] {
        rejected(&format!(r"C:\tools\target{character}.exe"));
    }
    for control in 0_u8..32 {
        rejected(&format!(r"C:\tools\target{}.exe", char::from(control)));
    }
    let raw = OsString::from_wide(&[b'C' as u16, b':' as u16, b'\\' as u16, 0xd800]);
    assert!(Input::parse(&PathBuf::from(raw)).is_err());
}

#[test]
fn device_names_are_rejected_with_case_variants_and_extensions() {
    let mut names = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .map(str::to_owned)
        .to_vec();
    for prefix in ["COM", "LPT"] {
        for suffix in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"] {
            names.push(format!("{prefix}{suffix}"));
        }
    }
    for name in names {
        for candidate in [
            name.clone(),
            format!("{name}.exe"),
            format!("{}.txt", name.to_ascii_lowercase()),
        ] {
            assert!(reserved_name(&candidate), "device name: {candidate}");
            rejected(&format!(r"C:\tools\{candidate}"));
            rejected(&format!(r"C:\{candidate}\target.exe"));
        }
    }
    for name in [
        "console.exe",
        "COM0",
        "COM10",
        "LPT0",
        "LPT10",
        "ordinary.con",
    ] {
        assert!(!reserved_name(name), "ordinary name: {name}");
        assert!(Input::parse(Path::new(&format!(r"C:\tools\{name}"))).is_ok());
    }
}

#[test]
fn unicode_filename_components_are_preserved() {
    let raw = r"C:\Ä-Ω-가\é-中.exe";
    let input = Input::parse(Path::new(raw)).unwrap();
    assert_eq!(
        input.parts,
        vec!["Ä-Ω-가".to_owned(), "é-中.exe".to_owned()]
    );
    assert_eq!(input.path, Path::new(r"\\?\C:\Ä-Ω-가\é-中.exe"));
}

#[test]
fn path_comparison_uses_ordinal_case_insensitivity_without_alias_rewriting() {
    assert!(same_path(
        Path::new(r"\\?\C:\Tools\Target.exe"),
        Path::new(r"\\?\c:\tools\TARGET.EXE"),
    ));
    assert!(same_path(
        Path::new(r"\\?\C:\Ä\Ω.exe"),
        Path::new(r"\\?\c:\ä\ω.EXE"),
    ));
    for other in [
        r"\\?\C:\tools\other.exe",
        r"\\?\C:\tools\nested\target.exe",
        r"\\?\D:\tools\target.exe",
        r"C:\tools\target.exe",
        r"\\?\C:\tools\.\target.exe",
    ] {
        assert!(!same_path(
            Path::new(r"\\?\C:\tools\target.exe"),
            Path::new(other)
        ));
    }
}
