use std::path::{Path, PathBuf};

pub(super) fn prepare_root(
    test_mode: bool,
    explicit: Option<PathBuf>,
    runtime: PathBuf,
    temporary: PathBuf,
    maintain: impl FnOnce(&Path),
) -> PathBuf {
    if test_mode {
        return temporary.join(format!("sgt-cc-tests-{}", std::process::id()));
    }
    let root = explicit.unwrap_or_else(|| runtime.join("cc-trace"));
    maintain(&root);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_trace_sessions_ignore_external_roots_and_skip_retention() {
        let runtime = PathBuf::from("runtime");
        let explicit = PathBuf::from("external");
        let temporary = std::env::temp_dir();
        for configured in [None, Some(explicit.clone())] {
            let root = prepare_root(true, configured, runtime.clone(), temporary.clone(), |_| {
                panic!("unit session attempted trace retention")
            });
            assert!(root.starts_with(&temporary));
            assert!(!root.starts_with(&runtime));
            assert!(!root.starts_with(&explicit));
        }
    }

    #[test]
    fn runtime_trace_retention_uses_only_the_selected_root() {
        let runtime = PathBuf::from("runtime");
        for configured in [None, Some(PathBuf::from("explicit"))] {
            let expected = configured
                .clone()
                .unwrap_or_else(|| runtime.join("cc-trace"));
            let mut maintained = None;
            let root = prepare_root(
                false,
                configured,
                runtime.clone(),
                PathBuf::from("temporary"),
                |path| {
                    maintained = Some(path.to_path_buf());
                },
            );
            assert_eq!(root, expected);
            assert_eq!(maintained, Some(expected));
        }
    }
}
