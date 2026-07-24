use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub(crate) type PresetModelDefaults = BTreeMap<String, Vec<(String, String)>>;

/// A staged update whose built-in preset model defaults must be compared after
/// the newly downloaded executable starts.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct PendingPresetModelUpdate {
    pub target_version: String,
    pub previous_models: BTreeMap<String, Vec<(String, String)>>,
}
