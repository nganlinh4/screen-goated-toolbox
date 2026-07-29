pub(in crate::overlay::image_creator) fn runtime_preparation_status() -> String {
    crate::overlay::creation_runtime::readiness("image")
}

pub(super) fn start_preparation() {
    if !crate::overlay::creation_close::is_closing("image") {
        crate::overlay::creation_runtime::maintain_readiness("image", false);
    }
}

pub(in crate::overlay::image_creator) fn prepare_runtime() -> String {
    if !crate::overlay::creation_close::is_closing("image") {
        crate::overlay::creation_runtime::maintain_readiness("image", true);
    }
    "preparing".to_string()
}
