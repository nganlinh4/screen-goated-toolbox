//! Conservative classification of complete HTML documents.

#[cfg(test)]
thread_local! {
    /// Full-text classifier passes on this thread, for streaming cost tests.
    pub(super) static DOCUMENT_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Return the executable HTML payload, removing only transport decoration that
/// models commonly add around an otherwise complete document.
pub fn normalize_html_content(content: &str) -> Option<String> {
    let trimmed = content.trim().trim_start_matches('\u{feff}').trim_start();
    let candidate = strip_outer_html_fence(trimmed).unwrap_or(trimmed).trim();
    // Every complete document opens with markup. Rejecting other text here
    // keeps per-update streaming classification independent of answer length.
    if !candidate.starts_with('<') {
        return None;
    }
    #[cfg(test)]
    DOCUMENT_SCANS.with(|scans| scans.set(scans.get() + 1));
    let lower = candidate.to_ascii_lowercase();

    if starts_html_document(&lower) {
        return Some(candidate.to_string());
    }
    None
}

fn starts_html_document(lower: &str) -> bool {
    fn skip_comments_and_space(mut text: &str) -> Option<&str> {
        loop {
            text = text.trim_start_matches(|character: char| character.is_ascii_whitespace());
            let Some(comment) = text.strip_prefix("<!--") else {
                return Some(text);
            };
            let end = comment.find("-->")?;
            let body = &comment[..end];
            // HTML has alternate error-recovery endings for malformed comments.
            // Do not guess where the browser will resume parsing them.
            if body.starts_with('>')
                || body.starts_with("->")
                || body.contains("--!>")
                || body.contains("<!--")
            {
                return None;
            }
            text = &comment[end + 3..];
        }
    }

    fn ambiguous_script(mut text: &str) -> bool {
        while let Some(start) = text.find("<!--") {
            let escaped = &text[start + 4..];
            let end = escaped.find("-->").unwrap_or(escaped.len());
            let mut section = &escaped[..end];
            while let Some(nested) = section.find("<script") {
                let rest = &section[nested + "<script".len()..];
                if rest
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'>' || byte == b'/')
                {
                    return true;
                }
                section = rest;
            }
            if end == escaped.len() {
                break;
            }
            text = &escaped[end + 3..];
        }
        false
    }

    fn opening_end(rest: &str) -> Option<(usize, bool)> {
        #[derive(Clone, Copy)]
        enum State {
            BeforeName,
            Name,
            AfterName,
            BeforeValue,
            Unquoted,
            Quoted(u8),
            AfterQuoted,
            SelfClosing,
        }
        let mut state = State::BeforeName;
        for (index, byte) in rest.bytes().enumerate() {
            if index == 0 && byte == b'/' {
                continue;
            }
            state = match (state, byte) {
                (State::Quoted(delimiter), byte) if delimiter == byte => State::AfterQuoted,
                (State::Quoted(delimiter), _) => State::Quoted(delimiter),
                (_, b'>') => return Some((index + 1, matches!(state, State::SelfClosing))),
                (State::SelfClosing, _) => return None,
                (State::BeforeName | State::AfterName, byte) if byte.is_ascii_whitespace() => state,
                (State::AfterQuoted, byte) if byte.is_ascii_whitespace() => State::BeforeName,
                (State::Name, byte) if byte.is_ascii_whitespace() => State::AfterName,
                (State::BeforeValue, byte) if byte.is_ascii_whitespace() => State::BeforeValue,
                (State::Unquoted, byte) if byte.is_ascii_whitespace() => State::BeforeName,
                (State::Name | State::AfterName, b'=') => State::BeforeValue,
                (State::BeforeValue, b'\'' | b'"') => State::Quoted(byte),
                (State::BeforeName | State::Name | State::AfterName | State::AfterQuoted, b'/') => {
                    State::SelfClosing
                }
                (_, b'<' | b'\'' | b'"' | b'`' | b'=') => return None,
                (State::BeforeName | State::AfterName, _) => State::Name,
                (State::Name, _) => State::Name,
                (State::BeforeValue | State::Unquoted, _) => State::Unquoted,
                (State::AfterQuoted, _) => return None,
            };
        }
        None
    }

    fn doctype_tail(rest: &str) -> Option<&str> {
        fn trim(text: &str) -> &str {
            text.trim_start_matches(|character: char| character.is_ascii_whitespace())
        }
        fn quoted(text: &str) -> Option<&str> {
            let quote = *text.as_bytes().first()?;
            if !matches!(quote, b'\'' | b'"') {
                return None;
            }
            let body = &text[1..];
            let end = body.bytes().position(|byte| byte == quote)?;
            if body[..end].contains(['<', '>']) {
                return None;
            }
            Some(&body[end + 1..])
        }
        let rest = trim(rest);
        if let Some(tail) = rest.strip_prefix('>') {
            return Some(tail);
        }
        let (public, rest) = if let Some(tail) = rest.strip_prefix("public") {
            (true, tail)
        } else {
            (false, rest.strip_prefix("system")?)
        };
        if !rest.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
            return None;
        }
        let after_first = quoted(trim(rest))?;
        let remaining = trim(after_first);
        if let Some(tail) = remaining.strip_prefix('>') {
            return Some(tail);
        }
        if !public || after_first.len() == remaining.len() {
            return None;
        }
        trim(quoted(remaining)?).strip_prefix('>')
    }

    fn terminal_root_close(mut body: &str) -> bool {
        const RAW_TEXT: [&str; 9] = [
            "script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
            "noscript",
        ];
        const SENSITIVE: [&str; 4] = ["template", "select", "table", "frameset"];
        const FOREIGN: [&str; 2] = ["svg", "math"];
        const MAX_NESTED_CONTEXTS: usize = 64;
        let mut contexts: Vec<&str> = Vec::new();
        // An untracked child tag may switch the parser to HTML at an
        // integration point or breakout tag. Do not guess after that point.
        let mut foreign: Vec<(&str, bool)> = Vec::new();
        while let Some(index) = body.find('<') {
            body = &body[index..];
            if body.starts_with("<!--") {
                let Some(next) = skip_comments_and_space(body) else {
                    return false;
                };
                body = next;
                continue;
            }
            if let Some(cdata) = body.strip_prefix("<![cdata[") {
                if foreign.is_empty() {
                    return false;
                }
                let Some(end) = cdata.find("]]>") else {
                    return false;
                };
                // CDATA is meaningful only in foreign content. In HTML content
                // the browser can end the declaration earlier as a comment.
                if cdata[..end].contains('<') {
                    return false;
                }
                body = &cdata[end + 3..];
                continue;
            }
            if let Some(rest) = body.strip_prefix("</html")
                && rest
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'>')
            {
                if !contexts.is_empty() || !foreign.is_empty() {
                    return false;
                }
                let rest =
                    rest.trim_start_matches(|character: char| character.is_ascii_whitespace());
                return rest
                    .strip_prefix('>')
                    .is_some_and(|tail| skip_comments_and_space(tail).is_some_and(str::is_empty));
            }
            let foreign_tag = FOREIGN.iter().find_map(|name| {
                let (closing, tag) = if let Some(rest) = body.strip_prefix("</") {
                    (true, rest)
                } else {
                    (false, body.strip_prefix('<')?)
                };
                let tail = tag.strip_prefix(*name)?;
                tail.bytes()
                    .next()
                    .filter(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
                    .map(|_| (closing, *name))
            });
            if let Some((closing, name)) = foreign_tag {
                let Some((end, self_closing)) = opening_end(&body[1..]) else {
                    return false;
                };
                if closing {
                    if foreign.pop().map(|(name, _)| name) != Some(name) {
                        return false;
                    }
                } else if !self_closing {
                    if foreign.len() == MAX_NESTED_CONTEXTS {
                        return false;
                    }
                    let ambiguous = !contexts.is_empty()
                        || foreign.last().is_some_and(|(_, ambiguous)| *ambiguous);
                    foreign.push((name, ambiguous));
                }
                body = &body[end + 1..];
                continue;
            }
            let context = SENSITIVE.iter().find_map(|name| {
                let (closing, tag) = if let Some(rest) = body.strip_prefix("</") {
                    (true, rest)
                } else {
                    (false, body.strip_prefix('<')?)
                };
                let tail = tag.strip_prefix(*name)?;
                tail.bytes()
                    .next()
                    .filter(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
                    .map(|_| (closing, *name))
            });
            if let Some((closing, name)) = context {
                let Some((end, _)) = opening_end(&body[1..]) else {
                    return false;
                };
                for (_, ambiguous) in &mut foreign {
                    *ambiguous = true;
                }
                if closing {
                    if contexts.pop() != Some(name) {
                        return false;
                    }
                } else {
                    if contexts.len() == MAX_NESTED_CONTEXTS
                        || ((name == "select" || name == "table") && contexts.contains(&name))
                    {
                        return false;
                    }
                    contexts.push(name);
                }
                body = &body[end + 1..];
                continue;
            }
            let raw = RAW_TEXT.iter().find(|name| {
                body.strip_prefix('<')
                    .and_then(|rest| rest.strip_prefix(**name))
                    .and_then(|rest| rest.bytes().next())
                    .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'>' || byte == b'/')
            });
            if body
                .strip_prefix("<plaintext")
                .and_then(|rest| rest.bytes().next())
                .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'>' || byte == b'/')
            {
                return false;
            }
            if let Some(name) = raw {
                let Some((end, self_closing)) = opening_end(&body[1..]) else {
                    return false;
                };
                if self_closing && let Some((_, ambiguous)) = foreign.last() {
                    if *ambiguous || !contexts.is_empty() {
                        return false;
                    }
                    body = &body[end + 1..];
                    continue;
                }
                body = &body[end + 1..];
                let raw_body = body;
                let marker = format!("</{name}");
                loop {
                    let Some(close) = body.find(&marker) else {
                        return false;
                    };
                    let rest = &body[close + marker.len()..];
                    if rest.bytes().next().is_some_and(|byte| {
                        byte.is_ascii_whitespace() || byte == b'>' || byte == b'/'
                    }) {
                        let raw_prefix_len = raw_body.len() - body.len() + close;
                        let raw_prefix = &raw_body[..raw_prefix_len];
                        if !foreign.is_empty() && raw_prefix.contains('<') {
                            let trimmed = raw_prefix.trim();
                            let Some(inner) = trimmed
                                .strip_prefix("<![cdata[")
                                .and_then(|text| text.strip_suffix("]]>"))
                            else {
                                return false;
                            };
                            if inner.contains("]]>") || inner.contains('<') {
                                return false;
                            }
                        }
                        let mut cdata = &raw_body[..raw_prefix_len];
                        while let Some(start) = cdata.find("<![cdata[") {
                            let rest = &cdata[start + "<![cdata[".len()..];
                            let Some(end) = rest.find("]]>") else {
                                return false;
                            };
                            cdata = &rest[end + 3..];
                        }
                        if *name == "script" && ambiguous_script(&raw_body[..raw_prefix_len]) {
                            return false;
                        }
                        let Some((end, _)) = opening_end(rest) else {
                            return false;
                        };
                        body = &rest[end..];
                        break;
                    }
                    body = rest;
                }
                continue;
            }
            if body.as_bytes().get(1).is_some_and(|byte| {
                byte.is_ascii_alphabetic() || matches!(*byte, b'/' | b'!' | b'?')
            }) {
                let Some((end, _)) = opening_end(&body[1..]) else {
                    return false;
                };
                for (_, ambiguous) in &mut foreign {
                    *ambiguous = true;
                }
                body = &body[end + 1..];
            } else {
                body = &body[1..];
            }
        }
        false
    }

    let Some(lower) = skip_comments_and_space(lower) else {
        return false;
    };
    let root = if let Some(rest) = lower.strip_prefix("<!doctype") {
        if !rest.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
            return false;
        }
        let Some(rest) = rest
            .trim_start_matches(|character: char| character.is_ascii_whitespace())
            .strip_prefix("html")
        else {
            return false;
        };
        if !rest
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'>')
        {
            return false;
        }
        let Some(rest) = doctype_tail(rest) else {
            return false;
        };
        let Some(root) = skip_comments_and_space(rest) else {
            return false;
        };
        root
    } else {
        lower
    };
    let Some(rest) = root.strip_prefix("<html") else {
        return false;
    };
    if !rest
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'>')
    {
        return false;
    }
    let Some((end, _)) = opening_end(rest) else {
        return false;
    };
    terminal_root_close(&rest[end..])
}

