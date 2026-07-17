//! Scoped browser surfaces used by research collection.

use super::diagnostics::{FailureClass, SourceDiagnostics};
use serde_json::{Value, json};

pub(super) struct TemporaryTab {
    tab: super::super::browser::TemporaryBrowserTab,
    failure_class: FailureClass,
    cleanup_attempted: bool,
}

impl TemporaryTab {
    pub(super) fn id(&self) -> i64 {
        self.tab.id
    }

    pub(super) fn open(
        url: &str,
        diagnostics: &mut SourceDiagnostics,
        failure_class: FailureClass,
    ) -> anyhow::Result<Self> {
        let tab = match super::super::browser::open_temporary_tab(url) {
            Ok(tab) => tab,
            Err(error) => {
                if super::super::browser::temporary_tab_open_effect_ambiguous(&error) {
                    diagnostics.temporary_tab_open_ambiguous_count = diagnostics
                        .temporary_tab_open_ambiguous_count
                        .saturating_add(1);
                }
                return Err(error);
            }
        };
        diagnostics.temporary_tab_opened_count =
            diagnostics.temporary_tab_opened_count.saturating_add(1);
        if tab.recovered_create {
            diagnostics.temporary_tab_open_recovered_count = diagnostics
                .temporary_tab_open_recovered_count
                .saturating_add(1);
        }
        super::super::telemetry::event(
            "research_surface_opened",
            "research",
            super::super::telemetry::Privacy::Safe,
            json!({"tab_id": tab.id, "foreground": tab.foreground, "create_recovered": tab.recovered_create}),
        );
        Ok(Self {
            tab,
            failure_class,
            cleanup_attempted: false,
        })
    }

    pub(super) fn close(mut self, diagnostics: &mut SourceDiagnostics) {
        self.cleanup_attempted = true;
        let result = super::super::browser::close_tab_verified(&self.tab);
        if result.closed_verified {
            diagnostics.temporary_tab_closed_count =
                diagnostics.temporary_tab_closed_count.saturating_add(1);
        } else {
            diagnostics.temporary_tab_cleanup_failed_count = diagnostics
                .temporary_tab_cleanup_failed_count
                .saturating_add(1);
            let detail = result
                .close_error
                .as_deref()
                .or(result.preservation_reason.as_deref())
                .unwrap_or("cleanup was not verified");
            diagnostics.failed(
                self.failure_class,
                "temporary_tab_cleanup_failed",
                format!("temporary tab cleanup failed: {detail}"),
            );
        }
        if let Some(error) = &result.restore_error {
            diagnostics.temporary_tab_restore_failed_count = diagnostics
                .temporary_tab_restore_failed_count
                .saturating_add(1);
            diagnostics.failed(
                self.failure_class,
                "temporary_tab_restore_failed",
                format!("temporary tab foreground restoration failed: {error}"),
            );
        }
        super::super::telemetry::event(
            "research_surface_closed",
            "research",
            super::super::telemetry::Privacy::Safe,
            cleanup_telemetry(&self.tab, &result, false),
        );
    }
}

impl Drop for TemporaryTab {
    fn drop(&mut self) {
        if self.cleanup_attempted {
            return;
        }
        self.cleanup_attempted = true;
        let result = super::super::browser::close_tab_verified(&self.tab);
        super::super::telemetry::event(
            "research_surface_closed",
            "research",
            super::super::telemetry::Privacy::Safe,
            cleanup_telemetry(&self.tab, &result, true),
        );
    }
}

fn cleanup_telemetry(
    tab: &super::super::browser::TemporaryBrowserTab,
    result: &super::super::browser::TemporaryTabCleanup,
    drop_fallback: bool,
) -> Value {
    json!({
        "tab_id": tab.id,
        "closed_verified": result.closed_verified,
        "preserved": result.preserved,
        "preservation_reason": result.preservation_reason,
        "restoration_required": result.restoration_required,
        "restored": result.restored,
        "close_error": result.close_error,
        "restore_error": result.restore_error,
        "attempts": 1,
        "drop_fallback": drop_fallback,
    })
}
