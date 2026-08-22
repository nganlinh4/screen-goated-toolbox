use super::migrate_config;
use crate::config::Config;
use crate::model_config::{ModelType, get_all_models};

fn model_ids(model_type: ModelType, count: usize) -> Vec<String> {
    get_all_models()
        .iter()
        .filter(|model| model.enabled && model.model_type == model_type)
        .map(|model| model.id.clone())
        .take(count)
        .collect()
}

#[test]
fn migrate_config_preserves_user_rows_beyond_preparation_targets() {
    let mut config = Config::default();
    let image_default_len = config.model_priority_chains.image_to_text.len();
    let text_default_len = config.model_priority_chains.text_to_text.len();
    config.model_priority_chains.image_to_text =
        model_ids(ModelType::Vision, image_default_len + 3);
    config.model_priority_chains.text_to_text = model_ids(ModelType::Text, text_default_len + 3);

    migrate_config(&mut config);

    assert!(config.model_priority_chains.image_to_text.len() > image_default_len);
    assert!(config.model_priority_chains.text_to_text.len() > text_default_len);
}