fn strip_outer_html_fence(content: &str) -> Option<&str> {
    let first = content.as_bytes().first().copied()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let marker_len = content.bytes().take_while(|byte| *byte == first).count();
    if marker_len < 3 {
        return None;
    }
    let header_end = content.find(['\r', '\n'])?;
    let label = content[marker_len..header_end].trim();
    if !label.is_empty()
        && !label.eq_ignore_ascii_case("html")
        && !label.eq_ignore_ascii_case("htm")
    {
        return None;
    }
    let header_break = if content.as_bytes().get(header_end..header_end + 2) == Some(b"\r\n") {
        2
    } else {
        1
    };
    let body_and_close = &content[header_end + header_break..];
    let close_start = body_and_close.rfind(['\r', '\n'])?;
    let is_closing = |line: &str| {
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        if indent > 3 {
            return false;
        }
        let line = line[indent..].trim_end();
        line.len() >= marker_len && line.bytes().all(|byte| byte == first)
    };
    let body_end = if close_start > 0
        && body_and_close.as_bytes()[close_start] == b'\n'
        && body_and_close.as_bytes()[close_start - 1] == b'\r'
    {
        close_start - 1
    } else {
        close_start
    };
    let body = &body_and_close[..body_end];
    if !is_closing(&body_and_close[close_start + 1..]) || body.split(['\r', '\n']).any(is_closing) {
        return None;
    }
    Some(body)
}
