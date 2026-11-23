//! Downloaded Tools card for the complete Creation product.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::gui::locale::LocaleText;
use eframe::egui;

use super::component_row::{ComponentState, RowAction, RowOperations, render_component_row};
use super::utils::{
    cached_probe, cached_u64, format_size, get_dir_size, invalidate_probe_cache,
    invalidate_u64_cache, start_install, start_removal, tool_card,
};

const PROBE_CREATION: &str = "downloaded-tools:creation";
const PROBE_CREATION_ANY: &str = "downloaded-tools:creation-any";
const VALUE_CREATION_SIZE: &str = "downloaded-tools:creation-size";
const INSTALL_CREATION: &str = "downloaded-tools:creation-install";

pub(super) fn render_three_d_generator_card(ui: &mut egui::Ui, text: &LocaleText) {
    use crate::overlay::three_d_generator as creation;

    let managed = &text.auxiliary.managed_tools;
    tool_card(ui, |ui| {
        ui.heading(managed.tool_creation_card);
        ui.add_space(4.0);
        let state = if cached_probe(PROBE_CREATION, creation::is_product_installed) {
            ComponentState::Installed {
                summary: format_size(cached_u64(VALUE_CREATION_SIZE, product_installed_size)),
            }
        } else {
            product_state(
                creation::is_product_available(),
                cached_probe(PROBE_CREATION_ANY, creation::is_product_partially_installed),
            )
        };
        let operations = RowOperations {
            install: Some(INSTALL_CREATION),
            removal: PROBE_CREATION,
        };
        match render_component_row(ui, text, managed.tool_creation_payload, operations, state) {
            Some(RowAction::Install) => start_install(INSTALL_CREATION, || {
                let _ = creation::download_product(Arc::new(AtomicBool::new(false)), true);
                invalidate_caches();
            }),
            Some(RowAction::Remove) => start_removal(
                PROBE_CREATION,
                managed.tool_creation_card.to_string(),
                creation::remove_product,
                invalidate_caches,
            ),
            None => {}
        }
        ui.label(managed.tool_desc_creation_product);
    });
}

/// A product that is not fully installed: leftover files from an earlier or
/// interrupted install can always be removed.
fn product_state(available: bool, partially_installed: bool) -> ComponentState<'static> {
    match (available, partially_installed) {
        (available, true) => ComponentState::Partial {
            installable: available,
        },
        (true, false) => ComponentState::Missing,
        (false, false) => ComponentState::Unavailable,
    }
}

fn invalidate_caches() {
    invalidate_probe_cache(PROBE_CREATION);
    invalidate_probe_cache(PROBE_CREATION_ANY);
    invalidate_u64_cache(VALUE_CREATION_SIZE);
}

fn product_installed_size() -> u64 {
    get_dir_size(&crate::overlay::three_d_generator::product_dir())
}

#[cfg(test)]
mod tests {
    use super::super::component_row::RowAction;
    use super::product_state;

    #[test]
    fn leftover_files_stay_removable_whether_or_not_delivery_is_available() {
        use RowAction::{Install, Remove};
        assert_eq!(
            product_state(true, true).offered_actions(),
            [Install, Remove]
        );
        assert_eq!(product_state(false, true).offered_actions(), [Remove]);
        assert_eq!(product_state(true, false).offered_actions(), [Install]);
        assert!(product_state(false, false).offered_actions().is_empty());
    }
}
