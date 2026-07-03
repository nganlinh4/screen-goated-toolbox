use serde_json::{Value, json};

const READ_PAGE_LIMIT: usize = 12_000;

struct PageText {
    title: String,
    url: String,
    text: String,
    same_origin_iframes: u64,
    skipped_iframes: u64,
}

pub(in crate::overlay::computer_control) fn read_page() -> Value {
    let page = match extract_current_page() {
        Ok(page) => page,
        Err(v) => return v,
    };
    let text: String = page.text.chars().take(READ_PAGE_LIMIT).collect();
    let truncated = page.text.chars().count() > READ_PAGE_LIMIT;
    let artifact = save_page_artifact(&page);
    json!({
        "ok": true,
        "page": {
            "title": page.title,
            "url": page.url,
            "text": text,
            "truncated": truncated,
            "char_count": page.text.chars().count(),
            "word_count": page.text.split_whitespace().count(),
            "same_origin_iframes": page.same_origin_iframes,
            "skipped_iframes": page.skipped_iframes,
        },
        "artifact": artifact,
        "instruction": if truncated {
            "The returned page.text is a preview. For exact copy/export, use artifact.id with paste_artifact or save_artifact."
        } else {
            "For exact copy/export, still prefer artifact.id with paste_artifact or save_artifact instead of retyping page.text."
        },
    })
}

pub(in crate::overlay::computer_control) fn extract_page() -> Value {
    let page = match extract_current_page() {
        Ok(page) => page,
        Err(v) => return v,
    };
    json!({
        "ok": true,
        "page": {
            "title": page.title,
            "url": page.url,
            "char_count": page.text.chars().count(),
            "word_count": page.text.split_whitespace().count(),
            "same_origin_iframes": page.same_origin_iframes,
            "skipped_iframes": page.skipped_iframes,
        },
        "artifact": save_page_artifact(&page),
    })
}

fn extract_current_page() -> Result<PageText, Value> {
    if let Some(v) = super::conn_guard() {
        return Err(v);
    }
    let js = r#"(() => {
        const seen = new WeakSet();
        let sameOriginIframes = 0;
        let skippedIframes = 0;
        const frameText = (doc, depth) => {
            if (!doc || seen.has(doc)) return "";
            seen.add(doc);
            let text = doc.body ? doc.body.innerText : "";
            if (depth < 4) {
                for (const f of doc.querySelectorAll("iframe")) {
                    try {
                        if (f.contentDocument) {
                            sameOriginIframes++;
                            text += "\n\n[iframe] " + frameText(f.contentDocument, depth + 1);
                        } else {
                            skippedIframes++;
                        }
                    } catch (e) {
                        skippedIframes++;
                    }
                }
            }
            return text;
        };
        return {
            title: document.title || "",
            url: location.href,
            text: frameText(document, 0),
            sameOriginIframes,
            skippedIframes
        };
    })()"#;
    match super::eval_value(js) {
        Ok(v) => Ok(PageText {
            title: v
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            url: v
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: v
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            same_origin_iframes: v
                .get("sameOriginIframes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            skipped_iframes: v.get("skippedIframes").and_then(Value::as_u64).unwrap_or(0),
        }),
        Err(e) => Err(super::err(e)),
    }
}

fn save_page_artifact(page: &PageText) -> Value {
    match super::super::artifacts::create_text(
        "browser_page_text",
        Some(&page.title),
        Some(&page.url),
        &page.text,
    ) {
        Ok(artifact) => artifact.response(&page.text),
        Err(e) => json!({"ok": false, "error": format!("artifact save failed: {e}")}),
    }
}
