//! Delivery compatibility describes the complete worker messages and model paths.
//!
//! Only the streaming worker contract is supported. The original request/response
//! detector (`sgtd-v4`, last shipped as 3.2.18) is retired: neither the build-time
//! contract nor a signed update can select it.

const STREAM: &str = "sgts-v1";
const RETIRED: &str = "sgtd-v4";
/// Shipped before deliveries declared their protocol.
const UNDECLARED_STREAM_VERSION: &str = "4.2.2";

pub(crate) fn validate(version: &str, declared: Option<&str>) -> Result<(), &'static str> {
    match declared {
        Some(STREAM) => Ok(()),
        Some(RETIRED) => Err("the legacy screen-text detector protocol is retired"),
        Some(_) => Err("unsupported screen-text detector protocol"),
        None if version == UNDECLARED_STREAM_VERSION => Ok(()),
        None => Err("screen-text detector delivery must declare its protocol"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_versions_require_the_declared_stream_protocol() {
        assert!(validate("5.0.0", None).is_err());
        assert!(validate("5.0.0", Some("sgts-v2")).is_err());
        assert_eq!(validate("5.0.0", Some("sgts-v1")), Ok(()));
    }

    #[test]
    fn only_the_shipped_stream_contract_migrates_without_metadata() {
        assert_eq!(validate("4.2.2", None), Ok(()));
        assert!(validate("4.2.2", Some("unknown")).is_err());
    }

    #[test]
    fn the_retired_legacy_detector_cannot_be_selected() {
        assert!(validate("3.2.18", None).is_err());
        assert!(validate("3.2.18", Some("sgtd-v4")).is_err());
        assert!(validate("5.0.0", Some("sgtd-v4")).is_err());
    }
}
