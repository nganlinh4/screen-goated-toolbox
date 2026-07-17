//! Typed, bounded diagnostics for research discovery and source retrieval.

const MAX_DIAGNOSTIC_ERRORS: usize = 3;
const MAX_DIAGNOSTIC_ERROR_CHARS: usize = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FailureClass {
    Discovery,
    Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FailureStage {
    Discovery,
    SourceRetrieval,
    SourceEvaluation,
    Mixed,
}

impl FailureStage {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Discovery => "discovery",
            Self::SourceRetrieval => "source_retrieval",
            Self::SourceEvaluation => "source_evaluation",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Default)]
pub(super) struct SourceDiagnostics {
    pub(super) initial_candidate_count: usize,
    pub(super) source_link_page_count: usize,
    pub(super) follow_up_candidate_count: usize,
    pub(super) rejected_domain_count: usize,
    pub(super) empty_source_count: usize,
    pub(super) duplicate_source_count: usize,
    pub(super) discovery_failure_count: usize,
    pub(super) source_failure_count: usize,
    pub(super) source_failure_cutoff_reached: bool,
    pub(super) consecutive_source_failures_at_cutoff: usize,
    pub(super) temporary_tab_opened_count: usize,
    pub(super) temporary_tab_closed_count: usize,
    pub(super) temporary_tab_cleanup_failed_count: usize,
    pub(super) temporary_tab_open_ambiguous_count: usize,
    pub(super) temporary_tab_open_recovered_count: usize,
    pub(super) temporary_tab_restore_failed_count: usize,
    pub(super) codes: Vec<&'static str>,
    pub(super) errors: Vec<String>,
}

impl SourceDiagnostics {
    pub(super) fn failed(
        &mut self,
        class: FailureClass,
        code: &'static str,
        error: impl std::fmt::Display,
    ) {
        match class {
            FailureClass::Discovery => {
                self.discovery_failure_count = self.discovery_failure_count.saturating_add(1);
            }
            FailureClass::Source => {
                self.source_failure_count = self.source_failure_count.saturating_add(1);
            }
        }
        if self.errors.len() < MAX_DIAGNOSTIC_ERRORS {
            self.codes.push(code);
            self.errors.push(
                error
                    .to_string()
                    .chars()
                    .take(MAX_DIAGNOSTIC_ERROR_CHARS)
                    .collect(),
            );
        }
    }

    pub(super) fn failure_count(&self) -> usize {
        self.discovery_failure_count
            .saturating_add(self.source_failure_count)
    }

    pub(super) fn failure_stage(
        &self,
        candidate_count: usize,
        has_sources: bool,
    ) -> Option<FailureStage> {
        if has_sources {
            return None;
        }
        match (self.discovery_failure_count, self.source_failure_count) {
            (discovery, source) if discovery > 0 && source > 0 => Some(FailureStage::Mixed),
            (discovery, _) if discovery > 0 => Some(FailureStage::Discovery),
            (_, source) if source > 0 => Some(FailureStage::SourceRetrieval),
            _ if candidate_count == 0 => Some(FailureStage::Discovery),
            _ => Some(FailureStage::SourceEvaluation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_stage_distinguishes_pipeline_phases() {
        let mut diagnostics = SourceDiagnostics::default();
        assert_eq!(
            diagnostics.failure_stage(0, false),
            Some(FailureStage::Discovery)
        );
        assert_eq!(
            diagnostics.failure_stage(2, false),
            Some(FailureStage::SourceEvaluation)
        );
        diagnostics.failed(FailureClass::Source, "source", "failed");
        assert_eq!(
            diagnostics.failure_stage(2, false),
            Some(FailureStage::SourceRetrieval)
        );
        diagnostics.failed(FailureClass::Discovery, "discovery", "failed");
        assert_eq!(
            diagnostics.failure_stage(2, false),
            Some(FailureStage::Mixed)
        );
        assert_eq!(diagnostics.failure_stage(2, true), None);
    }
}
