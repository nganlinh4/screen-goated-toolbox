//! Bounded admission for artifacts owned by one session.

use super::{TextArtifact, store};

pub(super) const MAX_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_ITEMS: usize = 128;

pub(super) fn check_size(kind: &str, title: &str, url: &str, text: &str) -> anyhow::Result<()> {
    let bytes = [kind.len(), title.len(), url.len(), text.len()]
        .into_iter()
        .try_fold(0usize, usize::checked_add);
    if bytes.is_none_or(|bytes| bytes > MAX_BYTES) {
        anyhow::bail!("artifact exceeds the session memory budget");
    }
    Ok(())
}

pub(super) fn insert(artifact: &TextArtifact) -> anyhow::Result<()> {
    let mut cache = store().lock();
    {
        let owner = &artifact.owner;
        let mut items = 0usize;
        let mut bytes = resident_bytes(artifact);
        for entry in cache
            .values()
            .filter(|entry| entry.owner.same_session(owner))
        {
            items += 1;
            bytes = bytes.saturating_add(resident_bytes(entry));
        }
        if items >= MAX_ITEMS || bytes > MAX_BYTES {
            anyhow::bail!(
                "session artifact memory budget exhausted; narrow the source or start a new session"
            );
        }
    }
    cache.insert(artifact.id.clone(), artifact.clone());
    Ok(())
}

fn resident_bytes(artifact: &TextArtifact) -> usize {
    artifact
        .bytes
        .saturating_add(artifact.kind.len())
        .saturating_add(artifact.title.len())
        .saturating_add(artifact.source_url.len())
}
