//! One status row for a managed component: title on the left; on the right,
//! a running removal or install, else the component's state and the actions
//! that state offers.

use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::download_manager::InstallStatus;
use crate::gui::theme::AppTheme;
use eframe::egui;

use super::utils::{removal_in_progress, render_install_progress};

pub(super) enum ComponentState<'a> {
    Checking,
    Installed {
        summary: String,
    },
    /// A usable copy exists outside the managed install; installing adds the
    /// managed one.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
        )
    )]
    FoundElsewhere {
        summary: String,
    },
    Installing {
        percent: f32,
    },
    /// An install step that reports no percentage.
    Working {
        label: &'a str,
    },
    Failed {
        message: &'a str,
    },
    /// A failed install that left files behind: retry or remove them.
    Broken {
        message: &'a str,
    },
    /// Some files are present but the install is incomplete.
    Partial {
        installable: bool,
    },
    Missing,
    Unavailable,
    /// Removal is recorded and finishes once the files are no longer in use.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
        )
    )]
    RemovalPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowAction {
    Install,
    Remove,
}

impl<'a> ComponentState<'a> {
    /// The actions this state offers, in reading order.
    pub(super) fn offered_actions(&self) -> &'static [RowAction] {
        match self {
            Self::Installed { .. } => &[RowAction::Remove],
            Self::FoundElsewhere { .. } | Self::Failed { .. } | Self::Missing => {
                &[RowAction::Install]
            }
            Self::Broken { .. } | Self::Partial { installable: true } => {
                &[RowAction::Install, RowAction::Remove]
            }
            Self::Partial { installable: false } => &[RowAction::Remove],
            Self::Checking
            | Self::Installing { .. }
            | Self::Working { .. }
            | Self::Unavailable
            | Self::RemovalPending => &[],
        }
    }

    /// The row state of a download-manager status; `installed` summarizes an
    /// installed component.
    pub(super) fn from_install_status(
        status: &'a InstallStatus,
        text: &'a LocaleText,
        installed: impl FnOnce() -> String,
    ) -> Self {
        let download = &text.auxiliary.download;
        match status {
            InstallStatus::Checking => ComponentState::Checking,
            InstallStatus::Installed => ComponentState::Installed {
                summary: installed(),
            },
            InstallStatus::Downloading(fraction) => ComponentState::Installing {
                percent: fraction * 100.0,
            },
            InstallStatus::Extracting => ComponentState::Working {
                label: download.download_status_extracting,
            },
            InstallStatus::Finalizing => ComponentState::Working {
                label: download.download_status_finalizing,
            },
            InstallStatus::Error(message) => ComponentState::Failed { message },
            InstallStatus::Missing => ComponentState::Missing,
            InstallStatus::Unavailable => ComponentState::Unavailable,
        }
    }
}

/// Keys of the background operations that take over the row while running.
/// `install` is `None` when the install reports through its own state.
pub(super) struct RowOperations {
    pub(super) install: Option<&'static str>,
    pub(super) removal: &'static str,
}

/// Returns the action the user clicked, if any.
pub(super) fn render_component_row(
    ui: &mut egui::Ui,
    text: &LocaleText,
    title: &str,
    operations: RowOperations,
    state: ComponentState<'_>,
) -> Option<RowAction> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            clicked = render_status(ui, text, &operations, &state);
        });
    });
    clicked
}

