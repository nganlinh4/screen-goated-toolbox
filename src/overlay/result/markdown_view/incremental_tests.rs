use super::*;

fn full_body(markdown: &str) -> String {
    render_for_compositor(markdown).body
}

/// Stream `document` in fixed-size chunks and require the incremental body to
/// equal a full render after every chunk.
fn stream(document: &str, chunk: usize) -> IncrementalMarkdown {
    let mut renderer = IncrementalMarkdown::default();
    let mut end = 0;
    while end < document.len() {
        end = (end + chunk).min(document.len());
        while !document.is_char_boundary(end) {
            end += 1;
        }
        let text = &document[..end];
        assert_eq!(renderer.render(text).body, full_body(text), "prefix {end}");
    }
    renderer
}

const MIXED: &str = "# Title\n\nIntro with **bold** and `code`.\n\n- one\n- two\n\n  continued item\n\nAfter list.\n\n1. first\n\n2. second\n\nplain\n\n> quote\n\n> second quote\n\n```rust\nfn main() {\n\n    println!(\"x\");\n}\n```\n\nSetext\n===\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n    indented code\n\n    more code\n\nText\n\n<!-- comment\n\nstill comment -->\n\n<pre>\n\nkeep\n\n</pre>\n\n~~~\n\n~~~\n\nTail paragraph\nwith soft break.\n";

#[test]
fn every_streamed_prefix_matches_a_full_render() {
    for chunk in [1, 3, 17] {
        stream(MIXED, chunk);
    }
    let words = "word ".repeat(2_000);
    stream(&format!("{words}\n\nnext {words}\n\nend"), 97);
}

#[test]
fn reference_definitions_and_rewritten_text_render_whole() {
    stream(
        "See [docs][d].\n\nMore text.\n\n[d]: https://example.com\n\nAfter.\n",
        2,
    );
    stream("Use [q].\n\n> [q]: https://example.com\n\nend\n", 3);
    let mut renderer = IncrementalMarkdown::default();
    for text in [
        "alpha\n\nbeta\n\ngamma",
        "alpha\n\nBETA\n\ngamma",
        "replaced",
    ] {
        assert_eq!(renderer.render(text).body, full_body(text));
    }
}

#[test]
fn streaming_parses_each_block_a_bounded_number_of_times() {
    let block = "A paragraph with *some* emphasis and enough words to matter.\n\n";
    let document = block.repeat(400);
    let chunk = 8;
    let renderer = stream(&document, chunk);
    let updates = document.len().div_ceil(chunk);
    // Each update reparses at most the open block plus one newly closed block,
    // where a full reparse per update would cost about updates * len / 2.
    let bound = 2 * document.len() + updates * 2 * block.len();
    assert!(
        renderer.parsed_bytes <= bound,
        "parsed {} bytes, bound {bound}",
        renderer.parsed_bytes
    );
    assert!(renderer.parsed_bytes * 20 < updates * document.len() / 2);
}

#[test]
fn raw_html_documents_keep_their_isolated_render() {
    let mut renderer = IncrementalMarkdown::default();
    let document = "<html><head></head><body><p>hello</p></body></html>";
    let rendered = renderer.render(document);
    assert!(rendered.isolated_document.is_some());
    assert_eq!(rendered.body, full_body(document));
    assert_eq!(renderer.render("after").body, full_body("after"));
}

#[test]
fn streaming_markdown_never_rescans_the_whole_answer_for_html() {
    use super::super::html_document::DOCUMENT_SCANS;
    let document = "Prose that mentions <html>, <style> and <script> tags.

"
    .repeat(200);
    let mut renderer = IncrementalMarkdown::default();
    DOCUMENT_SCANS.with(|scans| scans.set(0));
    for end in (64..document.len()).step_by(64) {
        assert!(
            renderer
                .render(&document[..end])
                .isolated_document
                .is_none()
        );
    }
    assert_eq!(DOCUMENT_SCANS.with(std::cell::Cell::get), 0);
    let fenced = "```html
<!doctype html><html><body>x</body></html>
```";
    assert!(renderer.render(fenced).isolated_document.is_some());
    assert!(DOCUMENT_SCANS.with(std::cell::Cell::get) > 0);
}
