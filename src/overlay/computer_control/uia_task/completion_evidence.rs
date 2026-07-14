//! Bounded, provenance-labelled verifier evidence, separate from the acting trail.
use serde_json::{Map, Value, json};

use super::super::controller::world::SurfaceIdentity;
use super::evidence_provenance::EvidenceProvenance;

mod lineage;
mod retention;
use retention::LedgerEntry;

const MAX_ENTRIES: usize = 12;
const MAX_EARLY_ENTRIES: usize = 4;
const MAX_RECENT_ENTRIES: usize = MAX_ENTRIES - MAX_EARLY_ENTRIES;
const MAX_TOTAL_BYTES: usize = 24 * 1024;
const MAX_ENTRY_BYTES: usize = 1_900;
const MAX_DEPTH: usize = 6;
const MAX_ARRAY_ITEMS: usize = 16;
const MAX_OBJECT_FIELDS: usize = 24;
const MAX_STRING_CHARS: usize = 320;
const MAX_KEY_CHARS: usize = 64;
const MAX_NODES: usize = 160;

#[derive(Debug, Default)]
pub(super) struct CompletionEvidence {
    entries: Vec<LedgerEntry>,
    provider_sources: [Option<String>; 2],
    total_bytes: usize,
    omitted_entries: usize,
}

impl CompletionEvidence {
    pub(super) fn record(&mut self, tool: &str, result: &Value, provenance: EvidenceProvenance) {
        let entry = LedgerEntry::new(compact_entry(tool, None, result, provenance), provenance);
        if entry.len() <= MAX_TOTAL_BYTES {
            self.total_bytes += entry.len();
            self.entries.push(entry);
            self.enforce_bounds();
        }
    }

    pub(super) fn record_job_source(&mut self, source: &super::FrameSource) {
        self.record(
            "job_source",
            &json!({
                "frame_id": source.frame_id,
                "surface": source.surface,
            }),
            EvidenceProvenance::JobSource,
        );
    }

    pub(super) fn record_provider_source(&mut self, source: &super::FrameSource) {
        let slot = match source.surface {
            SurfaceIdentity::Native { .. } => 0,
            SurfaceIdentity::Browser { .. } => 1,
        };
        if self.provider_sources[slot].is_some() {
            return;
        }
        let entry = compact_entry(
            "provider_source",
            None,
            &json!({"frame_id": source.frame_id, "surface": source.surface}),
            EvidenceProvenance::ProviderSource,
        );
        self.total_bytes += entry.len();
        self.provider_sources[slot] = Some(entry);
        self.enforce_bounds();
    }

    pub(super) fn record_grounded_surface(
        &mut self,
        title: &str,
        url: &str,
        identity: &SurfaceIdentity,
    ) {
        self.record(
            "grounded_surface",
            &json!({
                "ok": true,
                "title": title,
                "url": url,
                "identity": identity,
            }),
            EvidenceProvenance::GroundedSurface,
        );
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.provider_sources = Default::default();
        self.total_bytes = 0;
        self.omitted_entries = 0;
    }

    pub(super) fn context(&self) -> String {
        let retained = self
            .provider_sources
            .iter()
            .filter_map(Option::as_deref)
            .chain(self.entries.iter().map(LedgerEntry::as_str))
            .collect::<Vec<_>>();
        if retained.is_empty() {
            "none".to_string()
        } else if self.omitted_entries == 0 {
            retained.join("\n")
        } else {
            format!(
                "{}\n{}",
                json!({
                    "evidence_ledger_bounds": {
                        "retained_early_entries": self.entries.len().min(MAX_EARLY_ENTRIES),
                        "retained_recent_entries": self.entries.len().saturating_sub(MAX_EARLY_ENTRIES),
                        "omitted_middle_entries": self.omitted_entries,
                    }
                }),
                retained.join("\n")
            )
        }
    }

    fn enforce_bounds(&mut self) {
        while self.entries.len() > MAX_ENTRIES
            || (self.total_bytes > MAX_TOTAL_BYTES && !self.entries.is_empty())
        {
            let index = retention::eviction_index(&self.entries, MAX_EARLY_ENTRIES);
            let removed = self.entries.remove(index);
            self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            self.omitted_entries += 1;
        }
        debug_assert!(self.entries.len() <= MAX_ENTRIES);
        debug_assert!(self.total_bytes <= MAX_TOTAL_BYTES);
        debug_assert!(
            self.entries.len() <= MAX_EARLY_ENTRIES + MAX_RECENT_ENTRIES,
            "retention partition must cover every slot"
        );
    }
}

