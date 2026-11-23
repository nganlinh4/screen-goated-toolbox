use super::*;
use windows::Win32::Foundation::{E_ACCESSDENIED, E_FAIL};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, AclSizeInformation, GetAce, GetAclInformation,
    GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    LookupAccountSidW, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
};
use windows::Win32::Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ};
use windows::Win32::System::TaskScheduler::TASK_CREATE_OR_UPDATE;

fn return_bstr(value: BSTR) -> impl FnOnce(*mut BSTR) -> windows::core::Result<()> {
    move |out| {
        unsafe { *out = value };
        Ok(())
    }
}

#[test]
fn native_paths_preserve_unicode_metacharacters_and_unpaired_surrogates() {
    let mut wide: Vec<u16> = "C:\\개인 폴더\\app & % '!.exe".encode_utf16().collect();
    wide.insert(4, 0xD800);
    let value = PathBuf::from(OsString::from_wide(&wide));
    let encoded = native_path(&value).unwrap();
    assert_eq!(&*encoded, wide);
    assert_eq!(path(return_bstr(encoded)).unwrap(), value);
}

#[test]
fn outgoing_task_paths_obey_the_conservative_native_length_limit_before_registration() {
    for supplementary in [false, true] {
        let suffix = if supplementary { "😀.exe" } else { ".exe" };
        let suffix_units = suffix.encode_utf16().count();
        for extra in [0, 1] {
            let value = format!(
                r"C:\{}{suffix}",
                "a".repeat(MAX_TASK_PATH_UNITS - 3 - suffix_units + extra)
            );
            let result = native_path(Path::new(&value));
            if extra == 0 {
                assert_eq!(result.unwrap().len(), MAX_TASK_PATH_UNITS);
            } else {
                assert_eq!(
                    result.unwrap_err().downcast_ref::<super::super::Issue>(),
                    Some(&super::super::Issue::PathTooLong)
                );
            }
        }
    }
}

#[test]
fn malformed_text_and_embedded_nulls_are_query_errors() {
    assert!(text(return_bstr(BSTR::from_wide(&[0xD800]))).is_err());
    assert!(text(return_bstr(BSTR::from_wide(&[65, 0, 66]))).is_err());
    assert!(path(return_bstr(BSTR::from_wide(&[65, 0, 66]))).is_err());
    assert!(text(return_bstr(BSTR::from_wide(&vec![65; MAX_TEXT_UNITS + 1]))).is_err());
    assert!(text(|_| Err(windows::core::Error::from_hresult(E_FAIL))).is_err());
}

#[test]
fn only_file_not_found_is_an_absent_task() {
    assert!(task_missing(HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0)));
    assert!(!task_missing(E_ACCESSDENIED));
    assert!(!task_missing(E_FAIL));
    assert!(!task_missing(HRESULT::from_win32(3)));
}

#[test]
fn registration_never_implicitly_replaces_or_creates_and_preserves_principal_acl() {
    let create = registration_flags(false);
    let update = registration_flags(true);
    assert_ne!(create & TASK_DONT_ADD_PRINCIPAL_ACE.0, 0);
    assert_ne!(update & TASK_DONT_ADD_PRINCIPAL_ACE.0, 0);
    assert_eq!(create & TASK_CREATE_OR_UPDATE.0, TASK_CREATE.0);
    assert_eq!(update & TASK_CREATE_OR_UPDATE.0, TASK_UPDATE.0);
}

#[test]
fn all_zero_schema_duration_components_are_plain_logon_timing() {
    for value in [
        "",
        "P0Y",
        "P000M",
        "P0D",
        "PT0H",
        "PT0M",
        "PT0S",
        "PT00.000S",
        "P0Y0M0DT0H0M0.000S",
        "P00000Y0MT0M",
        "-P0Y",
        "-PT0.000S",
        "\tP0DT0H\r\n",
    ] {
        assert!(zero_duration(value), "Zero duration rejected: {value:?}");
    }
}

#[test]
fn every_optional_zero_duration_component_combination_is_supported() {
    let units = ['Y', 'M', 'D', 'H', 'M', 'S'];
    for mask in 1..64 {
        let mut duration = String::from("P");
        for (index, unit) in units.iter().enumerate() {
            if index == 3 && mask & 0b111000 != 0 {
                duration.push('T');
            }
            if mask & (1 << index) != 0 {
                duration.push_str(&format!("00{unit}"));
            }
        }
        assert!(
            zero_duration(&duration),
            "Zero duration rejected: {duration}"
        );
    }
}

