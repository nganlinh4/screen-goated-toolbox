use std::collections::{HashMap, HashSet};

use quick_xml::{
    Reader,
    encoding::Decoder,
    events::{BytesStart, Event},
};

const MAX_DEPTH: usize = 128;
const MAX_REFERENCE_DEPTH: usize = 64;
const MAX_REFERENCE_EDGES: usize = 100_000;
const MAX_PATH_COMMANDS: usize = 250_000;
const MAX_GEOMETRY_NUMBERS: usize = 1_000_000;
const MAX_GEOMETRY_ATTRIBUTE_BYTES: usize = 262_144;
const MAX_STYLE_ATTRIBUTE_BYTES: usize = 131_072;
const MAX_ATTRIBUTE_BYTES: usize = 32_768;
const MAX_EMBEDDED_RASTER_CHARACTERS: usize = 2_800_000;
const MAX_LOCAL_IDENTIFIER_BYTES: usize = 512;
const MAX_COORDINATE: f64 = 10_000_000.0;

#[derive(Default)]
struct SecurityState {
    depth: usize,
    frames_with_id: Vec<bool>,
    ancestor_ids: Vec<String>,
    graph: HashMap<String, HashSet<String>>,
    reference_edges: usize,
    path_commands: usize,
    geometry_numbers: usize,
}

impl SecurityState {
    fn enter(
        &mut self,
        element: &BytesStart<'_>,
        decoder: Decoder,
        persistent: bool,
    ) -> Result<(), String> {
        self.depth = self.depth.checked_add(1).ok_or_else(complexity_error)?;
        if self.depth > MAX_DEPTH {
            return Err(complexity_error());
        }
        let mut id = None;
        let mut references = HashSet::new();
        for attribute in element.attributes().with_checks(true) {
            let attribute = attribute.map_err(|_| complexity_error())?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).to_ascii_lowercase();
            let key = key.rsplit(':').next().unwrap_or(&key);
            let value = attribute
                .decode_and_unescape_value(decoder)
                .map_err(|_| complexity_error())?;
            validate_attribute_size(key, &value)?;
            if key == "id" {
                validate_local_identifier(&value)?;
                id = Some(value.to_string());
            }
            if is_geometry_attribute(key) {
                scan_geometry(
                    &value,
                    key == "d",
                    &mut self.path_commands,
                    &mut self.geometry_numbers,
                )?;
            }
            collect_local_references(key, &value, &mut references)?;
        }
        if let Some(id) = &id
            && self.graph.insert(id.clone(), HashSet::new()).is_some()
        {
            return Err(complexity_error());
        }
        for owner in self.ancestor_ids.iter().chain(id.iter()) {
            let edges = self.graph.get_mut(owner).ok_or_else(complexity_error)?;
            for reference in &references {
                if edges.insert(reference.clone()) {
                    self.reference_edges += 1;
                    if self.reference_edges > MAX_REFERENCE_EDGES {
                        return Err(complexity_error());
                    }
                }
            }
        }
        if persistent {
            self.frames_with_id.push(id.is_some());
            if let Some(id) = id {
                self.ancestor_ids.push(id);
            }
        } else {
            self.depth -= 1;
        }
        Ok(())
    }

    fn leave(&mut self) -> Result<(), String> {
        if self.depth == 0 {
            return Err(complexity_error());
        }
        if self.frames_with_id.pop().ok_or_else(complexity_error)? {
            self.ancestor_ids.pop().ok_or_else(complexity_error)?;
        }
        self.depth -= 1;
        Ok(())
    }
}

pub(super) fn validate(svg: &str) -> Result<(), String> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    let mut state = SecurityState::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => state.enter(&element, reader.decoder(), true)?,
            Ok(Event::Empty(element)) => state.enter(&element, reader.decoder(), false)?,
            Ok(Event::End(_)) => state.leave()?,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(complexity_error()),
        }
    }
    if state.depth != 0 || !state.frames_with_id.is_empty() || !state.ancestor_ids.is_empty() {
        return Err(complexity_error());
    }
    validate_reference_graph(&state.graph)
}

pub(super) fn validate_url_functions(value: &str) -> Result<(), String> {
    let mut remaining = value.to_ascii_lowercase();
    while let Some(index) = remaining.find("url(") {
        let after = &remaining[index + 4..];
        let end = after
            .find(')')
            .ok_or_else(|| "SVG contains a malformed resource URL.".to_string())?;
        let target = after[..end].trim().trim_matches(['\'', '"']);
        if !target.starts_with('#') || target.len() < 2 || target.chars().any(char::is_whitespace) {
            return Err("SVG cannot load external resources.".to_string());
        }
        remaining = after[end + 1..].to_string();
    }
    Ok(())
}