fn compact_entry(
    tool: &str,
    request: Option<&Value>,
    result: &Value,
    provenance: EvidenceProvenance,
) -> String {
    let mut nodes_left = MAX_NODES;
    let verifier_result = provenance.verifier_result_with_request(request, result);
    let evidence = sanitize(&verifier_result, 0, &mut nodes_left).unwrap_or(Value::Null);
    let entry = json!({
        "tool": tool.chars().take(MAX_KEY_CHARS).collect::<String>(),
        "provenance": provenance.as_str(),
        "result": evidence,
    })
    .to_string();
    if entry.len() <= MAX_ENTRY_BYTES {
        entry
    } else {
        bounded_preview(tool, provenance, &entry)
    }
}

fn sanitize(value: &Value, depth: usize, nodes_left: &mut usize) -> Option<Value> {
    if depth >= MAX_DEPTH || *nodes_left == 0 {
        return None;
    }
    *nodes_left -= 1;
    match value {
        Value::Object(object) => {
            let mut clean = Map::new();
            let retained = evenly_spaced_indices(object.len(), MAX_OBJECT_FIELDS);
            for (position, index) in retained.iter().copied().enumerate() {
                let Some((field, child)) = object.iter().nth(index) else {
                    continue;
                };
                let remaining = retained.len().saturating_sub(position).max(1);
                let child = sanitize_fair_child(child, depth + 1, nodes_left, remaining)
                    .unwrap_or_else(structural_budget_marker);
                insert_unique(&mut clean, field, child);
            }
            if object.len() > retained.len() {
                insert_unique(
                    &mut clean,
                    "__evidence_bounds",
                    json!({
                        "source_fields": object.len(),
                        "retained_fields": retained.len(),
                        "omitted_fields": object.len() - retained.len(),
                    }),
                );
            }
            Some(Value::Object(clean))
        }
        Value::Array(items) => {
            let retained = evenly_spaced_indices(items.len(), MAX_ARRAY_ITEMS);
            let mut clean =
                Vec::with_capacity(retained.len() + usize::from(items.len() > retained.len()));
            for (position, index) in retained.iter().copied().enumerate() {
                let remaining = retained.len().saturating_sub(position).max(1);
                clean.push(
                    sanitize_fair_child(&items[index], depth + 1, nodes_left, remaining)
                        .unwrap_or_else(structural_budget_marker),
                );
            }
            if items.len() > retained.len() {
                clean.push(json!({
                    "__evidence_bounds": {
                        "source_items": items.len(),
                        "retained_items": retained.len(),
                        "omitted_items": items.len() - retained.len(),
                    }
                }));
            }
            Some(Value::Array(clean))
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'['))
                && let Ok(parsed) = serde_json::from_str::<Value>(trimmed)
            {
                return sanitize(&parsed, depth + 1, nodes_left);
            }
            if looks_binary_like(text) {
                return Some(json!({
                    "omitted": "binary_like_string",
                    "byte_count": text.len(),
                }));
            }
            Some(Value::String(truncate_chars(text, MAX_STRING_CHARS)))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Some(value.clone()),
    }
}

fn looks_binary_like(value: &str) -> bool {
    if value.chars().any(|ch| ch == '\0') {
        return true;
    }
    value.split_once(',').is_some_and(|(prefix, _)| {
        prefix
            .get(..5)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
            && prefix
                .split(';')
                .any(|part| part.eq_ignore_ascii_case("base64"))
    })
}

fn sanitize_fair_child(
    child: &Value,
    depth: usize,
    nodes_left: &mut usize,
    siblings_left: usize,
) -> Option<Value> {
    if *nodes_left == 0 {
        return None;
    }
    let allowance = (*nodes_left / siblings_left.max(1)).max(1);
    let mut local_nodes = allowance;
    let clean = sanitize(child, depth, &mut local_nodes);
    *nodes_left = (*nodes_left).saturating_sub(allowance - local_nodes);
    clean
}

fn structural_budget_marker() -> Value {
    json!({"omitted": "structural_budget"})
}

fn evenly_spaced_indices(len: usize, limit: usize) -> Vec<usize> {
    if len == 0 || limit == 0 {
        return Vec::new();
    }
    if len <= limit {
        return (0..len).collect();
    }
    if limit <= 1 {
        return vec![0];
    }
    (0..limit)
        .map(|slot| slot * (len - 1) / (limit - 1))
        .collect()
}

fn insert_unique(clean: &mut Map<String, Value>, field: &str, value: Value) {
    let base = truncate_chars(field, MAX_KEY_CHARS);
    if !clean.contains_key(&base) {
        clean.insert(base, value);
        return;
    }
    for ordinal in 2usize.. {
        let suffix = format!("~{ordinal}");
        let prefix: String = base
            .chars()
            .take(MAX_KEY_CHARS.saturating_sub(suffix.chars().count()))
            .collect();
        let candidate = format!("{prefix}{suffix}");
        if !clean.contains_key(&candidate) {
            clean.insert(candidate, value);
            return;
        }
    }
}

