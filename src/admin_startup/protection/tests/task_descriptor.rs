use super::super::validate_task_descriptor;

const USER: &str = "S-1-5-21-100-200-300-1001";
const PROTECTED: &str = "O:BAG:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;S-1-5-21-100-200-300-1001)";

#[test]
fn administrators_keep_control_while_the_user_can_read_and_run() {
    validate_task_descriptor(PROTECTED).unwrap();
}

#[test]
fn every_untrusted_file_mutation_right_is_rejected() {
    for mask in [
        0x0000_0002_u32,
        0x0000_0004,
        0x0000_0010,
        0x0000_0100,
        0x0001_0000,
        0x0004_0000,
        0x0008_0000,
    ] {
        let descriptor = format!("O:BAG:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x{mask:08x};;;{USER})");
        assert!(
            validate_task_descriptor(&descriptor).is_err(),
            "untrusted mutation mask 0x{mask:08x} was accepted"
        );
    }
}

#[test]
fn generic_and_file_write_grants_are_rejected_for_untrusted_principals() {
    for rights in ["GW", "GA", "FW", "FA"] {
        for principal in [USER, "BU", "AU", "WD"] {
            let descriptor = format!("O:BAD:P(A;;FA;;;BA)(A;;{rights};;;{principal})");
            assert!(
                validate_task_descriptor(&descriptor).is_err(),
                "{principal} received {rights}"
            );
        }
    }
}

#[test]
fn an_untrusted_owner_is_rejected_even_with_only_read_and_execute_grants() {
    for owner in [USER, "BU", "WD"] {
        let descriptor = format!("O:{owner}D:P(A;;FRFX;;;{USER})");
        assert!(validate_task_descriptor(&descriptor).is_err());
    }
}

#[test]
fn absent_and_null_dacls_are_rejected() {
    for descriptor in ["O:BA", "O:BAD:NO_ACCESS_CONTROL", "D:P(A;;FRFX;;;BU)"] {
        assert!(
            validate_task_descriptor(descriptor).is_err(),
            "unprotected descriptor was accepted: {descriptor}"
        );
    }
}

#[test]
fn invalid_security_descriptor_strings_are_rejected() {
    for descriptor in [
        "",
        "invalid",
        "O:",
        "O:BAD:(A;;FA;;;not-a-sid)",
        "O:BAD:(A;;FA;;;BA",
    ] {
        assert!(validate_task_descriptor(descriptor).is_err());
    }
}

#[test]
fn embedded_nuls_cannot_truncate_the_security_policy() {
    for descriptor in [
        format!("{PROTECTED}\0"),
        format!("{PROTECTED}\0D:(A;;FA;;;WD)"),
        "O:BA\0D:P(A;;FA;;;BA)".to_owned(),
    ] {
        assert!(validate_task_descriptor(&descriptor).is_err());
    }
}
