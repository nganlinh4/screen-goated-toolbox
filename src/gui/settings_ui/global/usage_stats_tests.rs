use super::*;
use crate::model_config::{ModelConfig, ModelType};

fn model(id: &str, provider: &str, full_name: &str, latency: u32) -> ModelConfig {
    ModelConfig::new(
        id,
        provider,
        id,
        id,
        id,
        full_name,
        ModelType::Text,
        true,
        "10 lượt/ngày",
        "10회/일",
        "10 requests/day",
        false,
        false,
        1,
        latency,
        "test",
    )
}

#[test]
fn google_and_live_share_a_section_without_sharing_endpoint_identity() {
    let models = vec![
        model("google-text", "google", "same", 500),
        model("live-text", "gemini-live", "same", 600),
    ];
    let rows = endpoint_representatives(&models);
    let sections = group_rows(
        rows,
        ProviderEnabled {
            groq: true,
            gemini: true,
            openrouter: true,
            nvidia: true,
            ollama: true,
        },
    );
    assert_eq!(sections["google"].len(), 2);
}

#[test]
fn disabled_explicit_provider_is_hidden_but_future_provider_remains_visible() {
    let toggles = ProviderEnabled {
        groq: false,
        gemini: true,
        openrouter: true,
        nvidia: false,
        ollama: true,
    };
    assert!(!provider_enabled("groq", toggles));
    assert!(provider_enabled("future-provider", toggles));
}

#[test]
fn normal_settings_window_uses_the_shared_wide_dashboard_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/usage-statistics/contract.json"
    )))
    .unwrap();
    let layout = usage_dialog_layout(egui::vec2(
        crate::MIN_WINDOW_WIDTH,
        crate::MIN_WINDOW_HEIGHT,
    ));
    assert_eq!(
        layout.column_count,
        fixture["presentation"]["desktop_preferred_columns"]
            .as_u64()
            .unwrap() as usize
    );
    // The cap is a cap, not the width at the smallest window: at
    // MIN_WINDOW_WIDTH the dialog is narrower than it, and it must still clear
    // the window edges.
    let cap = fixture["presentation"]["desktop_maximum_dialog_width"]
        .as_f64()
        .unwrap() as f32;
    assert_eq!(usage_dialog_layout(egui::vec2(4_096.0, 1_000.0)).width, cap);
    assert!(layout.width <= cap);
    assert!(layout.width < crate::MIN_WINDOW_WIDTH);
    assert_eq!(
        usage_dialog_layout(egui::vec2(crate::MIN_WINDOW_WIDTH, 1_000.0)).body_height,
        fixture["presentation"]["desktop_maximum_body_height"]
            .as_f64()
            .unwrap() as f32
    );
    let columns = usage_stats_table::endpoint_columns(560.0);
    assert_eq!(
        columns.status,
        fixture["presentation"]["desktop_wide_status_column_width"]
            .as_f64()
            .unwrap() as f32
    );
    assert_eq!(
        columns.name,
        fixture["presentation"]["desktop_localized_name_column_width"]
            .as_f64()
            .unwrap() as f32
    );
    assert_eq!(
        usage_stats_table::PROVIDER_NAME_COLUMN_WIDTH,
        fixture["presentation"]["desktop_provider_name_column_width"]
            .as_f64()
            .unwrap() as f32
    );
    assert_eq!(
        usage_stats_table::CELL_GAP,
        fixture["presentation"]["desktop_cell_gap"]
            .as_f64()
            .unwrap() as f32
    );
    assert_eq!(fixture["presentation"]["desktop_borderless_table"], true);
    assert_eq!(
        fixture["presentation"]["provider_endpoint_count_visibility"],
        "hidden"
    );
    assert_eq!(
        fixture["presentation"]["desktop_endpoint_columns"],
        serde_json::json!([
            "intelligence",
            "latency",
            "localized_name",
            "full_name",
            "quota_or_live_usage"
        ])
    );
    assert_eq!(fixture["presentation"]["model_id_visibility"], "always");
    assert_eq!(fixture["presentation"]["model_id_placement"], "inline");
    assert_eq!(fixture["presentation"]["endpoint_identity_lines"], 1);
    assert_eq!(
        fixture["presentation"]["desktop_localized_name_font_size"]
            .as_f64()
            .unwrap() as f32,
        ENDPOINT_NAME_FONT_SIZE
    );
    assert_eq!(
        fixture["presentation"]["desktop_model_id_font_size"]
            .as_f64()
            .unwrap() as f32,
        ENDPOINT_ID_FONT_SIZE
    );
}

