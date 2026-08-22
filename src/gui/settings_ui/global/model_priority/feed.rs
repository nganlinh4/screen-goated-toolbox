use crate::config::Config;
use crate::config::types::LiveModelOverrides;
use crate::retry_model_chain::RetryChainKind;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ManualEdit {
    Replace { old: String, new: String },
    Remove(String),
    Move(String),
    Add(String),
}

pub(super) struct PreparedChain {
    pub(super) visible: Vec<String>,
    pub(super) adaptive: Vec<String>,
    pub(super) live_ids: Vec<String>,
}

/// Resolves both possible toggle states before the editor mutably borrows its
/// authored rows. This keeps full-config cloning out of egui's frame loop.
pub(super) fn prepare_chain(
    config: &Config,
    chain_kind: RetryChainKind,
    authored: &[String],
    overrides: &LiveModelOverrides,
    adaptive_enabled: bool,
) -> PreparedChain {
    let adaptive = visible_chain(config, chain_kind, authored, overrides, true);
    let visible = if adaptive_enabled {
        adaptive.clone()
    } else {
        authored.to_vec()
    };
    PreparedChain {
        visible,
        adaptive,
        live_ids: live_ids(config, chain_kind),
    }
}

/// Builds the rows the editor displays. Live entries deliberately use the same
/// shape as authored entries so each receives the normal selector and actions.
pub(super) fn visible_chain(
    config: &Config,
    chain_kind: RetryChainKind,
    authored: &[String],
    overrides: &LiveModelOverrides,
    adaptive_enabled: bool,
) -> Vec<String> {
    if !adaptive_enabled {
        return authored.to_vec();
    }
    chain_kind.adaptive_chain(config, authored, overrides)
}

pub(super) fn live_ids(config: &Config, chain_kind: RetryChainKind) -> Vec<String> {
    crate::model_feed::store::offered_models(config, chain_kind.target_model_type())
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

fn is_live_owned(id: &str, live_ids: &[String], overrides: &LiveModelOverrides) -> bool {
    live_ids.iter().any(|live| live == id)
        || overrides.pinned.iter().any(|pinned| pinned == id)
        || overrides.excluded.iter().any(|excluded| excluded == id)
}

/// Persists visible rows while narrowing manual intent to the affected live row.
/// Live stays enabled: pins remain authored anchors, exclusions are not offered
/// again, and every other live row remains under formula ownership.
pub(super) fn commit_manual_edits(
    authored: &mut Vec<String>,
    visible: Vec<String>,
    overrides: &mut LiveModelOverrides,
    live_ids: &[String],
    edits: &[ManualEdit],
) -> bool {
    for edit in edits {
        match edit {
            ManualEdit::Replace { old, new } => {
                if is_live_owned(old, live_ids, overrides) {
                    overrides.exclude(old);
                }
                if is_live_owned(new, live_ids, overrides) {
                    overrides.pin(new);
                }
            }
            ManualEdit::Remove(id) => {
                if is_live_owned(id, live_ids, overrides) {
                    overrides.exclude(id);
                }
            }
            ManualEdit::Move(id) | ManualEdit::Add(id) => {
                if is_live_owned(id, live_ids, overrides) {
                    overrides.pin(id);
                }
            }
        }
    }
    let has_live_row = visible.iter().any(|id| {
        live_ids.iter().any(|live| live == id) || overrides.pinned.iter().any(|pinned| pinned == id)
    });
    *authored = visible;
    has_live_row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_adaptation_returns_the_authored_chain_exactly() {
        let config = Config::default();
        let authored = vec!["first".to_string(), "second".to_string()];
        assert_eq!(
            visible_chain(
                &config,
                RetryChainKind::TextToText,
                &authored,
                &LiveModelOverrides::default(),
                false,
            ),
            authored
        );
    }

    #[test]
    fn moving_one_live_row_pins_only_that_row() {
        let mut authored = vec!["old".to_string()];
        let visible = vec!["leader".to_string(), "live-a".to_string()];
        let live = vec!["live-a".to_string(), "live-b".to_string()];
        let mut overrides = LiveModelOverrides::default();

        commit_manual_edits(
            &mut authored,
            visible.clone(),
            &mut overrides,
            &live,
            &[ManualEdit::Move("live-a".to_string())],
        );

        assert_eq!(authored, visible);
        assert_eq!(overrides.pinned, ["live-a"]);
        assert!(overrides.excluded.is_empty());
    }

    #[test]
    fn deleting_live_excludes_it_without_affecting_other_offers() {
        let mut authored = vec!["live-a".to_string(), "live-b".to_string()];
        let mut overrides = LiveModelOverrides::default();
        commit_manual_edits(
            &mut authored,
            vec!["live-b".to_string()],
            &mut overrides,
            &["live-a".to_string(), "live-b".to_string()],
            &[ManualEdit::Remove("live-a".to_string())],
        );

        assert_eq!(overrides.excluded, ["live-a"]);
        assert!(overrides.pinned.is_empty());
    }

    #[test]
    fn replacing_live_excludes_old_and_pins_live_replacement() {
        let mut authored = Vec::new();
        let mut overrides = LiveModelOverrides::default();
        let live = vec!["live-old".to_string(), "live-new".to_string()];
        commit_manual_edits(
            &mut authored,
            vec!["live-new".to_string()],
            &mut overrides,
            &live,
            &[ManualEdit::Replace {
                old: "live-old".to_string(),
                new: "live-new".to_string(),
            }],
        );

        assert_eq!(overrides.pinned, ["live-new"]);
        assert_eq!(overrides.excluded, ["live-old"]);
    }

    #[test]
    fn adding_an_excluded_live_identity_restores_it_as_a_pin() {
        let mut authored = Vec::new();
        let mut overrides = LiveModelOverrides {
            pinned: Vec::new(),
            excluded: vec!["live-a".to_string()],
        };
        commit_manual_edits(
            &mut authored,
            vec!["live-a".to_string()],
            &mut overrides,
            &[],
            &[ManualEdit::Add("live-a".to_string())],
        );

        assert_eq!(overrides.pinned, ["live-a"]);
        assert!(overrides.excluded.is_empty());
    }

    #[test]
    fn deleting_the_final_live_row_ends_live_ownership() {
        let mut authored = vec!["local".to_string(), "live-a".to_string()];
        let mut overrides = LiveModelOverrides::default();
        let can_remain_live = commit_manual_edits(
            &mut authored,
            vec!["local".to_string()],
            &mut overrides,
            &["live-a".to_string()],
            &[ManualEdit::Remove("live-a".to_string())],
        );

        assert!(!can_remain_live);
        assert_eq!(authored, ["local"]);
    }
}
