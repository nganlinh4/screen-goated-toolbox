//! A loaded history limit follows `maxItemsRange` from
//! `parity-fixtures/history-ui/state-machine.json`; an out-of-range saved value
//! must not prune the whole history at startup.

use super::migrate_config;
use crate::config::Config;

#[test]
fn loaded_history_limit_is_bounded_by_the_shared_range() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/history-ui/state-machine.json"
    ))
    .unwrap();
    let minimum = fixture["maxItemsRange"]["min"].as_u64().unwrap() as usize;
    let maximum = fixture["maxItemsRange"]["max"].as_u64().unwrap() as usize;
    let default = fixture["defaultMaxItems"].as_u64().unwrap() as usize;
    assert_eq!(
        (minimum, maximum),
        (
            crate::config::types::MIN_HISTORY_LIMIT,
            crate::config::types::MAX_HISTORY_LIMIT
        )
    );
    for (saved, expected) in [
        (0, minimum),
        (minimum - 1, minimum),
        (minimum, minimum),
        (default, default),
        (maximum, maximum),
        (maximum + 1, maximum),
        (usize::MAX, maximum),
    ] {
        let mut config = Config {
            max_history_items: saved,
            ..Default::default()
        };
        migrate_config(&mut config);
        assert_eq!(config.max_history_items, expected, "saved {saved}");
    }
}
