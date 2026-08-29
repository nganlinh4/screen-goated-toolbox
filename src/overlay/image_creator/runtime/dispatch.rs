pub(super) fn next_job_id() -> Result<String, String> {
    crate::overlay::creation_identity::random_id("image_")
}

pub(super) fn next_dispatch_id() -> Result<String, String> {
    crate::overlay::creation_identity::random_id("image-dispatch-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_submissions_get_fresh_identifiers() {
        assert_ne!(next_job_id().unwrap(), next_job_id().unwrap());
        assert_ne!(next_dispatch_id().unwrap(), next_dispatch_id().unwrap());
    }
}
