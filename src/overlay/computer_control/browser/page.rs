use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const READ_PAGE_LIMIT: usize = 12_000;
const SEMANTIC_ANNOTATION_LIMIT: usize = 4_000;
const STRUCTURAL_ANNOTATION_LIMIT: usize = 8_000;
const MAX_IFRAME_VISITS: u64 = 64;
const PAGE_CAPTURE_SCRIPT: &str = include_str!("page_capture.js");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IframeCaptureStats {
    inspected: u64,
    same_origin: u64,
    invisible: u64,
    inaccessible: u64,
    truncated: bool,
}

impl IframeCaptureStats {
    fn from_value(value: &Value) -> Self {
        let raw_inspected = value
            .get("inspectedIframes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let inspected = raw_inspected.min(MAX_IFRAME_VISITS);
        let raw_invisible = value
            .get("invisibleIframes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let invisible = raw_invisible.min(inspected);
        let raw_inaccessible = value
            .get("inaccessibleIframes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let inaccessible = raw_inaccessible.min(inspected.saturating_sub(invisible));
        let raw_same_origin = value
            .get("sameOriginIframes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let same_origin = raw_same_origin.min(inspected.saturating_sub(invisible + inaccessible));
        let accounted = same_origin + invisible + inaccessible;
        let truncated = bool_field(value, "iframeCaptureTruncated")
            || raw_inspected > MAX_IFRAME_VISITS
            || raw_invisible != invisible
            || raw_inaccessible != inaccessible
            || raw_same_origin != same_origin
            || accounted != inspected;
        Self {
            inspected,
            same_origin,
            invisible,
            inaccessible,
            truncated,
        }
    }

    fn skipped(self) -> u64 {
        self.invisible + self.inaccessible
    }

    fn visible_content_incomplete(self) -> bool {
        self.inaccessible > 0 || self.truncated
    }
}

pub(in crate::overlay::computer_control) struct PageCapture {
    title: String,
    title_char_count: u64,
    title_truncated: bool,
    url: String,
    text: String,
    semantic_annotations: String,
    structural_annotations: String,
    iframe_capture: IframeCaptureStats,
    capture_truncated: bool,
    inspected_text_nodes: u64,
    eligible_text_nodes: u64,
    text_inspection_truncated: bool,
    text_evidence_truncated: bool,
    semantic_truncated: bool,
    inspected_semantic_nodes: u64,
    eligible_semantic_nodes: u64,
    semantic_inspection_truncated: bool,
    semantic_evidence_truncated: bool,
    structural_truncated: bool,
    inspected_structural_nodes: u64,
    eligible_structural_nodes: u64,
    structural_inspection_truncated: bool,
    structural_evidence_truncated: bool,
}

impl PageCapture {
    pub(in crate::overlay::computer_control) fn title(&self) -> &str {
        &self.title
    }

    pub(in crate::overlay::computer_control) fn url(&self) -> &str {
        &self.url
    }

    pub(in crate::overlay::computer_control) fn text_char_count(&self) -> usize {
        self.text.chars().count()
    }

    pub(in crate::overlay::computer_control) fn fingerprint(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for value in [
            self.title.as_bytes(),
            self.url.as_bytes(),
            self.text.as_bytes(),
            self.semantic_annotations.as_bytes(),
            self.structural_annotations.as_bytes(),
        ] {
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(value);
        }
        for value in [
            self.title_char_count,
            self.iframe_capture.inspected,
            self.iframe_capture.same_origin,
            self.iframe_capture.invisible,
            self.iframe_capture.inaccessible,
            self.inspected_text_nodes,
            self.eligible_text_nodes,
            self.inspected_semantic_nodes,
            self.eligible_semantic_nodes,
            self.inspected_structural_nodes,
            self.eligible_structural_nodes,
        ] {
            hasher.update(value.to_le_bytes());
        }
        hasher.update([
            self.title_truncated as u8,
            self.iframe_capture.truncated as u8,
            self.capture_truncated as u8,
            self.text_inspection_truncated as u8,
            self.text_evidence_truncated as u8,
            self.semantic_truncated as u8,
            self.semantic_inspection_truncated as u8,
            self.semantic_evidence_truncated as u8,
            self.structural_truncated as u8,
            self.structural_inspection_truncated as u8,
            self.structural_evidence_truncated as u8,
        ]);
        hasher.finalize().into()
    }
}

pub(in crate::overlay::computer_control) fn read_page() -> Value {
    read_page_impl(None, Some(READ_PAGE_LIMIT))
}

pub(in crate::overlay::computer_control) fn read_page_on_tab(tab_id: i64) -> Value {
    read_page_impl(Some(tab_id), Some(READ_PAGE_LIMIT))
}

pub(in crate::overlay::computer_control) fn capture_page_on_tab(
    tab_id: i64,
) -> Result<PageCapture, Value> {
    extract_current_page(Some(tab_id))
}

pub(in crate::overlay::computer_control) fn publish_bounded_page_on_tab(
    page: PageCapture,
    tab_id: i64,
) -> Value {
    page_result(page, Some(tab_id), None)
}

fn read_page_impl(tab_id: Option<i64>, preview_limit: Option<usize>) -> Value {
    let page = match extract_current_page(tab_id) {
        Ok(page) => page,
        Err(value) => return tag_target(value, tab_id),
    };
    page_result(page, tab_id, preview_limit)
}

fn page_result(page: PageCapture, tab_id: Option<i64>, preview_limit: Option<usize>) -> Value {
    let text = match preview_limit {
        Some(limit) => page.text.chars().take(limit).collect(),
        None => page.text.clone(),
    };
    let semantic_annotations = page
        .semantic_annotations
        .chars()
        .take(SEMANTIC_ANNOTATION_LIMIT)
        .collect();
    let structural_annotations = page
        .structural_annotations
        .chars()
        .take(STRUCTURAL_ANNOTATION_LIMIT)
        .collect();
    let truncated = preview_limit.is_some_and(|limit| page.text.chars().count() > limit);
    let mut partial_proof = vec!["/artifact/preview"];
    if truncated || page.capture_truncated {
        partial_proof.push("/page/text");
    }
    if page.semantic_truncated {
        partial_proof.push("/page/semantic_annotations");
    }
    if page.structural_truncated {
        partial_proof.push("/page/structural_annotations");
    }
    let instruction = if page.capture_truncated {
        "The page capture hit a safety bound. Returned text and its artifact are partial; narrow the page or use another bounded read before claiming complete coverage. If only a subset is requested, derive it with extract_artifact before paste/save."
    } else if truncated {
        "The returned page.text is a preview and artifact.id is the whole bounded capture. For a whole exact copy/export use paste_artifact or save_artifact; for a subset call extract_artifact first with exact boundaries."
    } else {
        "artifact.id is the whole page capture. Use it directly only when the whole capture was requested; for a subset call extract_artifact first with exact boundaries, then paste/save the derived id."
    };
    let artifact = save_page_artifact(&page);
    let mut metadata = page_metadata(&page);
    if let Some(object) = metadata.as_object_mut() {
        object.insert("text".into(), Value::String(text));
        object.insert(
            "semantic_annotations".into(),
            Value::String(semantic_annotations),
        );
        object.insert(
            "structural_annotations".into(),
            Value::String(structural_annotations),
        );
        object.insert("truncated".into(), Value::Bool(truncated));
    }
    tag_target(
        json!({
            "ok": true,
            "page": metadata,
            "artifact": artifact,
            "instruction": instruction,
            "completion_proof": {
                "partial": partial_proof,
                "exact": [
                    "/page/url", "/artifact/id", "/artifact/source_url",
                    "/artifact/path", "/artifact/sha256"
                ],
            },
        }),
        tab_id,
    )
}

pub(in crate::overlay::computer_control) fn extract_page() -> Value {
    extract_page_impl(None)
}

pub(in crate::overlay::computer_control) fn extract_page_on_tab(tab_id: i64) -> Value {
    extract_page_impl(Some(tab_id))
}

fn extract_page_impl(tab_id: Option<i64>) -> Value {
    let page = match extract_current_page(tab_id) {
        Ok(page) => page,
        Err(value) => return tag_target(value, tab_id),
    };
    tag_target(
        json!({
            "ok": true,
            "page": page_metadata(&page),
            "artifact": save_page_artifact(&page),
            "completion_proof": {
                "partial": ["/artifact/preview"],
                "exact": [
                    "/page/url", "/artifact/id", "/artifact/source_url",
                    "/artifact/path", "/artifact/sha256"
                ],
            },
        }),
        tab_id,
    )
}

fn page_metadata(page: &PageCapture) -> Value {
    json!({
        "title": page.title,
        "title_char_count": page.title_char_count,
        "title_truncated": page.title_truncated,
        "url": page.url,
        "char_count": page.text.chars().count(),
        "word_count": page.text.split_whitespace().count(),
        "same_origin_iframes": page.iframe_capture.same_origin,
        "skipped_iframes": page.iframe_capture.skipped(),
        "inspected_iframes": page.iframe_capture.inspected,
        "invisible_iframes": page.iframe_capture.invisible,
        "inaccessible_iframes": page.iframe_capture.inaccessible,
        "iframe_capture_truncated": page.iframe_capture.truncated,
        "visible_iframe_content_incomplete": page.iframe_capture.visible_content_incomplete(),
        "iframe_visit_limit": MAX_IFRAME_VISITS,
        "semantic_annotation_chars": page.semantic_annotations.chars().count(),
        "structural_annotation_chars": page.structural_annotations.chars().count(),
        "capture_truncated": page.capture_truncated,
        "inspected_text_nodes": page.inspected_text_nodes,
        "eligible_text_nodes": page.eligible_text_nodes,
        "visited_text_nodes": page.eligible_text_nodes,
        "text_inspection_truncated": page.text_inspection_truncated,
        "text_evidence_truncated": page.text_evidence_truncated,
        "semantic_truncated": page.semantic_truncated,
        "inspected_semantic_nodes": page.inspected_semantic_nodes,
        "eligible_semantic_nodes": page.eligible_semantic_nodes,
        "visited_semantic_nodes": page.eligible_semantic_nodes,
        "semantic_inspection_truncated": page.semantic_inspection_truncated,
        "semantic_evidence_truncated": page.semantic_evidence_truncated,
        "structural_truncated": page.structural_truncated,
        "inspected_structural_nodes": page.inspected_structural_nodes,
        "eligible_structural_nodes": page.eligible_structural_nodes,
        "structural_inspection_truncated": page.structural_inspection_truncated,
        "structural_evidence_truncated": page.structural_evidence_truncated,
    })
}

fn tag_target(mut result: Value, tab_id: Option<i64>) -> Value {
    if let (Some(tab_id), Some(object)) = (tab_id, result.as_object_mut()) {
        object.insert("target_tab_id".to_string(), json!(tab_id));
    }
    result
}

fn extract_current_page(tab_id: Option<i64>) -> Result<PageCapture, Value> {
    if let Some(value) = super::conn_guard() {
        return Err(value);
    }
    let result = match tab_id {
        Some(id) => super::eval_value_on_tab(PAGE_CAPTURE_SCRIPT, id),
        None => super::eval_value(PAGE_CAPTURE_SCRIPT),
    };
    let value = result.map_err(super::err)?;
    if bool_field(&value, "urlTooLong") {
        return Err(json!({
            "ok": false,
            "code": "ERR_BROWSER_PAGE_URL_TOO_LONG",
            "error": "page URL exceeded the bounded capture contract",
        }));
    }
    let iframe_capture = IframeCaptureStats::from_value(&value);
    let text_inspection_truncated = bool_field(&value, "textInspectionTruncated");
    let text_evidence_truncated = bool_field(&value, "textEvidenceTruncated");
    let semantic_inspection_truncated = bool_field(&value, "semanticInspectionTruncated");
    let semantic_evidence_truncated = bool_field(&value, "semanticEvidenceTruncated");
    let structural_inspection_truncated = bool_field(&value, "structuralInspectionTruncated");
    let structural_evidence_truncated = bool_field(&value, "structuralEvidenceTruncated");
    Ok(PageCapture {
        title: string_field(&value, "title"),
        title_char_count: u64_field(&value, "titleCharCount"),
        title_truncated: bool_field(&value, "titleTruncated"),
        url: string_field(&value, "url"),
        text: string_field(&value, "text"),
        semantic_annotations: string_field(&value, "semanticAnnotations"),
        structural_annotations: string_field(&value, "structuralAnnotations"),
        iframe_capture,
        capture_truncated: bool_field(&value, "captureTruncated")
            || iframe_capture.truncated
            || text_inspection_truncated
            || text_evidence_truncated,
        inspected_text_nodes: u64_field(&value, "inspectedTextNodes"),
        eligible_text_nodes: u64_field(&value, "eligibleTextNodes"),
        text_inspection_truncated,
        text_evidence_truncated,
        semantic_truncated: bool_field(&value, "semanticTruncated")
            || semantic_inspection_truncated
            || semantic_evidence_truncated,
        inspected_semantic_nodes: u64_field(&value, "inspectedSemanticNodes"),
        eligible_semantic_nodes: u64_field(&value, "eligibleSemanticNodes"),
        semantic_inspection_truncated,
        semantic_evidence_truncated,
        structural_truncated: bool_field(&value, "structuralTruncated")
            || structural_inspection_truncated
            || structural_evidence_truncated,
        inspected_structural_nodes: u64_field(&value, "inspectedStructuralNodes"),
        eligible_structural_nodes: u64_field(&value, "eligibleStructuralNodes"),
        structural_inspection_truncated,
        structural_evidence_truncated,
    })
}

fn string_field(value: &Value, name: &str) -> String {
    value
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn u64_field(value: &Value, name: &str) -> u64 {
    value.get(name).and_then(Value::as_u64).unwrap_or(0)
}

fn bool_field(value: &Value, name: &str) -> bool {
    value.get(name).and_then(Value::as_bool).unwrap_or(false)
}

fn save_page_artifact(page: &PageCapture) -> Value {
    match super::super::artifacts::create_text(
        "browser_page_text",
        Some(&page.title),
        Some(&page.url),
        &page.text,
    ) {
        Ok(artifact) => {
            let mut response = artifact.response(&page.text);
            if let Some(object) = response.as_object_mut() {
                let fingerprint = page.fingerprint();
                let capture_sha256 = fingerprint
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                object.insert("capture_sha256".into(), Value::String(capture_sha256));
            }
            response
        }
        Err(error) => json!({"ok": false, "error": format!("artifact save failed: {error}")}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(text: &str, inspected_text_nodes: u64) -> PageCapture {
        PageCapture {
            title: "Title".into(),
            title_char_count: 5,
            title_truncated: false,
            url: "https://example.test/source".into(),
            text: text.into(),
            semantic_annotations: "[link] source -> https://example.test".into(),
            structural_annotations: "[section] Plan :: value".into(),
            iframe_capture: IframeCaptureStats {
                inspected: 0,
                same_origin: 0,
                invisible: 0,
                inaccessible: 0,
                truncated: false,
            },
            capture_truncated: false,
            inspected_text_nodes,
            eligible_text_nodes: 1,
            text_inspection_truncated: false,
            text_evidence_truncated: false,
            semantic_truncated: false,
            inspected_semantic_nodes: 1,
            eligible_semantic_nodes: 1,
            semantic_inspection_truncated: false,
            semantic_evidence_truncated: false,
            structural_truncated: false,
            inspected_structural_nodes: 1,
            eligible_structural_nodes: 1,
            structural_inspection_truncated: false,
            structural_evidence_truncated: false,
        }
    }

    #[test]
    fn iframe_stats_preserve_a_valid_global_partition() {
        let stats = IframeCaptureStats::from_value(&json!({
            "inspectedIframes": 7,
            "sameOriginIframes": 3,
            "invisibleIframes": 2,
            "inaccessibleIframes": 2,
            "iframeCaptureTruncated": false,
        }));
        assert_eq!(
            stats,
            IframeCaptureStats {
                inspected: 7,
                same_origin: 3,
                invisible: 2,
                inaccessible: 2,
                truncated: false,
            }
        );
        assert_eq!(stats.skipped(), 4);
        assert!(stats.visible_content_incomplete());
    }

    #[test]
    fn invisible_frames_alone_do_not_claim_visible_content_is_missing() {
        let stats = IframeCaptureStats::from_value(&json!({
            "inspectedIframes": 2,
            "sameOriginIframes": 0,
            "invisibleIframes": 2,
            "inaccessibleIframes": 0,
            "iframeCaptureTruncated": false,
        }));
        assert_eq!(stats.skipped(), 2);
        assert!(!stats.visible_content_incomplete());
    }

    #[test]
    fn malformed_frame_counts_are_bounded_and_marked_incomplete() {
        let stats = IframeCaptureStats::from_value(&json!({
            "inspectedIframes": 900,
            "sameOriginIframes": 900,
            "invisibleIframes": 40,
            "inaccessibleIframes": 40,
            "iframeCaptureTruncated": false,
        }));
        assert_eq!(stats.inspected, MAX_IFRAME_VISITS);
        assert_eq!(stats.invisible, 40);
        assert_eq!(stats.inaccessible, 24);
        assert_eq!(stats.same_origin, 0);
        assert_eq!(stats.skipped(), MAX_IFRAME_VISITS);
        assert!(stats.truncated);
        assert!(stats.visible_content_incomplete());
    }

    #[test]
    fn capture_script_shares_frames_and_separates_inspection_from_evidence() {
        assert!(PAGE_CAPTURE_SCRIPT.contains("capturedDocuments.push(doc)"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("semanticAnnotations(capturedDocuments)"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("structuralAnnotations(capturedDocuments)"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("[row] ${cells.join(\" | \")}"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("[section] ${heading} ::"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("MAX_INSPECTED_TEXT_NODES"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("MAX_VISIBLE_TEXT_NODES"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("textInspectionTruncated"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("textEvidenceTruncated"));
        assert!(PAGE_CAPTURE_SCRIPT.contains("if (!visible(frame))"));
        assert!(!PAGE_CAPTURE_SCRIPT.contains("Math.min(frames.length, 64)"));
    }

    #[test]
    fn readiness_fingerprint_covers_content_and_structural_metadata() {
        let baseline = capture("evidence", 1).fingerprint();
        assert_ne!(baseline, capture("changed evidence", 1).fingerprint());
        assert_ne!(baseline, capture("evidence", 2).fingerprint());
        assert_eq!(baseline, capture("evidence", 1).fingerprint());
    }
}
