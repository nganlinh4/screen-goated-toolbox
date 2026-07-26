use super::ModelConfig;

pub fn sort_models_for_display(models: &mut [ModelConfig]) {
    models.sort_by(|left, right| {
        left.typical_latency_ms
            .unwrap_or(u32::MAX)
            .cmp(&right.typical_latency_ms.unwrap_or(u32::MAX))
            .then_with(|| left.id.cmp(&right.id))
    });
}

#[cfg(test)]
mod tests {
    use super::sort_models_for_display;
    use crate::model_config::get_all_models;

    #[test]
    fn display_order_sorts_globally_by_speed() {
        let mut models = get_all_models().to_vec();
        sort_models_for_display(&mut models);
        assert!(
            models.windows(2).all(|pair| {
                pair[0].typical_latency_ms.unwrap_or(u32::MAX)
                    <= pair[1].typical_latency_ms.unwrap_or(u32::MAX)
            }),
            "all models must be sorted by latency regardless of provider"
        );
    }

    #[test]
    fn model_id_breaks_equal_latency_ties() {
        let base = get_all_models()[0].clone();
        let mut z_slow = base.clone();
        z_slow.id = "z-slow".to_string();
        z_slow.provider = "z-provider".to_string();
        z_slow.typical_latency_ms = Some(500);
        let mut a_slow = base.clone();
        a_slow.id = "a-slow".to_string();
        a_slow.provider = "a-provider".to_string();
        a_slow.typical_latency_ms = Some(900);
        let mut z_fast = base;
        z_fast.id = "z-fast".to_string();
        z_fast.provider = "z-provider".to_string();
        z_fast.typical_latency_ms = Some(100);

        let mut models = vec![z_slow, a_slow, z_fast];
        sort_models_for_display(&mut models);
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["z-fast", "z-slow", "a-slow"]
        );
    }
}
