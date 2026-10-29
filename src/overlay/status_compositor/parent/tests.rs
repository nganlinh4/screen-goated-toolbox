use super::*;

#[test]
fn restart_backoff_is_exponential_and_bounded() {
    assert_eq!(restart_delay_ms(1), 250);
    assert_eq!(restart_delay_ms(2), 500);
    assert_eq!(restart_delay_ms(3), 1_000);
    assert_eq!(restart_delay_ms(8), 30_000);
    assert_eq!(restart_delay_ms(u32::MAX), 30_000);
}

#[test]
fn software_fallback_preserves_existing_browser_arguments() {
    let arguments = browser_arguments_with_software_fallback(Some(OsString::from("--foo")));
    assert!(arguments.to_string_lossy().contains("--foo"));
    assert!(
        arguments
            .to_string_lossy()
            .contains(SOFTWARE_RENDERING_ARGUMENT)
    );
}
