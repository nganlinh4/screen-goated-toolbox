use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    ACE_HEADER, GetAce, GetSecurityDescriptorDacl, PSECURITY_DESCRIPTOR,
};
use windows::core::{BOOL, PCWSTR};

use super::{Kind, validate_descriptor};

const INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn new(sddl: &str) -> Self {
        let wide = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .unwrap_or_else(|error| panic!("Invalid descriptor fixture {sddl}: {error}"));
        Self(descriptor)
    }

    fn first_ace(&mut self) -> *mut ACE_HEADER {
        let mut present = BOOL::default();
        let mut defaulted = BOOL::default();
        let mut acl = std::ptr::null_mut();
        unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut acl, &mut defaulted) }
            .unwrap();
        assert!(present.as_bool() && !acl.is_null());
        let mut ace = std::ptr::null_mut();
        unsafe { GetAce(acl, 0, &mut ace) }.unwrap();
        ace.cast()
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        let _ = unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}

fn kinds() -> [(&'static str, Kind); 3] {
    [
        ("file", Kind::File),
        ("parent", Kind::Parent),
        ("ancestor", Kind::Ancestor),
    ]
}

fn check(sddl: &str, kind: Kind, allowed: bool, context: &str) {
    let descriptor = Descriptor::new(sddl);
    let result = unsafe { validate_descriptor(descriptor.0, kind) };
    assert_eq!(
        result.is_ok(),
        allowed,
        "{context}; descriptor {sddl}; result {result:?}"
    );
}

#[test]
fn every_mutation_right_is_rejected_independently() {
    let rights = [
        ("write data or add file", 0x0000_0002),
        ("write extended attributes", 0x0000_0010),
        ("write attributes", 0x0000_0100),
        ("delete", 0x0001_0000),
        ("write discretionary ACL", 0x0004_0000),
        ("write owner", 0x0008_0000),
    ];
    for (right, mask) in rights {
        for (kind_name, kind) in kinds() {
            check(
                &format!("O:SYD:(A;;0x{mask:08x};;;BU)"),
                kind,
                false,
                &format!("{kind_name}: {right}"),
            );
        }
    }
    for (kind_name, kind) in [("parent", Kind::Parent), ("ancestor", Kind::Ancestor)] {
        check(
            "O:SYD:(A;;0x00000040;;;BU)",
            kind,
            false,
            &format!("{kind_name}: delete child"),
        );
    }
}

#[test]
fn sibling_directory_creation_is_allowed_only_above_the_executable_parent() {
    let sddl = "O:SYD:(A;;0x00000004;;;BU)";
    check(sddl, Kind::File, false, "file append");
    check(sddl, Kind::Parent, false, "neighboring directory creation");
    check(
        sddl,
        Kind::Ancestor,
        true,
        "ancestor sibling directory creation",
    );
}

#[test]
fn generic_read_and_execute_do_not_grant_mutation() {
    for rights in ["GR", "GX", "GRGX", "FRFX"] {
        for (kind_name, kind) in kinds() {
            check(&format!("O:SYD:(A;;{rights};;;BU)"), kind, true, kind_name);
        }
    }
}

#[test]
fn generic_write_and_all_access_are_rejected() {
    for rights in ["GW", "GA", "FW", "FA"] {
        for (kind_name, kind) in kinds() {
            check(&format!("O:SYD:(A;;{rights};;;BU)"), kind, false, kind_name);
        }
    }
}

#[test]
fn unknown_and_request_only_access_bits_are_rejected() {
    for mask in [0x0000_0200_u32, 0x0100_0000, 0x0200_0000, 0x0400_0000] {
        for (kind_name, kind) in kinds() {
            check(
                &format!("O:SYD:(A;;0x{mask:08x};;;BU)"),
                kind,
                false,
                kind_name,
            );
        }
    }
}

#[test]
fn only_system_administrators_and_installer_owners_are_trusted() {
    for owner in ["SY", "BA", INSTALLER] {
        for (kind_name, kind) in kinds() {
            check(
                &format!("O:{owner}D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{INSTALLER})(A;;GRGX;;;BU)"),
                kind,
                true,
                kind_name,
            );
        }
    }
    for owner in ["BU", "S-1-5-21-100-200-300-1001"] {
        for (kind_name, kind) in kinds() {
            check(
                &format!("O:{owner}D:(A;;GRGX;;;BU)"),
                kind,
                false,
                kind_name,
            );
        }
    }
}

#[test]
fn owner_rights_depend_on_the_actual_owner() {
    for owner in ["SY", "BA", INSTALLER, "S-1-5-21-100-200-300-1001"] {
        for (kind_name, kind) in kinds() {
            check(
                &format!("O:{owner}D:(A;;GA;;;S-1-3-4)"),
                kind,
                owner != "S-1-5-21-100-200-300-1001",
                kind_name,
            );
        }
    }
}

#[test]
fn absent_or_null_dacls_are_rejected_but_a_trusted_empty_dacl_is_safe() {
    for sddl in ["O:SY", "O:SYD:NO_ACCESS_CONTROL", "D:(A;;GR;;;BU)"] {
        for (kind_name, kind) in kinds() {
            check(sddl, kind, false, kind_name);
        }
    }
    for (kind_name, kind) in kinds() {
        check("O:SYD:", kind, true, kind_name);
    }
}

#[test]
fn inheritance_only_grants_do_not_apply_to_the_current_object() {
    for (kind_name, kind) in kinds() {
        check("O:SYD:(A;OICIIO;GA;;;BU)", kind, true, kind_name);
    }
    for (kind_name, kind) in kinds() {
        check("O:SYD:(A;ID;GW;;;BU)", kind, false, kind_name);
    }
}

#[test]
fn deny_entries_do_not_excuse_an_untrusted_mutation_grant() {
    for sddl in [
        "O:SYD:(D;;GA;;;BU)(A;;GW;;;BU)",
        "O:SYD:(A;;GW;;;BU)(D;;GA;;;BU)",
    ] {
        for (kind_name, kind) in kinds() {
            check(sddl, kind, false, kind_name);
        }
    }
    for (kind_name, kind) in kinds() {
        check("O:SYD:(D;;GA;;;BU)(A;;GRGX;;;BU)", kind, true, kind_name);
    }
}

#[test]
fn unsupported_callback_and_object_aces_fail_closed() {
    for sddl in [
        "O:SYD:(XA;;GR;;;BU;(Member_of {SID(S-1-5-32-545)}))",
        "O:SYD:(OA;;GR;00000000-0000-0000-0000-000000000001;;BU)",
    ] {
        for (kind_name, kind) in kinds() {
            check(sddl, kind, false, kind_name);
        }
    }
}

#[test]
fn unknown_ace_types_fail_closed() {
    for (kind_name, kind) in kinds() {
        let mut descriptor = Descriptor::new("O:SYD:(A;;GR;;;BU)");
        unsafe { (*descriptor.first_ace()).AceType = 0x7f };
        let result = unsafe { validate_descriptor(descriptor.0, kind) };
        assert!(result.is_err(), "{kind_name}: unknown ACE: {result:?}");
    }
}
