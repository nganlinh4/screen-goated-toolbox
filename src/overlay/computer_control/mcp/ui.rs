use super::{catalog, install, registry, runtime};
use std::collections::HashSet;

/// One integration row for the settings panel.
pub(crate) struct UiIntegration {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub addon_hint: Option<&'static str>,
    pub installed: bool,
    pub connected: bool,
    pub installing: bool,
    pub removing: bool,
}

pub(crate) fn ui_list() -> Vec<UiIntegration> {
    let installing = install::installing().lock();
    let removing = removing().lock();
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
            removing: removing.contains(integration.id),
        })
        .collect()
}

pub(crate) fn ui_installing() -> bool {
    !install::installing().lock().is_empty()
}

/// Install + connect on a background thread (the UI button calls this). Idempotent.
pub(crate) fn ui_install(id: &str) {
    install::spawn(id);
}

pub(crate) fn ui_remove(id: &str) {
    if !removing().lock().insert(id.to_string()) {
        return;
    }
    let id = id.to_string();
    let name = catalog::all()
        .iter()
        .find(|integration| integration.id == id)
        .map(|integration| integration.display_name)
        .unwrap_or("MCP integration")
        .to_string();
    std::thread::spawn(move || {
        notify_removal(&name, None, true);
        runtime::disconnect(&id);
        let result = registry::try_remove(&id);
        notify_removal(&name, result.as_ref().err(), false);
        removing().lock().remove(&id);
    });
}

/// Uninstall + forget everything (the panel's "Clean all").
pub(crate) fn ui_remove_all() {
    for id in registry::installed_ids() {
        runtime::disconnect(&id);
        registry::remove(&id);
    }
}

fn removing() -> &'static parking_lot::Mutex<HashSet<String>> {
    static REMOVING: std::sync::OnceLock<parking_lot::Mutex<HashSet<String>>> =
        std::sync::OnceLock::new();
    REMOVING.get_or_init(|| parking_lot::Mutex::new(HashSet::new()))
}

fn notify_removal(name: &str, error: Option<&anyhow::Error>, started: bool) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = if started {
        (
            locale.removing_component_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Info,
            String::new(),
        )
    } else if let Some(error) = error {
        (
            locale.component_remove_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        )
    } else {
        (
            locale.component_removed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        )
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
}