#[test]
fn endpoint_columns_fill_each_lane_with_stable_content_independent_starts() {
    let columns = usage_stats_table::endpoint_columns(560.0);
    assert_eq!(
        columns.prefix
            + columns.name
            + columns.id
            + columns.status
            + usage_stats_table::CELL_GAP * 3.0,
        560.0
    );
    assert_eq!(columns.prefix, crate::gui::model_performance::PREFIX_WIDTH);
    let row = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(560.0, 22.0));
    let rects = columns.rects(row);
    assert_eq!(
        rects.name.left(),
        rects.prefix.right() + usage_stats_table::CELL_GAP
    );
    assert_eq!(
        rects.id.left(),
        rects.name.right() + usage_stats_table::CELL_GAP
    );
    assert_eq!(
        rects.status.left(),
        rects.id.right() + usage_stats_table::CELL_GAP
    );
    assert_eq!(rects.status.right(), row.right());

    let header = usage_stats_table::provider_header_rects(row);
    assert_eq!(
        header.name.left(),
        header.icon.right() + usage_stats_table::CELL_GAP
    );
    assert_eq!(
        header.link.left(),
        header.name.right() + usage_stats_table::CELL_GAP
    );
    assert_eq!(header.link.right(), row.right());
}

#[test]
fn missing_live_snapshot_shows_only_the_static_quota() {
    let sample = model("sample", "demo", "sample", 100);
    let theme = AppTheme::from_dark(true);
    let text = LocaleText::get("vi");
    let status = endpoint_status(None, &sample, false, &text, "vi", &theme);
    assert_eq!(status.headlines.len(), 1);
    assert_eq!(status.headlines[0].label, "10 lượt/ngày");
    assert!(status.detail.is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/usage-statistics/contract.json"
        )))
        .unwrap()["presentation"]["missing_snapshot_marker"],
        "none"
    );
}

#[test]
fn live_quota_labels_and_colors_match_the_shared_contract() {
    use crate::usage_stats::UsageMetricKind;

    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/usage-statistics/contract.json"
    )))
    .unwrap();
    let quota = &fixture["presentation"]["live_quota"];
    assert_eq!(
        quota["warning_at_or_below_ratio"].as_f64(),
        Some(LOW_QUOTA_RATIO)
    );
    let theme = AppTheme::from_dark(true);
    let now = 10_000;
    for case in quota["color_cases"].as_array().unwrap() {
        let metric = UsageMetric {
            kind: UsageMetricKind::TokensMinute,
            remaining: case["remaining"].as_str().map(str::to_owned),
            limit: case["limit"].as_str().map(str::to_owned),
            reset: None,
        };
        let snapshot = UsageSnapshot {
            metrics: vec![metric],
            observed_at_unix_seconds: now - case["age_seconds"].as_u64().unwrap(),
        };
        let summary = snapshot_summary(&snapshot, &LocaleText::get("vi"), &theme, now);
        let expected = match case["color"].as_str().unwrap() {
            "green" => theme.success(),
            "amber" => theme.warning(),
            "neutral" => theme.on_surface_variant(),
            other => panic!("unexpected fixture color {other}"),
        };
        assert_eq!(summary.headlines[0].color, expected, "{case}");
    }

    let metric = UsageMetric {
        kind: UsageMetricKind::TokensMinute,
        remaining: Some("9500".into()),
        limit: Some("10000".into()),
        reset: None,
    };
    let ctx = egui::Context::default();
    crate::gui::utils::configure_fonts(&ctx);
    let status_width = usage_stats_table::endpoint_columns(320.0).status;
    let _ = crate::gui::test_support::run_ui(&ctx, egui::RawInput::default(), |ui| {
        for (lang, expected) in quota["example_labels"].as_object().unwrap() {
            let text = LocaleText::get(lang);
            let label = metric_headline(&metric, text.desktop_settings.usage_quota_left);
            assert_eq!(Some(label.as_str()), expected.as_str());
            let width = ui.ctx().fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(
                        label,
                        egui::FontId::proportional(ENDPOINT_STATUS_FONT_SIZE),
                        theme.on_surface_variant(),
                    )
                    .size()
                    .x
            });
            assert!(width <= status_width, "{lang}: {width} > {status_width}");
        }
    });
}

#[test]
fn empty_copy_names_every_provider_that_records_observed_usage() {
    let copy = LocaleText::get("vi").desktop_settings.usage_no_live_data;
    for provider in ["Groq", "OpenRouter"] {
        assert!(copy.contains(provider), "{copy}");
    }
}

#[test]
fn uneven_provider_groups_balance_across_the_two_desktop_lanes() {
    let sample = model("sample", "demo", "sample", 100);
    let sections = BTreeMap::from([
        ("google".to_string(), vec![&sample; 12]),
        ("groq".to_string(), vec![&sample; 7]),
        ("openrouter".to_string(), vec![&sample; 2]),
        ("nvidia".to_string(), vec![&sample; 2]),
        ("google-gtx".to_string(), vec![&sample]),
        ("ollama".to_string(), vec![&sample]),
        ("qrserver".to_string(), vec![&sample]),
        ("taalas".to_string(), vec![&sample]),
    ]);
    let columns = balance_sections(sections, WIDE_COLUMN_COUNT);
    let weights: Vec<usize> = columns
        .iter()
        .map(|column| {
            column
                .iter()
                .map(|(key, rows)| section_weight(key, rows.len()))
                .sum()
        })
        .collect();
    assert_eq!(columns.iter().flatten().count(), 8);
    assert!(weights[0].abs_diff(weights[1]) <= 2, "{weights:?}");
}
