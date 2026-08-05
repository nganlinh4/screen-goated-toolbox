use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

const IMAGE_CREATION_CONTRACT: &str =
    include_str!("../parity-fixtures/image-creation-editing/state-contract.json");
const IMAGE_TO_SVG_CONTRACT: &str =
    include_str!("../parity-fixtures/image-to-svg/state-contract.json");

static IMAGE_CREATOR_RELEASE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    let contract: serde_json::Value =
        serde_json::from_str(IMAGE_CREATION_CONTRACT).expect("image creation contract is valid");
    contract["releaseAvailability"]["enabled"]
        .as_bool()
        .expect("image creation release availability is boolean")
});
static IMAGE_TO_SVG_RELEASE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    let contract: serde_json::Value =
        serde_json::from_str(IMAGE_TO_SVG_CONTRACT).expect("image-to-SVG contract is valid");
    contract["releaseAvailability"]["enabled"]
        .as_bool()
        .expect("image-to-SVG release availability is boolean")
});
static IMAGE_CREATOR_DIALOG_REQUESTED: AtomicBool = AtomicBool::new(false);
static IMAGE_TO_SVG_DIALOG_REQUESTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn image_creator_release_enabled() -> bool {
    *IMAGE_CREATOR_RELEASE_ENABLED
}

pub(crate) fn image_to_svg_release_enabled() -> bool {
    *IMAGE_TO_SVG_RELEASE_ENABLED
}

pub(crate) fn image_creator_entry_visible() -> bool {
    image_creator_release_enabled()
}

pub(crate) fn image_to_svg_entry_visible() -> bool {
    image_to_svg_release_enabled()
}

pub(crate) fn request_image_creator_entry() -> bool {
    if image_creator_release_enabled() {
        return true;
    }
    crate::log_info!("[ImageCreator] Coming soon dialog requested");
    IMAGE_CREATOR_DIALOG_REQUESTED.store(true, Ordering::SeqCst);
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.request_repaint();
    }
    false
}

pub(crate) fn take_image_creator_dialog_request() -> bool {
    IMAGE_CREATOR_DIALOG_REQUESTED.swap(false, Ordering::SeqCst)
}

pub(crate) fn request_image_to_svg_entry() -> bool {
    if image_to_svg_release_enabled() {
        return true;
    }
    crate::log_info!("[ImageToSvg] Coming soon dialog requested");
    IMAGE_TO_SVG_DIALOG_REQUESTED.store(true, Ordering::SeqCst);
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.request_repaint();
    }
    false
}

pub(crate) fn take_image_to_svg_dialog_request() -> bool {
    IMAGE_TO_SVG_DIALOG_REQUESTED.swap(false, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_contract_disables_release_entry_without_starting_work() {
        let contract: serde_json::Value = serde_json::from_str(IMAGE_CREATION_CONTRACT).unwrap();
        let availability = &contract["releaseAvailability"];
        assert_eq!(availability["enabled"], false);
        assert_eq!(availability["entryBehavior"], "coming_soon_dialog");
        assert_eq!(availability["startsSurface"], false);
        assert_eq!(availability["startsReadiness"], false);
        assert_eq!(availability["preservesPreparedCapacity"], true);
        assert_eq!(availability["entryVisible"], false);
        assert!(!image_creator_entry_visible());
        assert!(!request_image_creator_entry());
        assert!(take_image_creator_dialog_request());
    }

    #[test]
    fn shared_svg_contract_hides_and_gates_release_entry() {
        let contract: serde_json::Value = serde_json::from_str(IMAGE_TO_SVG_CONTRACT).unwrap();
        let availability = &contract["releaseAvailability"];
        assert_eq!(availability["enabled"], false);
        assert_eq!(availability["entryVisible"], false);
        assert_eq!(availability["entryBehavior"], "coming_soon_dialog");
        assert_eq!(availability["startsSurface"], false);
        assert_eq!(availability["startsReadiness"], false);
        assert_eq!(availability["preservesPreparedCapacity"], true);
        assert!(!image_to_svg_entry_visible());
        assert!(!request_image_to_svg_entry());
        assert!(take_image_to_svg_dialog_request());
    }
}
