use super::*;

impl TemporaryBrowserTab {
    pub(in crate::overlay::computer_control) fn test_lease(id: i64) -> Self {
        Self {
            id,
            foreground: true,
            recovered_create: false,
            epoch: 1,
            window_id: Some(1),
            requested_url: "https://lease.test/".to_string(),
            navigation_allowed: true,
            restore_allowed: false,
            restore: None,
        }
    }
}
