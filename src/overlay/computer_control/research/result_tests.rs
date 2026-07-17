use super::super::diagnostics::FailureClass;
use super::*;

fn page(title: &str, url: &str, text: &str) -> Value {
    json!({"ok": true, "page": {"title": title, "url": url, "text": text}})
}

fn source(title: &str, url: &str, marker: &str) -> ResearchSource {
    ResearchSource {
        title: title.into(),
        title_truncated: false,
        url: url.into(),
        query_omitted: false,
        source_kind: "web",
        char_count: json!(600),
        body: format!("{marker} {}", "body ".repeat(120)),
        semantic_annotations: format!("{marker}-SEMANTIC {}", "note ".repeat(20)),
        structural_annotations: format!("[section] {marker} :: bound value"),
        capture_truncated: false,
        semantic_truncated: false,
        structural_truncated: false,
        inaccessible_iframe_count: 0,
        artifact: json!({"id": marker, "preview": "must not escape"}),
    }
}

#[test]
fn readable_source_needs_title_url_and_nontrivial_text() {
    let text = "evidence ".repeat(12);
    assert!(source_is_usable(&page(
        "Source",
        "https://example.test/fact",
        &text
    )));
    assert!(!source_is_usable(&page(
        "",
        "https://example.test/fact",
        &text
    )));
    assert!(!source_is_usable(&page(
        "Source",
        "https://example.test/fact",
        "short"
    )));
}

#[test]
fn duplicate_content_hash_is_independent_of_url() {
    let text = "same evidence ".repeat(10);
    let first = page("One", "https://one.test/fact", &text);
    let second = page("Two", "https://two.test/copy", &text);
    assert_eq!(source_content_hash(&first), source_content_hash(&second));
}

#[test]
fn source_kind_uses_only_supported_scheme_and_host() {
    for url in [
        "https://subdomain.example.test/reference",
        "http://127.0.0.1:8080/evidence",
        "https://[::1]/evidence",
    ] {
        assert_eq!(source_kind(url), "web", "unexpected kind for {url}");
    }

    for url in [
        "not a url",
        "file:///local/evidence",
        "mailto:a@example.test",
        "https://",
    ] {
        assert_eq!(source_kind(url), "unknown", "unexpected kind for {url}");
    }
}

