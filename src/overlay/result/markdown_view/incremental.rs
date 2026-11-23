//! Incremental Markdown rendering for streamed result text.
//!
//! A streamed answer only grows at its end, so blocks before the last stable
//! block boundary render once and are reused. A boundary is the start of a
//! line that follows a blank line, outside fenced code and multi-line HTML,
//! and that cannot continue the previous block (no indentation, list marker,
//! quote, or HTML). Link reference definitions connect distant blocks, so a
//! document that has one is always rendered whole.

use super::conversion::{
    CompositorRender, render_for_compositor, render_markdown_segment,
    should_enable_interactive_word_wrap,
};
use super::html_utils::normalize_html_content;

#[derive(Default)]
pub struct IncrementalMarkdown {
    /// Source through the last classified line; later text must extend it.
    scanned: String,
    /// `scanned[..committed_len]` ends at a stable boundary.
    committed_len: usize,
    committed_html: String,
    word_wrap: bool,
    scan: LineScan,
    #[cfg(test)]
    parsed_bytes: usize,
}

#[derive(Default)]
struct LineScan {
    previous_blank: bool,
    /// The fence character and length that close the open code block.
    fence: Option<(u8, usize)>,
    /// The marker that closes an open HTML block that may span blank lines.
    html_end: Option<&'static str>,
    has_reference_definition: bool,
    /// The newest stable boundary found since the last commit.
    boundary: Option<usize>,
}

impl IncrementalMarkdown {
    pub fn render(&mut self, markdown: &str) -> CompositorRender {
        if normalize_html_content(markdown).is_some() {
            *self = Self::default();
            return render_for_compositor(markdown);
        }
        CompositorRender {
            body: self.render_markdown(markdown),
            isolated_document: None,
        }
    }

    fn render_markdown(&mut self, markdown: &str) -> String {
        let word_wrap = should_enable_interactive_word_wrap(markdown);
        if word_wrap != self.word_wrap || !markdown.starts_with(self.scanned.as_str()) {
            *self = Self {
                word_wrap,
                #[cfg(test)]
                parsed_bytes: self.parsed_bytes,
                ..Self::default()
            };
        }
        let from = self.scanned.len();
        let classified = self.scan.advance(markdown, from);
        self.scanned.push_str(&markdown[from..classified]);
        let open_line = &markdown[classified..];
        if self.scan.has_reference_definition
            || (self.scan.fence.is_none() && open_line.contains("]:"))
        {
            return self.parse(markdown);
        }
        if let Some(boundary) = self.scan.boundary.take() {
            let html = self.parse(&markdown[self.committed_len..boundary]);
            self.committed_html.push_str(&html);
            self.committed_len = boundary;
        }
        let tail = self.parse(&markdown[self.committed_len..]);
        let mut body = String::with_capacity(self.committed_html.len() + tail.len());
        body.push_str(&self.committed_html);
        body.push_str(&tail);
        body
    }

    fn parse(&mut self, markdown: &str) -> String {
        #[cfg(test)]
        {
            self.parsed_bytes += markdown.len();
        }
        render_markdown_segment(markdown, self.word_wrap)
    }
}

const HTML_BLOCKS_SPANNING_BLANK_LINES: [(&str, &str); 8] = [
    ("<script", "</script>"),
    ("<pre", "</pre>"),
    ("<style", "</style>"),
    ("<textarea", "</textarea>"),
    ("<!--", "-->"),
    ("<?", "?>"),
    ("<![cdata[", "]]>"),
    ("<!", ">"),
];

impl LineScan {
    /// Classify complete lines from `offset`; returns the end of the last one.
    fn advance(&mut self, markdown: &str, mut offset: usize) -> usize {
        while let Some(length) = markdown[offset..].find('\n') {
            self.classify(offset, &markdown[offset..offset + length]);
            offset += length + 1;
        }
        offset
    }

    fn classify(&mut self, start: usize, line: &str) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let indent = line.len() - line.trim_start_matches(' ').len();
        let content = &line[indent..];
        if let Some((marker, length)) = self.fence {
            let run = content.bytes().take_while(|byte| *byte == marker).count();
            if indent <= 3 && run >= length && content[run..].trim().is_empty() {
                self.fence = None;
            }
            self.previous_blank = false;
            return;
        }
        if let Some(end) = self.html_end {
            if line.to_ascii_lowercase().contains(end) {
                self.html_end = None;
            }
            self.previous_blank = false;
            return;
        }
        if line.trim().is_empty() {
            self.previous_blank = true;
            return;
        }
        if self.previous_blank && starts_independent_block(line) {
            self.boundary = Some(start);
        }
        self.previous_blank = false;
        // Definitions may also sit in quotes and list items; over-matching
        // only costs a whole render.
        if line.contains("]:") {
            self.has_reference_definition = true;
        }
        if indent > 3 {
            return;
        }
        if let Some(marker) = content
            .bytes()
            .next()
            .filter(|byte| matches!(byte, b'`' | b'~'))
        {
            let run = content.bytes().take_while(|byte| *byte == marker).count();
            if run >= 3 && (marker == b'~' || !content[run..].contains('`')) {
                self.fence = Some((marker, run));
            }
            return;
        }
        let lower = content.to_ascii_lowercase();
        if let Some((_, end)) = HTML_BLOCKS_SPANNING_BLANK_LINES
            .iter()
            .find(|(open, _)| lower.starts_with(open))
            && !lower[1..].contains(end)
        {
            self.html_end = Some(end);
        }
    }
}

/// Whether a line after a blank line always starts a new top-level block.
fn starts_independent_block(line: &str) -> bool {
    line.bytes().next().is_some_and(|first| {
        !first.is_ascii_whitespace()
            && !first.is_ascii_digit()
            && !matches!(first, b'-' | b'+' | b'*' | b'>' | b'<' | b'|' | b'[')
    })
}

#[cfg(test)]
#[path = "incremental_tests.rs"]
mod tests;
