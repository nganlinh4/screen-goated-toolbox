#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalToolInstallEvent {
    Preparing,
    Checking,
    Downloading { downloaded: u64, total: u64 },
    Extracting,
    Finalizing,
}

pub(crate) fn report_badge_event(
    badge: &crate::overlay::auto_copy_badge::DownloadProgressBadge,
    component_name: &str,
    event: ExternalToolInstallEvent,
) {
    match event {
        ExternalToolInstallEvent::Preparing => {
            let message = localized_install_event_message(component_name, event);
            badge.set_phase(&message, 0.0);
        }
        ExternalToolInstallEvent::Checking => {
            let message = localized_install_event_message(component_name, event);
            badge.set_phase(&message, 100.0);
        }
        ExternalToolInstallEvent::Downloading { downloaded, total } => {
            badge.report(downloaded, total);
        }
        ExternalToolInstallEvent::Extracting => {
            let message = localized_install_event_message(component_name, event);
            badge.set_phase(&message, 100.0);
        }
        ExternalToolInstallEvent::Finalizing => {
            let message = localized_install_event_message(component_name, event);
            badge.set_phase(&message, 100.0);
        }
    }
}

pub(crate) fn localized_install_event_message(
    component_name: &str,
    event: ExternalToolInstallEvent,
) -> String {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let template = match event {
        ExternalToolInstallEvent::Preparing => locale.preparing_component_fmt,
        ExternalToolInstallEvent::Checking => locale.checking_component_fmt,
        ExternalToolInstallEvent::Downloading { .. } => locale.downloading_component_fmt,
        ExternalToolInstallEvent::Extracting => locale.extracting_package_fmt,
        ExternalToolInstallEvent::Finalizing => locale.finalizing_component_fmt,
    };
    crate::overlay::auto_copy_badge::format_locale(template, &[("name", component_name)])
}
