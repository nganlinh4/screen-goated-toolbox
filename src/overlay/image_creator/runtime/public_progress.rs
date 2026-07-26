pub(super) fn stage(value: &str) -> &'static str {
    match value {
        "queued" => "queued",
        "uploading" => "uploading",
        "generating" => "generating",
        "finalizing" => "finalizing",
        "done" => "done",
        "failed" => "failed",
        "cancelled" => "cancelled",
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
        for internal in ["internal", "unknown"] {
            assert_eq!(stage(internal), "preparing");
            assert_eq!(text(stage(internal), false), "Getting ready");
        }
    }

    #[test]
    fn upload_copy_requires_a_reference() {
        assert_eq!(text("uploading", true), "Adding reference image");
        assert_eq!(text("uploading", false), "Getting ready");
    }
}
