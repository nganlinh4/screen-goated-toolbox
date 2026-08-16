use std::sync::LazyLock;

const IMAGE_TO_SVG_CONTRACT: &str =
    include_str!("../parity-fixtures/image-to-svg/state-contract.json");
const IMAGE_CREATOR_CONTRACT: &str =
    include_str!("../parity-fixtures/image-creation-editing/state-contract.json");

static IMAGE_TO_SVG_RELEASE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    let contract: serde_json::Value =
        serde_json::from_str(IMAGE_TO_SVG_CONTRACT).expect("image-to-SVG contract is valid");
    contract["releaseAvailability"]["enabled"]
        .as_bool()
        .expect("image-to-SVG release availability is boolean")
});

static IMAGE_CREATOR_RELEASE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    let contract: serde_json::Value =
        serde_json::from_str(IMAGE_CREATOR_CONTRACT).expect("image creator contract is valid");
    contract["releaseAvailability"]["enabled"]
        .as_bool()
        .expect("image creator release availability is boolean")
});

pub(crate) fn image_to_svg_release_enabled() -> bool {
    *IMAGE_TO_SVG_RELEASE_ENABLED
}

pub(crate) fn image_to_svg_entry_visible() -> bool {
    image_to_svg_release_enabled()
}

pub(crate) fn request_image_to_svg_entry() -> bool {
    image_to_svg_release_enabled()
}

pub(crate) fn image_creator_release_enabled() -> bool {
    *IMAGE_CREATOR_RELEASE_ENABLED
}

pub(crate) fn image_creator_entry_visible() -> bool {
    image_creator_release_enabled()
}

pub(crate) fn request_image_creator_entry() -> bool {
    image_creator_release_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_svg_contract_enables_release_entry() {
        let contract: serde_json::Value = serde_json::from_str(IMAGE_TO_SVG_CONTRACT).unwrap();
        let availability = &contract["releaseAvailability"];
        assert_eq!(availability["enabled"], true);
        assert_eq!(availability["entryVisible"], true);
        assert_eq!(availability["entryBehavior"], "open_surface");
        assert_eq!(availability["startsSurface"], true);
        assert_eq!(availability["startsReadiness"], true);
        assert_eq!(availability["preservesPreparedCapacity"], true);
        assert!(image_to_svg_entry_visible());
        assert!(request_image_to_svg_entry());
    }

    #[test]
    fn shared_image_contract_hides_and_gates_release_entry() {
        let contract: serde_json::Value = serde_json::from_str(IMAGE_CREATOR_CONTRACT).unwrap();
        let availability = &contract["releaseAvailability"];
        assert_eq!(availability["enabled"], false);
        assert_eq!(availability["entryVisible"], false);
        assert_eq!(availability["entryBehavior"], "hidden");
        assert_eq!(availability["startsSurface"], false);
        assert_eq!(availability["startsReadiness"], false);
        assert_eq!(availability["preservesPreparedCapacity"], true);
        assert!(!image_creator_entry_visible());
        assert!(!request_image_creator_entry());
    }
}
