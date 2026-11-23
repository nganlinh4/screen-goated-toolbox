use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RestoreError {
    Invalid,
    Deferred,
}

impl From<()> for RestoreError {
    fn from(_: ()) -> Self {
        Self::Invalid
    }
}

pub(crate) fn validate_directory(path: &Path) -> Result<(), RestoreError> {
    crate::overlay::creation_intent_journal::validate_persisted_path(path).map_err(|_| ())?;
    crate::overlay::creation_path::normalized_absolute(path).map_err(|_| ())?;
    (std::fs::canonicalize(path).ok().as_deref() == Some(path))
        .then_some(())
        .ok_or(RestoreError::Deferred)
}
