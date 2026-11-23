use super::*;
use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_SUCCESS};
use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{DACL_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION};
use windows::Win32::Storage::FileSystem::READ_CONTROL;
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_ACCESS_RIGHTS};

const BASE: &str =
    "Global\\ScreenGoatedToolboxSingleInstanceMutex-0123456789abcdef0123456789abcdef";

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[test]
fn names_are_scoped_to_one_user_and_reject_unsafe_input() {
    let sid = "S-1-5-21-1-2-3-1001";
    let names = instance_names(&wide(BASE), sid).unwrap();
    assert_eq!(names.mutex, format!("{BASE}-{sid}"));
    assert_eq!(
        names.pipe,
        format!(
            r"\\.\pipe\ScreenGoatedToolboxSingleInstanceMutex-0123456789abcdef0123456789abcdef-{sid}-activation"
        )
    );
    let other_user = instance_names(&wide(BASE), "S-1-5-21-1-2-3-1002").unwrap();
    assert_ne!(names.mutex, other_user.mutex);
    assert_ne!(names.pipe, other_user.pipe);
    for sid in [
        "",
        "S-1-",
        "X-1-5-18",
        "S-1-5\\18",
        "S-1-5-18-a",
        "S-1-5/18",
    ] {
        assert!(instance_names(&wide(BASE), sid).is_err(), "{sid:?}");
    }
    for base in [
        "Local\\Name",
        "Global\\",
        "Global\\a\\b",
        "Global\\a/b",
        "Name",
    ] {
        assert!(instance_names(&wide(base), "S-1-5-18").is_err(), "{base:?}");
    }
}

#[test]
fn singleton_grants_only_the_token_user_and_labels_medium_no_write_up() {
    let sid = current_user_sid().unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("Local\\sgt-singleton-test-{}-{nonce}", std::process::id());
    let security = ObjectSecurity::for_mutex(&sid).unwrap();

    let (_owner, existed) = create_singleton_mutex(&name, &security).unwrap();
    assert!(!existed);
    // A later launch opens the same object with only the granted rights.
    let (_later, existed) = create_singleton_mutex(&name, &security).unwrap();
    assert!(existed);
    // The previous full-access request is refused: the explicit DACL is applied.
    let full = unsafe { CreateMutexW(None, false, PCWSTR(wide(&name).as_ptr())) };
    assert_eq!(
        full.unwrap_err().code(),
        ERROR_ACCESS_DENIED.to_hresult(),
        "explicit descriptor was not applied"
    );

    let reader = unsafe {
        OpenMutexW(
            SYNCHRONIZATION_ACCESS_RIGHTS(READ_CONTROL.0),
            false,
            PCWSTR(wide(&name).as_ptr()),
        )
    }
    .unwrap();
    let information = DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetSecurityInfo(
            reader,
            SE_KERNEL_OBJECT,
            information,
            None,
            None,
            None,
            None,
            Some(&mut descriptor),
        )
    };
    assert_eq!(status, ERROR_SUCCESS);
    let mut text = PWSTR::null();
    let applied = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            information,
            &mut text,
            None,
        )
        .unwrap();
        let applied = String::from_utf16(text.as_wide()).unwrap();
        LocalFree(Some(HLOCAL(text.0.cast())));
        LocalFree(Some(HLOCAL(descriptor.0)));
        let _ = CloseHandle(reader);
        applied
    };
    // The kernel reports the label's SACL as auto-inherited; its ACE is unchanged.
    assert_eq!(
        applied,
        format!("D:P(A;;0x100001;;;{sid})S:AI(ML;;NW;;;ME)")
    );
}