#[test]
fn evidence_budget_is_shared_across_every_source() {
    let sources = [
        source("One", "https://one.test/", "ONE-MARKER"),
        source("Two", "https://two.test/", "TWO-MARKER"),
        source("Three", "https://three.test/", "THREE-MARKER"),
    ];
    let shaped = fair_source_responses(&sources, 900, "marker evidence");
    let excerpts = shaped
        .iter()
        .map(|source| source["excerpt"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(excerpts[0].contains("ONE-MARKER"));
    assert!(excerpts[1].contains("TWO-MARKER"));
    assert!(excerpts[2].contains("THREE-MARKER"));
    let total = excerpts
        .iter()
        .map(|excerpt| excerpt.chars().count())
        .sum::<usize>();
    assert!(total > 0 && total <= 900);
    assert!(
        shaped
            .iter()
            .all(|source| source.get("text_preview").is_none())
    );
    assert!(
        shaped
            .iter()
            .all(|source| source["artifact"].get("preview").is_none())
    );
}

#[test]
fn zero_and_partial_retrieval_are_shaped_truthfully() {
    let mut diagnostics = SourceDiagnostics::default();
    diagnostics.failed(
        FailureClass::Source,
        "source_page_unavailable",
        "document never became readable",
    );
    let zero = research_result(
        "facts",
        "facts",
        "domain_restricted",
        &[],
        source_policy::Coverage {
            assessed: true,
            covered: vec![],
            missing: vec!["one.test".into()],
        },
        2,
        &diagnostics,
    );
    assert_eq!(zero["ok"], false);
    assert_eq!(zero["code"], "ERR_RESEARCH_NO_USABLE_SOURCES");
    assert_eq!(zero["capture_complete"], false);
    assert_eq!(zero["coverage_complete"], false);
    assert_eq!(zero["failure_stage"], "source_retrieval");
    assert!(zero.get("effect_may_have_occurred").is_none());
    assert_eq!(zero["source_diagnostics"]["discovery_failure_count"], 0);
    assert_eq!(zero["source_diagnostics"]["source_failure_count"], 1);
    assert_eq!(zero["source_diagnostics"]["failed_source_count"], 1);

    let sources = [source("One", "https://one.test/", "ONE")];
    let partial = research_result(
        "facts",
        "facts",
        "domain_restricted",
        &sources,
        source_policy::Coverage {
            assessed: true,
            covered: vec!["one.test".into()],
            missing: vec!["two.test".into()],
        },
        1,
        &SourceDiagnostics::default(),
    );
    assert_eq!(partial["ok"], true);
    assert_eq!(partial["retrieval_status"], "partial");
    assert_eq!(partial["valid_source_count"], 1);
    assert_eq!(partial["coverage_complete"], false);
    assert_eq!(partial["failure_stage"], Value::Null);

    let no_policy_sources = research_result(
        "facts",
        "facts",
        "best_available",
        &[],
        source_policy::Coverage {
            assessed: false,
            covered: vec![],
            missing: vec![],
        },
        0,
        &SourceDiagnostics::default(),
    );
    assert_eq!(no_policy_sources["coverage_complete"], false);
    assert_eq!(no_policy_sources["coverage_assessed"], false);
    assert_eq!(no_policy_sources["failure_stage"], "discovery");

    let broad_sources = research_result(
        "facts",
        "facts",
        "broad",
        &sources,
        source_policy::Coverage {
            assessed: false,
            covered: vec![],
            missing: vec![],
        },
        1,
        &SourceDiagnostics::default(),
    );
    assert_eq!(broad_sources["coverage_assessed"], false);
    assert_eq!(broad_sources["coverage_complete"], false);
    assert_eq!(broad_sources["retrieval_status"], "usable");
}

#[test]
fn failure_stage_and_counts_preserve_discovery_source_and_mixed_failures() {
    let result_for = |diagnostics: &SourceDiagnostics| {
        research_result(
            "facts",
            "facts",
            "broad",
            &[],
            source_policy::Coverage {
                assessed: false,
                covered: vec![],
                missing: vec![],
            },
            2,
            diagnostics,
        )
    };

    let mut discovery = SourceDiagnostics::default();
    discovery.failed(FailureClass::Discovery, "search_failed", "unavailable");
    let result = result_for(&discovery);
    assert_eq!(result["failure_stage"], "discovery");
    assert_eq!(result["source_diagnostics"]["discovery_failure_count"], 1);
    assert_eq!(result["source_diagnostics"]["source_failure_count"], 0);

    discovery.failed(FailureClass::Source, "source_failed", "unavailable");
    let result = result_for(&discovery);
    assert_eq!(result["failure_stage"], "mixed");
    assert_eq!(result["source_diagnostics"]["failure_count"], 2);

    let evaluation = result_for(&SourceDiagnostics::default());
    assert_eq!(evaluation["failure_stage"], "source_evaluation");
}

#[test]
fn source_diagnostics_separate_initial_and_follow_up_discovery() {
    let diagnostics = SourceDiagnostics {
        initial_candidate_count: 4,
        source_link_page_count: 3,
        follow_up_candidate_count: 2,
        ..SourceDiagnostics::default()
    };
    let result = research_result(
        "facts",
        "facts",
        "broad",
        &[],
        source_policy::Coverage {
            assessed: false,
            covered: vec![],
            missing: vec![],
        },
        6,
        &diagnostics,
    );
    assert_eq!(result["source_diagnostics"]["candidate_count"], 6);
    assert_eq!(result["source_diagnostics"]["initial_candidate_count"], 4);
    assert_eq!(result["source_diagnostics"]["source_link_page_count"], 3);
    assert_eq!(result["source_diagnostics"]["follow_up_candidate_count"], 2);
}

#[test]
fn full_artifact_hash_wins_over_a_shared_preview() {
    let first = json!({
        "page": {"text": "shared preview"},
        "artifact": {"sha256": "AA11"}
    });
    let second = json!({
        "page": {"text": "shared preview"},
        "artifact": {"sha256": "BB22"}
    });
    assert_eq!(source_content_hash(&first), "aa11");
    assert_ne!(source_content_hash(&first), source_content_hash(&second));
}

#[test]
fn structural_capture_hash_and_annotations_survive_research_shaping() {
    let page = json!({
        "page": {
            "title": "Structured source",
            "url": "https://source.test/fact",
            "text": "Plan Alpha 10 Plan Beta 20",
            "structural_annotations": "[section] Plan Alpha :: 10\n[section] Plan Beta :: 20",
            "structural_truncated": false,
        },
        "artifact": {
            "sha256": "a".repeat(64),
            "capture_sha256": "b".repeat(64),
        },
    });
    assert_eq!(source_content_hash(&page), "b".repeat(64));
    let source = source_from_page(&page, "https://source.test/fact".into(), false);
    let excerpt = source_excerpt(&source, 400, "Plan Beta");
    assert!(excerpt.contains("[visible structural annotations]"));
    assert!(excerpt.contains("[section] Plan Beta :: 20"));
}

#[test]
fn fallback_sampling_reaches_late_document_evidence() {
    let body = format!(
        "{}\nMIDDLE-EVIDENCE\n{}\nTAIL-EVIDENCE",
        "header ".repeat(100),
        "body ".repeat(140)
    );
    let sampled = relevant_body_sample(&body, 600, "unmatched query terms");
    assert!(sampled.contains("TAIL-EVIDENCE"));
    assert!(sampled.contains("MIDDLE-EVIDENCE"));
}

#[test]
fn matched_passages_do_not_spend_evidence_budget_on_unrelated_document_spans() {
    let body = format!(
        "{}\nTarget renewal is 12 units per annual term.\n{}",
        "unrelated header material\n".repeat(100),
        "unrelated footer material\n".repeat(100),
    );
    let sampled = relevant_body_sample(&body, 500, "target renewal annual");
    assert!(sampled.contains("Target renewal is 12 units"));
    assert!(!sampled.contains("[document span sample]"));
}

#[test]
fn effective_evidence_purpose_drives_excerpt_ranking() {
    let filler = (0..180)
        .map(|index| format!("general catalog description {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut focused = source("Plan", "https://plan.test/", "GENERAL");
    focused.body = format!(
        "{filler}\nTemporary offer applies only initially.\nThe standard recurring rate is 12 units per annual term.\n{filler}"
    );
    focused.semantic_annotations = "pricing details".into();

    let result = research_result(
        "plan pricing",
        "plan pricing standard recurring annual term",
        "broad",
        &[focused],
        source_policy::Coverage {
            assessed: false,
            covered: vec![],
            missing: vec![],
        },
        1,
        &SourceDiagnostics::default(),
    );

    let excerpt = result["sources"][0]["excerpt"].as_str().unwrap();
    assert!(excerpt.contains("standard recurring rate"));
}

#[test]
fn separated_value_qualifier_survives_exact_scope_ranking() {
    let filler = (0..120)
        .map(|index| format!("unrelated catalog material {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "Household plan\n12 units per annual term.\n{filler}\nThe lower displayed amount is temporary and applies only to new accounts."
    );

    let excerpt = relevant_body_sample(
        &body,
        760,
        "household standard recurring annual temporary new accounts",
    );

    assert!(excerpt.contains("12 units per annual term"), "{excerpt}");
    assert!(
        excerpt.contains("temporary and applies only to new accounts"),
        "{excerpt}"
    );
}

#[test]
fn unannotated_pages_still_rank_relevant_body_evidence() {
    let filler = (0..180)
        .map(|index| format!("unrelated navigation item {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut focused = source("Plan", "https://plan.test/", "GENERAL");
    focused.body =
        format!("{filler}\nThe standard recurring rate is 12 units per annual term.\n{filler}");
    focused.semantic_annotations.clear();
    focused.structural_annotations.clear();

    let excerpt = source_excerpt(&focused, 500, "standard recurring annual term");

    assert!(excerpt.contains("standard recurring rate"));
    assert!(!excerpt.starts_with("unrelated navigation item 0"));
}

#[test]
fn contextual_ranking_keeps_values_and_qualifiers_near_a_relevant_heading() {
    let generic = (0..80)
        .map(|index| {
            format!("Account access and shared vault support for family members detail {index}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "{generic}\nFamilies\nPeace of mind for your entire family.\n$4.49\nUSD\n$5.99\nUSD\nPer month. Paid annually.\nEverything from the individual plan, plus shared vaults\n{generic}"
    );

    let excerpt = relevant_body_sample(
        &body,
        900,
        "family plan pricing standard annual renewal shared vaults",
    );

    assert!(excerpt.contains("$4.49"), "{excerpt}");
    assert!(excerpt.contains("$5.99"), "{excerpt}");
    assert!(excerpt.contains("Per month. Paid annually."), "{excerpt}");
}

#[test]
fn currency_values_survive_a_broader_feature_dense_document() {
    let feature_matrix = (0..70)
        .map(|index| {
            format!(
                "Family plan compatibility shared access desktop mobile credential feature {index}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "{feature_matrix}\nHousehold tier\nIntroductory amount\n$7.25\nStandard renewal\n$11.50\nPer month. Paid annually.\n{feature_matrix}"
    );

    let excerpt = relevant_body_sample(
        &body,
        900,
        "family plan pricing standard annual renewal shared access desktop mobile",
    );

    assert!(excerpt.contains("$7.25"), "{excerpt}");
    assert!(excerpt.contains("$11.50"), "{excerpt}");
    assert!(excerpt.contains("Per month. Paid annually."), "{excerpt}");
}

#[test]
fn ranked_center_survives_when_an_overlapping_context_starts_with_long_lines() {
    let prefix = (0..100)
        .map(|index| format!("unrelated introduction material {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let suffix = (0..300)
        .map(|index| format!("unrelated appendix material {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let long_context = "extended descriptive context without another query term ".repeat(16);
    let body = format!(
        "{prefix}\nrenewal\n{long_context}\n{long_context}\n{long_context}\n{long_context}\n{long_context}\n$4.99\nCharged once per twelve-month term.\n{suffix}"
    );

    let excerpt = relevant_body_sample(&body, 900, "renewal");

    assert!(excerpt.contains("$4.99"), "{excerpt}");
    assert!(
        excerpt.contains("Charged once per twelve-month term."),
        "{excerpt}"
    );
}

#[test]
fn known_capture_truncation_prevents_a_usable_claim() {
    let mut truncated = source("Large", "https://large.test/", "FACT");
    truncated.capture_truncated = true;
    let result = research_result(
        "facts",
        "facts",
        "broad",
        &[truncated],
        source_policy::Coverage {
            assessed: false,
            covered: vec![],
            missing: vec![],
        },
        1,
        &SourceDiagnostics::default(),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["retrieval_status"], "partial");
    assert_eq!(result["capture_complete"], false);
    assert_eq!(result["sources"][0]["capture_truncated"], true);
}

#[test]
fn inaccessible_visible_frames_make_retrieval_partial() {
    let mut incomplete = source("Framed", "https://framed.test/", "FACT");
    incomplete.inaccessible_iframe_count = 2;
    let result = research_result(
        "facts",
        "facts",
        "broad",
        &[incomplete],
        source_policy::Coverage {
            assessed: false,
            covered: vec![],
            missing: vec![],
        },
        1,
        &SourceDiagnostics::default(),
    );
    assert_eq!(result["retrieval_status"], "partial");
    assert_eq!(result["capture_complete"], false);
    assert_eq!(result["inaccessible_iframe_count"], 2);
    assert_eq!(result["sources"][0]["inaccessible_iframe_count"], 2);
}

#[test]
fn page_iframe_completeness_uses_inaccessible_not_invisible_count() {
    let text = "evidence ".repeat(12);
    let page = json!({
        "page": {
            "title": "Source",
            "url": "https://example.test/fact",
            "text": text,
            "invisible_iframes": 7,
            "inaccessible_iframes": 0,
        }
    });
    let source = source_from_page(&page, "https://example.test/fact".into(), false);
    assert_eq!(source.inaccessible_iframe_count, 0);
}

#[test]
fn aggregate_model_visible_budget_includes_all_source_metadata() {
    let mut sources = (0..5)
        .map(|index| {
            source(
                &format!("{index}-{}", "title".repeat(2000)),
                &format!("https://{index}.test/{}", "path/".repeat(1000)),
                "FACT",
            )
        })
        .collect::<Vec<_>>();
    for source in &mut sources {
        source.title = format!("\0\n\"\\{}", source.title);
        source.body = "\0\n\"\\evidence".repeat(3000);
        source.semantic_annotations = "\u{0001}\tsemantic".repeat(1000);
        source.artifact = json!({
            "id": "artifact".repeat(1000),
            "path": "must-not-enter-model-budget".repeat(1000),
            "sha256": "a".repeat(1000),
        });
    }
    let result = research_result(
        &"query".repeat(1000),
        &"effective".repeat(1000),
        &"policy".repeat(1000),
        &sources,
        source_policy::Coverage {
            assessed: true,
            covered: vec!["covered".repeat(1000)],
            missing: vec![],
        },
        5,
        &SourceDiagnostics::default(),
    );

    assert!(serialized_json_bytes(&result) <= MAX_MODEL_VISIBLE_BYTES);
    assert_eq!(
        result["model_visible_byte_count"],
        serialized_json_bytes(&result)
    );
    assert_eq!(result["model_visible_byte_limit"], MAX_MODEL_VISIBLE_BYTES);
    assert_eq!(result["query_truncated"], true);
    assert_eq!(result["sources"][0]["title_truncated"], true);
    assert_eq!(result["sources"][0]["url_omitted"], true);
    assert_eq!(result["sources"][0]["url_omission_reason"], "too_long");
    assert!(result["sources"][0]["artifact"].get("path").is_none());
}
