//! Environment switches available only to debug and test harness builds.

#[cfg(any(debug_assertions, test))]
pub(super) fn dry_run_requested() -> bool {
    std::env::var_os("CC_DRY").is_some()
}

#[cfg(not(any(debug_assertions, test)))]
pub(super) const fn dry_run_requested() -> bool {
    false
}

#[cfg(any(debug_assertions, test))]
pub(super) fn skip_locate_verification_requested() -> bool {
    std::env::var("CC_VERIFY_LOCATE").as_deref() == Ok("0")
}

#[cfg(not(any(debug_assertions, test)))]
pub(super) const fn skip_locate_verification_requested() -> bool {
    false
}

#[cfg(not(any(debug_assertions, test)))]
const _: () = {
    assert!(!dry_run_requested());
    assert!(!skip_locate_verification_requested());
};

#[cfg(test)]
mod tests {
    #[test]
    fn test_build_compiles_harness_environment_switches() {
        let _ = super::dry_run_requested();
        let _ = super::skip_locate_verification_requested();
    }
}