fn render_status(
    ui: &mut egui::Ui,
    text: &LocaleText,
    operations: &RowOperations,
    state: &ComponentState<'_>,
) -> Option<RowAction> {
    let managed = &text.auxiliary.managed_tools;
    if removal_in_progress(operations.removal) {
        ui.label(managed.tool_status_removing);
        ui.spinner();
        return None;
    }
    if let Some(progress) = operations
        .install
        .and_then(crate::install_progress::snapshot)
    {
        render_install_progress(ui, text, progress);
        return None;
    }

    let theme = AppTheme::from_ui(ui);
    // Right-to-left layout: the last action in reading order is added first
    // so it sits at the far right, followed by the status label.
    let mut clicked = None;
    for action in state.offered_actions().iter().rev() {
        let button = match action {
            RowAction::Remove => egui::Button::new(
                egui::RichText::new(managed.tool_action_delete).color(theme.danger_text()),
            ),
            RowAction::Install => egui::Button::new(managed.tool_action_download),
        };
        if ui.add(button).clicked() {
            clicked = Some(*action);
        }
    }
    match state {
        ComponentState::Checking => {
            ui.label(text.auxiliary.download.download_status_checking);
            ui.spinner();
        }
        ComponentState::Installed { summary } => {
            ui.label(egui::RichText::new(summary).color(theme.success()));
        }
        ComponentState::FoundElsewhere { summary } => {
            ui.label(egui::RichText::new(summary).color(egui::Color32::from_rgb(96, 125, 139)));
        }
        ComponentState::Installing { percent } => {
            ui.label(format!("{percent:.0}%"));
            ui.spinner();
        }
        ComponentState::Working { label } => {
            ui.label(*label);
            ui.spinner();
        }
        ComponentState::Failed { message } | ComponentState::Broken { message } => {
            ui.label(
                egui::RichText::new(managed.tool_status_install_failed).color(theme.danger_text()),
            )
            .on_hover_text(*message);
        }
        ComponentState::Partial { installable: true } | ComponentState::Missing => {
            ui.label(egui::RichText::new(managed.tool_status_missing).color(egui::Color32::GRAY));
        }
        ComponentState::Partial { installable: false } | ComponentState::Unavailable => {
            ui.label(
                egui::RichText::new(managed.tool_status_unavailable).color(theme.danger_text()),
            );
        }
        ComponentState::RemovalPending => {
            ui.label(managed.tool_status_removal_pending);
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::{ComponentState, InstallStatus, RowAction};
    use crate::gui::locale::LocaleText;

    #[test]
    fn each_state_offers_only_the_actions_that_apply() {
        use RowAction::{Install, Remove};
        let installed = ComponentState::Installed {
            summary: String::new(),
        };
        assert_eq!(installed.offered_actions(), [Remove]);
        let elsewhere = ComponentState::FoundElsewhere {
            summary: String::new(),
        };
        assert_eq!(elsewhere.offered_actions(), [Install]);
        assert_eq!(
            ComponentState::Failed { message: "x" }.offered_actions(),
            [Install]
        );
        assert_eq!(ComponentState::Missing.offered_actions(), [Install]);
        assert_eq!(
            ComponentState::Broken { message: "x" }.offered_actions(),
            [Install, Remove]
        );
        assert_eq!(
            ComponentState::Partial { installable: true }.offered_actions(),
            [Install, Remove]
        );
        assert_eq!(
            ComponentState::Partial { installable: false }.offered_actions(),
            [Remove]
        );
        for idle in [
            ComponentState::Checking,
            ComponentState::Installing { percent: 10.0 },
            ComponentState::Working { label: "x" },
            ComponentState::Unavailable,
            ComponentState::RemovalPending,
        ] {
            assert!(idle.offered_actions().is_empty());
        }
    }

    #[test]
    fn download_manager_statuses_map_onto_row_states() {
        let text = LocaleText::get("en");
        let state = |status: &InstallStatus| {
            ComponentState::from_install_status(status, &text, || "1 MB".to_string())
                .offered_actions()
                .to_vec()
        };
        assert_eq!(state(&InstallStatus::Installed), [RowAction::Remove]);
        assert_eq!(state(&InstallStatus::Missing), [RowAction::Install]);
        assert_eq!(
            state(&InstallStatus::Error("disk".into())),
            [RowAction::Install]
        );
        for busy in [
            InstallStatus::Checking,
            InstallStatus::Downloading(0.5),
            InstallStatus::Extracting,
            InstallStatus::Finalizing,
            InstallStatus::Unavailable,
        ] {
            assert!(state(&busy).is_empty());
        }
        let downloading = InstallStatus::Downloading(0.25);
        assert!(matches!(
            ComponentState::from_install_status(&downloading, &text, String::new),
            ComponentState::Installing { percent } if (percent - 25.0).abs() < f32::EPSILON
        ));
    }
}
