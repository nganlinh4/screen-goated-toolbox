pub(super) fn stage(value: &str, has_references: bool) -> &'static str {
    match value {
        "queued" => "queued",
        "uploading" if has_references => "uploading",
        "generating" => "generating",
        "finalizing" => "finalizing",
        _ => "preparing",
    }
}

pub(super) fn text(stage: &str, has_references: bool) -> &'static str {
    match stage {
        "queued" => "Queued",
        "uploading" if has_references => "Adding reference image",
        "generating" => "Creating image",
        "finalizing" => "Finishing image",
        "done" => "Image ready",
        "failed" => "Could not create image",
        "cancelled" => "Cancelled",
        _ => "Getting ready",
    }
}

pub(super) fn key(stage: &str) -> String {
    format!("image.{stage}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_stages_collapse_to_normal_product_progress() {
        for internal in ["internal", "unknown", "done", "failed", "cancelled"] {
            assert_eq!(stage(internal, false), "preparing");
            assert_eq!(text(stage(internal, false), false), "Getting ready");
        }
    }

    #[test]
    fn upload_copy_requires_a_reference() {
        assert_eq!(text("uploading", true), "Adding reference image");
        assert_eq!(text("uploading", false), "Getting ready");
        assert_eq!(stage("uploading", true), "uploading");
        assert_eq!(stage("uploading", false), "preparing");
    }
}
