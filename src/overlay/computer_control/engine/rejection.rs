use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::overlay::computer_control) enum RejectionKind {
    Request,
    Application,
    ProviderFrame,
}

impl RejectionKind {
    pub(in crate::overlay::computer_control) const ALL: [Self; 3] =
        [Self::Request, Self::Application, Self::ProviderFrame];

    pub(in crate::overlay::computer_control) fn label(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Application => "application",
            Self::ProviderFrame => "provider_frame",
        }
    }
}

#[derive(Debug)]
pub(in crate::overlay::computer_control) struct DataRejection {
    kind: RejectionKind,
    message: String,
}

impl DataRejection {
    pub(in crate::overlay::computer_control) fn request(message: impl ToString) -> Self {
        Self {
            kind: RejectionKind::Request,
            message: message.to_string(),
        }
    }

    pub(in crate::overlay::computer_control) fn application(message: impl ToString) -> Self {
        Self {
            kind: RejectionKind::Application,
            message: message.to_string(),
        }
    }

    pub(in crate::overlay::computer_control) fn provider_frame(message: impl ToString) -> Self {
        Self {
            kind: RejectionKind::ProviderFrame,
            message: message.to_string(),
        }
    }

    pub(in crate::overlay::computer_control) fn classify(
        error: &anyhow::Error,
    ) -> Option<RejectionKind> {
        error.downcast_ref::<Self>().map(|rejection| rejection.kind)
    }
}

impl fmt::Display for DataRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DataRejection {}
