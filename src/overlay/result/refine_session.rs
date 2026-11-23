use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_EDIT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RefinePhase {
    #[default]
    Idle,
    Editing,
    Submitting,
    Streaming,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RefineSession {
    phase: RefinePhase,
    edit_id: Option<u64>,
    draft: String,
    original_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefineSubmission {
    pub original_text: String,
    pub instruction: String,
}

impl RefineSession {
    pub fn restored(editing: bool, draft: String) -> Self {
        if editing {
            Self {
                phase: RefinePhase::Editing,
                edit_id: Some(NEXT_EDIT_SESSION.fetch_add(1, Ordering::Relaxed)),
                draft,
                original_text: None,
            }
        } else {
            Self::default()
        }
    }

    pub fn is_editing(&self) -> bool {
        self.phase == RefinePhase::Editing
    }

    pub fn edit_id(&self) -> Option<u64> {
        self.is_editing().then_some(self.edit_id).flatten()
    }

    pub fn begin_edit(&mut self) {
        self.phase = RefinePhase::Editing;
        self.edit_id = Some(NEXT_EDIT_SESSION.fetch_add(1, Ordering::Relaxed));
        self.draft.clear();
        self.original_text = None;
    }

    pub fn cancel_edit(&mut self) {
        if self.is_editing() {
            self.finish();
        }
    }

    pub fn set_draft(&mut self, draft: impl Into<String>) -> bool {
        if !self.is_editing() {
            return false;
        }
        self.draft = draft.into();
        true
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn begin_submit(
        &mut self,
        original_text: String,
        instruction: impl Into<String>,
    ) -> Option<RefineSubmission> {
        if !self.is_editing() {
            return None;
        }
        let instruction = instruction.into();
        if instruction.trim().is_empty() {
            return None;
        }
        self.phase = RefinePhase::Submitting;
        self.draft.clone_from(&instruction);
        self.original_text = Some(original_text.clone());
        Some(RefineSubmission {
            original_text,
            instruction,
        })
    }

    /// The text a submitted refinement started from, while it is in flight.
    pub fn submitted_original(&self) -> Option<&str> {
        matches!(self.phase, RefinePhase::Submitting | RefinePhase::Streaming)
            .then_some(self.original_text.as_deref())
            .flatten()
    }

    pub fn mark_streaming(&mut self) {
        if self.phase == RefinePhase::Submitting {
            self.phase = RefinePhase::Streaming;
        }
    }

    pub fn finish(&mut self) {
        self.phase = RefinePhase::Idle;
        self.edit_id = None;
        self.draft.clear();
        self.original_text = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{RefinePhase, RefineSession};

    #[test]
    fn submission_snapshots_content_before_the_result_is_mutated() {
        let mut session = RefineSession::default();
        session.begin_edit();
        assert!(session.set_draft("make it concise"));

        let submission = session
            .begin_submit("original result".to_string(), "make it concise")
            .unwrap();

        assert_eq!(submission.original_text, "original result");
        assert_eq!(submission.instruction, "make it concise");
        assert_eq!(session.phase, RefinePhase::Submitting);
        session.mark_streaming();
        assert_eq!(session.phase, RefinePhase::Streaming);
        session.finish();
        assert_eq!(session, RefineSession::default());
    }

    #[test]
    fn stale_draft_messages_cannot_reopen_a_closed_editor() {
        let mut session = RefineSession::default();
        assert!(!session.set_draft("late input"));
        session.begin_edit();
        session.cancel_edit();
        assert!(!session.set_draft("late input"));
        assert_eq!(session.draft(), "");
    }

    #[test]
    fn reopened_editor_has_a_distinct_session_identity() {
        let mut session = RefineSession::default();
        session.begin_edit();
        let first = session.edit_id().unwrap();
        session.cancel_edit();
        assert_eq!(session.edit_id(), None);
        session.begin_edit();
        assert_ne!(session.edit_id(), Some(first));
    }
}
