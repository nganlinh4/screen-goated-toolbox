use anyhow::Result;

#[derive(Clone, Debug)]
pub(in crate::overlay::computer_control) struct CandidateAttempt {
    pub(in crate::overlay::computer_control) model_id: String,
    pub(in crate::overlay::computer_control) provider: String,
    pub(in crate::overlay::computer_control) response: Option<String>,
    pub(in crate::overlay::computer_control) error: Option<String>,
    pub(in crate::overlay::computer_control) accepted: bool,
}

impl CandidateAttempt {
    pub(super) fn response(
        model_id: &str,
        provider: &str,
        response: String,
        accepted: bool,
    ) -> Self {
        Self {
            model_id: model_id.to_string(),
            provider: provider.to_string(),
            response: Some(response),
            error: None,
            accepted,
        }
    }

    pub(super) fn error(model_id: &str, provider: &str, error: String) -> Self {
        Self {
            model_id: model_id.to_string(),
            provider: provider.to_string(),
            response: None,
            error: Some(error),
            accepted: false,
        }
    }
}

#[derive(Debug)]
pub(in crate::overlay::computer_control) struct CandidateReport {
    pub(in crate::overlay::computer_control) answer: Result<String>,
    pub(in crate::overlay::computer_control) attempts: Vec<CandidateAttempt>,
}
