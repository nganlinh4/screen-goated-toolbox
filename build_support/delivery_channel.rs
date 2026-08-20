use std::path::{Path, PathBuf};

pub(crate) const PRODUCTION_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
pub(crate) const STAGING_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-staging/";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryChannel {
    Production,
    Staging,
}

pub(crate) struct SelectedManifest {
    pub(crate) path: PathBuf,
    pub(crate) channel: DeliveryChannel,
}

pub(crate) fn configure_build() {
    println!("cargo::rustc-check-cfg=cfg(sgt_staging_delivery)");
    println!("cargo:rerun-if-env-changed=SGT_COMPONENT_DELIVERY_CHANNEL");
    println!("cargo:rerun-if-env-changed=SGT_STAGING_DELIVERY_ROOT");
    if requested_channel() == DeliveryChannel::Staging {
        let profile = std::env::var("PROFILE").unwrap_or_default();
        assert_eq!(
            profile, "debug",
            "staging component delivery is allowed only for debug-profile builds"
        );
        println!("cargo:rustc-cfg=sgt_staging_delivery");
    }
}

pub(crate) fn select(manifest_dir: &Path, default_relative: &str) -> SelectedManifest {
    let production = manifest_dir.join(default_relative);
    let requested = requested_channel();
    let (path, channel) = match requested {
        DeliveryChannel::Production => (production, DeliveryChannel::Production),
        DeliveryChannel::Staging => {
            let root = std::env::var_os("SGT_STAGING_DELIVERY_ROOT")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .expect("SGT_STAGING_DELIVERY_ROOT is required for staging delivery");
            assert!(
                root.is_absolute(),
                "SGT_STAGING_DELIVERY_ROOT must be an absolute path"
            );
            let staged = root.join(default_relative);
            if staged.is_file() {
                (staged, DeliveryChannel::Staging)
            } else {
                (production, DeliveryChannel::Production)
            }
        }
    };
    println!("cargo:rerun-if-changed={}", path.display());
    SelectedManifest { path, channel }
}

pub(crate) fn assert_owned_asset_url(
    channel: DeliveryChannel,
    asset: &str,
    url: &str,
    label: &str,
) {
    let prefix = match channel {
        DeliveryChannel::Production => PRODUCTION_PREFIX,
        DeliveryChannel::Staging => STAGING_PREFIX,
    };
    assert_eq!(
        url,
        format!("{prefix}{asset}"),
        "{label} must use the selected {channel:?} component release"
    );
}

pub(crate) fn assert_candidate_asset_url(
    channel: DeliveryChannel,
    asset: &str,
    url: &str,
    label: &str,
) {
    if channel == DeliveryChannel::Staging && url == format!("{PRODUCTION_PREFIX}{asset}") {
        return;
    }
    assert_owned_asset_url(channel, asset, url, label);
}

pub(crate) fn copy_selected_manifest(manifest_dir: &Path, default_relative: &str, output: &Path) {
    let selected = select(manifest_dir, default_relative);
    std::fs::copy(&selected.path, output).unwrap_or_else(|error| {
        panic!(
            "failed to copy selected delivery {} to {}: {error}",
            selected.path.display(),
            output.display()
        )
    });
}

fn requested_channel() -> DeliveryChannel {
    match std::env::var("SGT_COMPONENT_DELIVERY_CHANNEL") {
        Ok(value) if value == "staging" => DeliveryChannel::Staging,
        Ok(value) if value == "production" || value.is_empty() => DeliveryChannel::Production,
        Ok(value) => panic!("unsupported SGT_COMPONENT_DELIVERY_CHANNEL {value:?}"),
        Err(_) => DeliveryChannel::Production,
    }
}

#[cfg(test)]
mod tests {
    use super::{DeliveryChannel, PRODUCTION_PREFIX, STAGING_PREFIX, assert_candidate_asset_url};

    #[test]
    fn release_channels_have_distinct_fixed_tags() {
        assert_ne!(PRODUCTION_PREFIX, STAGING_PREFIX);
        assert!(PRODUCTION_PREFIX.ends_with("/sgt-runtime-bundles/"));
        assert!(STAGING_PREFIX.ends_with("/sgt-runtime-staging/"));
        assert_ne!(DeliveryChannel::Production, DeliveryChannel::Staging);
    }

    #[test]
    fn partial_staging_contracts_keep_unselected_production_assets() {
        assert_candidate_asset_url(
            DeliveryChannel::Staging,
            "unchanged.zip",
            &format!("{PRODUCTION_PREFIX}unchanged.zip"),
            "candidate",
        );
        assert_candidate_asset_url(
            DeliveryChannel::Staging,
            "selected.zip",
            &format!("{STAGING_PREFIX}selected.zip"),
            "candidate",
        );
        assert!(
            std::panic::catch_unwind(|| {
                assert_candidate_asset_url(
                    DeliveryChannel::Production,
                    "candidate.zip",
                    &format!("{STAGING_PREFIX}candidate.zip"),
                    "candidate",
                );
            })
            .is_err()
        );
    }
}
