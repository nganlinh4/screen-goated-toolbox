pub(super) fn runtime_preparation_status() -> String {
    crate::overlay::creation_runtime::readiness("3d")
}

pub(super) fn start_preparation_maintainer(install_if_missing: bool) {
    if !crate::overlay::creation_close::is_closing("3d") {
        crate::overlay::creation_runtime::maintain_readiness("3d", install_if_missing);
    }
}

pub(super) fn prepare_runtime() -> String {
    start_preparation_maintainer(true);
    "preparing".to_string()
}