pub(super) fn validate_css(css: &str, stylesheet: bool) -> Result<(), String> {
    let normalized = css.to_ascii_lowercase();
    let compact = normalized
        .chars()
        .filter(|value| !value.is_ascii_whitespace())
        .collect::<String>();
    if normalized.contains('\\')
        || normalized.contains("/*")
        || normalized.contains("*/")
        || normalized.contains("<!--")
        || normalized.contains("-->")
        || normalized.contains("@import")
        || normalized.contains("javascript:")
        || normalized.contains("vbscript:")
        || normalized.contains("expression(")
        || normalized.contains("-moz-binding")
        || normalized.contains("behavior:")
        || normalized.contains("image-set(")
        || normalized.contains("@keyframes")
        || normalized.contains("@-webkit-keyframes")
        || contains_motion_property(&normalized)
        || compact.contains("filter:")
    {
        return Err("SVG contains unsupported active content.".to_string());
    }
    if stylesheet && normalized.contains("url(") {
        return Err("SVG stylesheet resource references are unsupported.".to_string());
    }
    validate_url_functions(&normalized)
}

#[cfg(windows)]
pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(super) fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn validate_attribute_size(key: &str, value: &str) -> Result<(), String> {
    let maximum = match key {
        "href" => MAX_EMBEDDED_RASTER_CHARACTERS,
        "d" | "points" => MAX_GEOMETRY_ATTRIBUTE_BYTES,
        "style" => MAX_STYLE_ATTRIBUTE_BYTES,
        _ => MAX_ATTRIBUTE_BYTES,
    };
    if value.len() > maximum {
        return Err(complexity_error());
    }
    Ok(())
}

fn contains_motion_property(css: &str) -> bool {
    css.split(['{', '}', ';']).any(|declaration| {
        let Some((name, _)) = declaration.split_once(':') else {
            return false;
        };
        let name = name.trim();
        name == "animation"
            || name.starts_with("animation-")
            || name == "-webkit-animation"
            || name.starts_with("-webkit-animation-")
            || name == "transition"
            || name.starts_with("transition-")
    })
}

fn validate_local_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_LOCAL_IDENTIFIER_BYTES
        || value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(character, '\\' | '%' | '\'' | '"' | '(' | ')')
        })
    {
        return Err(complexity_error());
    }
    Ok(())
}

fn collect_local_references(
    key: &str,
    value: &str,
    references: &mut HashSet<String>,
) -> Result<(), String> {
    let trimmed = value.trim();
    if key == "href"
        && let Some(target) = trimmed.strip_prefix('#')
    {
        validate_local_identifier(target)?;
        references.insert(target.to_string());
    }
    let lower = value.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative) = lower[offset..].find("url(") {
        let start = offset + relative + 4;
        let end = lower[start..].find(')').ok_or_else(complexity_error)? + start;
        let target = value[start..end].trim().trim_matches(['\'', '"']);
        if let Some(target) = target.strip_prefix('#') {
            validate_local_identifier(target)?;
            references.insert(target.to_string());
        }
        offset = end + 1;
    }
    Ok(())
}

fn is_geometry_attribute(key: &str) -> bool {
    matches!(
        key,
        "d" | "points"
            | "viewbox"
            | "transform"
            | "x"
            | "y"
            | "x1"
            | "y1"
            | "x2"
            | "y2"
            | "cx"
            | "cy"
            | "r"
            | "rx"
            | "ry"
            | "width"
            | "height"
    )
}

fn scan_geometry(
    value: &str,
    path: bool,
    path_commands: &mut usize,
    geometry_numbers: &mut usize,
) -> Result<(), String> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("nan") || lower.contains("inf") {
        return Err(complexity_error());
    }
    if path {
        *path_commands = path_commands
            .checked_add(
                value
                    .bytes()
                    .filter(|byte| b"AaCcHhLlMmQqSsTtVvZz".contains(byte))
                    .count(),
            )
            .ok_or_else(complexity_error)?;
        if *path_commands > MAX_PATH_COMMANDS {
            return Err(complexity_error());
        }
    }
    let bytes = value.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = number_end(bytes, offset) {
            let number = value[offset..end]
                .parse::<f64>()
                .map_err(|_| complexity_error())?;
            if !number.is_finite() || number.abs() > MAX_COORDINATE {
                return Err(complexity_error());
            }
            *geometry_numbers = geometry_numbers
                .checked_add(1)
                .ok_or_else(complexity_error)?;
            if *geometry_numbers > MAX_GEOMETRY_NUMBERS {
                return Err(complexity_error());
            }
            offset = end;
        } else {
            offset += 1;
        }
    }
    Ok(())
}

