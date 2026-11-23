//! Stable failure codes shared by creation hosts and their WebViews.
//!
//! A command reply carries only one of these codes. Each WebView owns the
//! localized message for a code (see `ui-shared/creation-error-code.ts`), and
//! the host logs the detailed cause. A failure without an explicit code is
//! `operation_failed`, so host wording never reaches the interface.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorCode {
    /// The requested result name is not a valid filename.
    NameInvalid,
    /// Another file in the result folder already uses the requested name.
    NameTaken,
    /// The result file is no longer on disk.
    ResultMissing,
    /// The result is in use by a pending delivery or recovery; retry later.
    ResultBusy,
    /// The command queue cannot accept more work right now.
    QueueFull,
    OperationFailed,
}

impl ErrorCode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NameInvalid => "name_invalid",
            Self::NameTaken => "name_taken",
            Self::ResultMissing => "result_missing",
            Self::ResultBusy => "result_busy",
            Self::QueueFull => "queue_full",
            Self::OperationFailed => "operation_failed",
        }
    }
}

#[derive(Debug)]
pub(crate) struct CreationError {
    code: ErrorCode,
    detail: String,
}

impl CreationError {
    pub(crate) fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub(crate) fn code(&self) -> ErrorCode {
        self.code
    }
}

impl From<String> for CreationError {
    fn from(detail: String) -> Self {
        Self::new(ErrorCode::OperationFailed, detail)
    }
}

impl From<&str> for CreationError {
    fn from(detail: &str) -> Self {
        Self::new(ErrorCode::OperationFailed, detail)
    }
}

impl fmt::Display for CreationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}
