use super::{catalog, install, prefs, registry, runtime};

/// One integration row for the settings panel.
pub(crate) struct UiIntegration {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub addon_hint: Option<&'static str>,
    pub installed: bool,
    pub connected: bool,
    pub installing: bool,
}

pub(crate) fn ui_list() -> Vec<UiIntegration> {
    let installing = install::installing().lock();
    catalog::all()
        .iter()
        .map(|integration| UiIntegration {
            id: integration.id,
            display_name: integration.display_name,
            description: integration.description,
            addon_hint: integration.addon_hint,
            installed: registry::is_installed(integration.id),
            connected: super::is_connected(integration.id),
            installing: installing.contains(integration.id),
        })
        .collect()
}

/// Install + connect on a background thread (the UI button calls this). Idempotent.
pub(crate) fn ui_install(id: &str) {
    install::spawn(id);
}

pub(crate) fn ui_remove(id: &str) {
    runtime::disconnect(id);
    registry::remove(id);
}

/// Uninstall + forget everything (the panel's "Clean all").
pub(crate) fn ui_remove_all() {
    for id in registry::installed_ids() {
        runtime::disconnect(&id);
        registry::remove(&id);
    }
    prefs::clear();
}