fn bounded_preview(tool: &str, provenance: EvidenceProvenance, serialized: &str) -> String {
    let tool: String = tool.chars().take(MAX_KEY_CHARS).collect();
    let mut segment_bytes = MAX_ENTRY_BYTES.saturating_sub(384) / 6;
    loop {
        let candidate = json!({
            "tool": tool,
            "provenance": provenance.as_str(),
            "result_segments": sampled_segments(serialized, 6, segment_bytes),
            "truncated": true,
        })
        .to_string();
        if candidate.len() <= MAX_ENTRY_BYTES || segment_bytes == 0 {
            return candidate;
        }
        let excess_per_segment = (candidate.len() - MAX_ENTRY_BYTES).div_ceil(6);
        segment_bytes = segment_bytes.saturating_sub(excess_per_segment + 4);
    }
}

fn sampled_segments(value: &str, count: usize, segment_bytes: usize) -> Vec<String> {
    if value.is_empty() || count == 0 || segment_bytes == 0 {
        return Vec::new();
    }
    if value.len() <= segment_bytes {
        return vec![value.to_string()];
    }
    let width = segment_bytes.min(value.len());
    let max_start = value.len().saturating_sub(width);
    let slots = count.min(max_start.saturating_add(1));
    (0..slots)
        .map(|slot| {
            let nominal = if slots <= 1 {
                0
            } else {
                slot * max_start / (slots - 1)
            };
            let mut start = nominal;
            while start < value.len() && !value.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (start + width).min(value.len());
            while end > start && !value.is_char_boundary(end) {
                end -= 1;
            }
            value[start..end].to_string()
        })
        .collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut short: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::super::super::controller::world::BrowserWindowIdentity;
    use super::*;

    const BROWSER_WINDOW: BrowserWindowIdentity = BrowserWindowIdentity {
        browser_window_id: 2,
        hwnd: 3,
        pid: 4,
        generation: 5,
    };

    #[test]
    fn relational_json_string_and_later_verified_url_survive() {
        let rows = json!([
            {"rank": 5, "score": 81, "label": "opaque-row-a"},
            {"rank": 6, "score": 144, "label": "opaque-row-b"},
            {"rank": 7, "score": 109, "label": "opaque-row-c"},
            {"rank": 8, "score": 233, "label": "opaque-row-d"},
        ]);
        let mut evidence = CompletionEvidence::default();
        evidence.record(
            "future_query_tool",
            &json!({"ok": true, "result": rows.to_string()}),
            EvidenceProvenance::CapabilityResult,
        );
        evidence.record(
            "future_transition_tool",
            &json!({
                "ok": true,
                "effect_verified": true,
                "verification": {
                    "status": "committed",
                    "actual_url": "https://example.invalid/opaque-final"
                }
            }),
            EvidenceProvenance::CapabilityResult,
        );

        let context = evidence.context();
        assert!(context.contains("opaque-row-a"));
        assert!(context.contains("opaque-row-d"));
        assert!(context.contains("233"));
        assert!(context.contains("effect_verified"));
        assert!(context.contains("https://example.invalid/opaque-final"));
    }

    #[test]
    fn future_field_names_survive_while_binary_payloads_and_totals_are_bounded() {
        let binary = format!(
            "data:application/octet-stream;base64,{}",
            "A".repeat(10_000)
        );
        let short_binary = format!("DATA:image/png;BASE64,{}", "B".repeat(96));
        let mut evidence = CompletionEvidence::default();
        evidence.record(
            "future_read_tool",
            &json!({
                "ok": true,
                "data": {"count": 3, "name": "structured-marker"},
                "content": "meaningful-marker",
                "opaque_payload": binary,
                "compact_payload": short_binary,
            }),
            EvidenceProvenance::CapabilityResult,
        );
        let first = evidence.context();
        assert!(first.contains("structured-marker"));
        assert!(first.contains("meaningful-marker"));
        assert!(first.contains("binary_like_string"));
        assert!(!first.contains(&"A".repeat(1025)));
        assert!(!first.contains(&"B".repeat(64)));

        let prose = "long grounded prose with ordinary word boundaries ".repeat(200);
        assert!(!looks_binary_like(&prose));

        for index in 0..20 {
            evidence.record(
                &format!("future_tool_{index}"),
                &json!({"ok": true, "status": index}),
                EvidenceProvenance::CapabilityResult,
            );
        }

        let context = evidence.context();
        assert!(evidence.entries.len() <= MAX_ENTRIES);
        assert!(context.len() <= MAX_TOTAL_BYTES);
        assert_eq!(
            evidence.total_bytes,
            evidence.entries.iter().map(LedgerEntry::len).sum::<usize>()
        );
    }

    #[test]
    fn long_turn_retains_early_decision_evidence_and_recent_committed_state() {
        let mut evidence = CompletionEvidence::default();
        for index in 0..32 {
            evidence.record(
                "future_capability",
                &json!({
                    "ok": true,
                    "opaque_sequence": format!("marker-{index:03}"),
                }),
                EvidenceProvenance::CapabilityResult,
            );
        }

        let context = evidence.context();
        for index in 0..MAX_EARLY_ENTRIES {
            assert!(context.contains(&format!("marker-{index:03}")));
        }
        for index in (32 - MAX_RECENT_ENTRIES)..32 {
            assert!(context.contains(&format!("marker-{index:03}")));
        }
        assert!(!context.contains("marker-004"));
        assert!(context.contains("omitted_middle_entries"));
        assert!(context.len() <= MAX_TOTAL_BYTES);
    }

    #[test]
    fn large_objects_and_arrays_sample_the_whole_structure() {
        let object: Map<String, Value> = (0..80)
            .map(|index| {
                (
                    format!("future_field_{index:03}"),
                    Value::String(format!("value-{index:03}")),
                )
            })
            .collect();
        let array: Vec<Value> = (0..80)
            .map(|index| Value::String(format!("item-{index:03}")))
            .collect();
        let mut evidence = CompletionEvidence::default();
        evidence.record(
            "future_shape",
            &json!({
                "object": object,
                "array": array,
            }),
            EvidenceProvenance::CapabilityResult,
        );

        let context = evidence.context();
        assert!(context.contains("future_field_000"));
        assert!(context.contains("value-079"));
        assert!(context.contains("item-000"));
        assert!(context.contains("item-079"));
        assert!(context.contains("omitted_fields"));
        assert!(context.contains("omitted_items"));
        assert!(context.len() <= MAX_ENTRY_BYTES);
    }

    #[test]
    fn one_deep_field_cannot_starve_later_siblings() {
        let large_branch: Vec<Value> = (0..200)
            .map(|index| json!({"index": index, "payload": vec![index; 12]}))
            .collect();
        let mut evidence = CompletionEvidence::default();
        evidence.record(
            "future_shape",
            &json!({
                "a_large_branch": large_branch,
                "z_later_future_field": "later-field-marker",
            }),
            EvidenceProvenance::CapabilityResult,
        );

        let context = evidence.context();
        assert!(context.contains("z_later_future_field"));
        assert!(context.contains("later-field-marker"));
        assert!(context.len() <= MAX_ENTRY_BYTES);
    }

    #[test]
    fn grounded_browser_receipt_keeps_title_url_and_exact_document() {
        let mut evidence = CompletionEvidence::default();
        evidence.record_grounded_surface(
            "Current page",
            "https://example.invalid/final",
            &SurfaceIdentity::Browser {
                tab_id: 47,
                document_id: "document-final".to_string(),
                window: BROWSER_WINDOW,
            },
        );

        let context = evidence.context();
        assert!(context.contains("Current page"));
        assert!(context.contains("https://example.invalid/final"));
        assert!(context.contains("47"));
        assert!(context.contains("document-final"));
        assert!(context.len() <= MAX_TOTAL_BYTES);
    }

    #[test]
    fn immutable_job_source_keeps_exact_surface_identity() {
        let mut evidence = CompletionEvidence::default();
        evidence.record_job_source(&super::super::FrameSource {
            frame_id: 91,
            surface: SurfaceIdentity::Browser {
                tab_id: 42,
                document_id: "source-document".into(),
                window: BROWSER_WINDOW,
            },
        });
        let context = evidence.context();
        assert!(context.contains(r#""provenance":"job_source""#));
        assert!(context.contains("91"));
        assert!(context.contains("42"));
        assert!(context.contains("source-document"));
    }

    #[test]
    fn first_source_per_provider_survives_a_long_job() {
        let mut evidence = CompletionEvidence::default();
        for source in [
            super::super::FrameSource::native(1, (2, 3, 4)),
            super::super::FrameSource {
                frame_id: 9,
                surface: SurfaceIdentity::Browser {
                    tab_id: 10,
                    document_id: "first-browser-document".into(),
                    window: BROWSER_WINDOW,
                },
            },
        ] {
            evidence.record_provider_source(&source);
        }
        for n in 0..40 {
            evidence.record(
                "future",
                &json!({"n": n}),
                EvidenceProvenance::CapabilityResult,
            );
        }
        let context = evidence.context();
        assert!(context.contains(r#""hwnd":2"#));
        assert!(context.contains("first-browser-document"));
    }
}