#[test]
fn nonzero_durations_are_never_rounded_or_overflowed_to_zero() {
    for value in [
        "P1Y",
        "P1M",
        "P1D",
        "PT1H",
        "PT1M",
        "PT1S",
        "PT0.00000000000000000001S",
        "P0Y0M0DT0H0M1S",
        "-PT1S",
        "P999999999999999999999999999999999999999999Y",
    ] {
        assert!(
            !zero_duration(value),
            "Nonzero duration accepted: {value:?}"
        );
    }
}

#[test]
fn malformed_or_unsupported_duration_forms_are_not_plain_logon_timing() {
    for value in [
        "P", "PT", "P0YT", "P0Y0Y", "P0D0M", "PT0M0H", "PT0S0M", "PT0.0H", "P0.0D", "PT.0S",
        "PT0.S", "PT0,0S", "P0W", "P000", "0", "0S", "PT-0S", "--P0D", "P+0D", "+P0D", "P0DT0MTS",
        "P0D T0H", "pt0s", "Ｐ0D", "PT٠S", " \t ", " P T0S ", "PT0S\0",
    ] {
        assert!(
            !zero_duration(value),
            "Invalid duration accepted: {value:?}"
        );
    }
}

#[test]
fn current_token_sid_round_trips_without_scheduler_access() {
    let sid = current_user_sid().unwrap();
    assert_eq!(resolve_principal_sid(&sid).unwrap(), sid);
    assert_eq!(parse_sid("SY").unwrap(), "S-1-5-18");
    assert!(parse_sid("S-1-invalid").is_err());
    assert!(parse_sid("S-1-5-18\0SY").is_err());
}

#[test]
fn local_principal_names_resolve_to_the_same_sid_as_sid_strings() {
    let wide = wide_text("S-1-5-18").unwrap();
    let mut sid = PSID::default();
    unsafe { ConvertStringSidToSidW(PCWSTR(wide.as_ptr()), &mut sid) }.unwrap();
    let _allocation = LocalAllocation(sid.0);
    let mut name = [0_u16; 256];
    let mut domain = [0_u16; 256];
    let mut name_length = name.len() as u32;
    let mut domain_length = domain.len() as u32;
    let mut usage = SID_NAME_USE::default();
    unsafe {
        LookupAccountSidW(
            None,
            sid,
            Some(PWSTR(name.as_mut_ptr())),
            &mut name_length,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_length,
            &mut usage,
        )
    }
    .unwrap();
    let name = String::from_utf16(&name[..name_length as usize]).unwrap();
    let domain = String::from_utf16(&domain[..domain_length as usize]).unwrap();
    assert_eq!(
        resolve_principal_sid(&format!("{domain}\\{name}")).unwrap(),
        "S-1-5-18"
    );
}

#[test]
fn task_descriptor_has_protected_dacl_and_only_read_execute_for_current_user() {
    let user = "S-1-5-21-100-200-300-1001".to_string();
    let descriptor = wide_text(&task_descriptor(&user)).unwrap();
    let mut security = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(descriptor.as_ptr()),
            SDDL_REVISION_1,
            &mut security,
            None,
        )
        .unwrap();
        let _allocation = LocalAllocation(security.0);
        let mut owner = PSID::default();
        let mut defaulted = windows::core::BOOL::default();
        GetSecurityDescriptorOwner(security, &mut owner, &mut defaulted).unwrap();
        assert_eq!(sid_text(owner).unwrap(), "S-1-5-32-544");
        let mut control = Default::default();
        let mut revision = 0;
        GetSecurityDescriptorControl(security, &mut control, &mut revision).unwrap();
        assert_ne!(control & SE_DACL_PROTECTED.0, 0);
        let mut present = windows::core::BOOL::default();
        let mut acl = std::ptr::null_mut();
        GetSecurityDescriptorDacl(security, &mut present, &mut acl, &mut defaulted).unwrap();
        assert!(present.as_bool());
        assert!(!acl.is_null());
        let mut information = ACL_SIZE_INFORMATION::default();
        GetAclInformation(
            acl,
            &mut information as *mut _ as *mut _,
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
        .unwrap();
        assert_eq!(information.AceCount, 3);
        for index in 0..information.AceCount {
            let mut raw = std::ptr::null_mut();
            GetAce(acl, index, &mut raw).unwrap();
            let ace = &*raw.cast::<ACCESS_ALLOWED_ACE>();
            assert_eq!(ace.Header.AceType, 0);
            let sid = sid_text(PSID((&ace.SidStart as *const u32).cast_mut().cast())).unwrap();
            if sid == user {
                assert_eq!(ace.Mask, (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE).0);
            } else {
                assert!(sid == "S-1-5-18" || sid == "S-1-5-32-544");
            }
        }
    }
}
