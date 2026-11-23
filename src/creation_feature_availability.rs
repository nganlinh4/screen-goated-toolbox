use serde::Deserialize;
use std::sync::LazyLock;

const IMAGE_TO_SVG_CONTRACT: &str =
    include_str!("../parity-fixtures/image-to-svg/state-contract.json");

/// The shared `releaseAvailability` contract. Each accessor reads its own field
/// so a staged rollout (enabled but hidden, or visible but not yet opening a
/// surface) behaves the same on every platform.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseAvailability {
    enabled: bool,
    entry_visible: bool,
    starts_surface: bool,
}

impl ReleaseAvailability {
    fn parse(contract: &str) -> Self {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Contract {
            release_availability: ReleaseAvailability,
        }
        serde_json::from_str::<Contract>(contract)
            .expect("image-to-SVG release availability contract is valid")
            .release_availability
    }

    fn entry_visible(&self) -> bool {
        self.enabled && self.entry_visible
    }

    fn starts_surface(&self) -> bool {
        self.enabled && self.starts_surface
    }
}

static IMAGE_TO_SVG: LazyLock<ReleaseAvailability> =
    LazyLock::new(|| ReleaseAvailability::parse(IMAGE_TO_SVG_CONTRACT));

pub(crate) fn image_to_svg_release_enabled() -> bool {
    IMAGE_TO_SVG.enabled
}

pub(crate) fn image_to_svg_entry_visible() -> bool {
    IMAGE_TO_SVG.entry_visible()
}

pub(crate) fn request_image_to_svg_entry() -> bool {
    IMAGE_TO_SVG.starts_surface()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_svg_contract_enables_release_entry() {
        let contract: serde_json::Value = serde_json::from_str(IMAGE_TO_SVG_CONTRACT).unwrap();
        let availability = &contract["releaseAvailability"];
        assert_eq!(availability["entryBehavior"], "open_surface");
        assert_eq!(availability["startsReadiness"], true);
        assert_eq!(availability["preservesPreparedCapacity"], true);
        assert!(image_to_svg_release_enabled());
        assert!(image_to_svg_entry_visible());
        assert!(request_image_to_svg_entry());
    }

    #[test]
    fn each_accessor_follows_its_own_contract_field() {
        let parse = |enabled: bool, entry_visible: bool, starts_surface: bool| {
            ReleaseAvailability::parse(
                &serde_json::json!({"releaseAvailability": {
                    "enabled": enabled,
                    "entryVisible": entry_visible,
                    "startsSurface": starts_surface,
                    "entryBehavior": "open_surface"
                }})
                .to_string(),
            )
        };
        let staged = parse(true, false, true);
        assert!(staged.enabled && !staged.entry_visible() && staged.starts_surface());
        let visible_only = parse(true, true, false);
        assert!(visible_only.entry_visible() && !visible_only.starts_surface());
        let disabled = parse(false, true, true);
        assert!(!disabled.entry_visible() && !disabled.starts_surface());
    }
}