fn number_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut offset = start;
    if matches!(bytes.get(offset), Some(b'+' | b'-')) {
        offset += 1;
    }
    let integer_start = offset;
    while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
        offset += 1;
    }
    let mut digits = offset > integer_start;
    if bytes.get(offset) == Some(&b'.') {
        offset += 1;
        let fraction_start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        digits |= offset > fraction_start;
    }
    if !digits {
        return None;
    }
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        let exponent = offset;
        offset += 1;
        if matches!(bytes.get(offset), Some(b'+' | b'-')) {
            offset += 1;
        }
        let exponent_start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        if offset == exponent_start {
            offset = exponent;
        }
    }
    Some(offset)
}

fn validate_reference_graph(graph: &HashMap<String, HashSet<String>>) -> Result<(), String> {
    let mut states = HashMap::new();
    let mut heights = HashMap::new();
    for id in graph.keys() {
        visit_reference(id, graph, &mut states, &mut heights, 0)?;
    }
    Ok(())
}

fn visit_reference<'a>(
    id: &'a str,
    graph: &'a HashMap<String, HashSet<String>>,
    states: &mut HashMap<&'a str, u8>,
    heights: &mut HashMap<&'a str, usize>,
    depth: usize,
) -> Result<usize, String> {
    let Some(references) = graph.get(id) else {
        return Ok(0);
    };
    if depth >= MAX_REFERENCE_DEPTH {
        return Err(complexity_error());
    }
    match states.get(id).copied() {
        Some(1) => return Err(complexity_error()),
        Some(2) => {
            let height = heights.get(id).copied().ok_or_else(complexity_error)?;
            if depth.saturating_add(height) > MAX_REFERENCE_DEPTH {
                return Err(complexity_error());
            }
            return Ok(height);
        }
        _ => {}
    }
    states.insert(id, 1);
    let mut height = 1_usize;
    for reference in references {
        let child_height =
            visit_reference(reference, graph, states, heights, depth.saturating_add(1))?;
        height = height.max(child_height.saturating_add(1));
        if depth.saturating_add(height) > MAX_REFERENCE_DEPTH {
            return Err(complexity_error());
        }
    }
    states.insert(id, 2);
    heights.insert(id, height);
    Ok(height)
}

fn complexity_error() -> String {
    "SVG is too complex to preview safely.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_excessive_depth_geometry_and_identifier_memory() {
        let deep = format!(
            "<svg>{}<path d=\"M0 0\"/>{}</svg>",
            "<g>".repeat(MAX_DEPTH),
            "</g>".repeat(MAX_DEPTH)
        );
        assert!(validate(&deep).is_err());
        assert!(validate("<svg><path d=\"M10000001 0\"/></svg>").is_err());
        assert!(validate("<svg><path d=\"MNaN 0\"/></svg>").is_err());
        let id = "x".repeat(MAX_LOCAL_IDENTIFIER_BYTES + 1);
        assert!(validate(&format!("<svg><g id=\"{id}\"/></svg>")).is_err());
    }

    #[test]
    fn rejects_duplicate_and_cyclic_local_references() {
        assert!(validate("<svg><g id=\"a\"/><g id=\"a\"/></svg>").is_err());
        assert!(
            validate(
                "<svg><g id=\"a\"><use href=\"#b\"/></g><g id=\"b\"><use href=\"#a\"/></g></svg>"
            )
            .is_err()
        );
        assert!(
            validate(
                "<svg><linearGradient id=\"a\"/><path id=\"b\" fill=\"url(#a)\" d=\"M0 0h1\"/></svg>"
            )
            .is_ok()
        );
        assert!(validate("<svg><g id=\"a\"><use href=\"#%61\"/></g></svg>").is_err());
    }

    #[test]
    fn rejects_long_reference_chains_regardless_of_shared_suffixes() {
        let mut body = String::new();
        for index in 0..=MAX_REFERENCE_DEPTH {
            let next = index + 1;
            body.push_str(&format!(
                "<g id=\"node{index}\"><use href=\"#node{next}\"/></g>"
            ));
        }
        assert!(validate(&format!("<svg>{body}</svg>")).is_err());
    }
}
