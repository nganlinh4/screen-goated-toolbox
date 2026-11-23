pub(super) use crate::component_registry::directory::prepare_target;
pub(super) use crate::component_registry::owned_tree::remove_listed as cleanup_owned;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parent_creation_rejects_a_non_directory_component() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-web-staging-parent-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("assets"), b"not a directory").unwrap();
        assert!(prepare_target(&root, Path::new("assets/index.js")).is_err());
        std::fs::remove_file(root.join("assets")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
