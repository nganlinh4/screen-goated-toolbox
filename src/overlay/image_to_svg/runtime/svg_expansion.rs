use std::collections::{HashMap, HashSet};

use quick_xml::{Reader, events::Event};

use super::asset_io::bounded_embedded_raster_pixels;

const MAX_EXPANDED_ELEMENTS: u64 = 50_000;
const MAX_EXPANDED_RASTER_PIXELS: u64 = 32_000_000;
const MAX_USE_DEPTH: usize = 64;
const MAX_REFERENCE_OCCURRENCES: usize = 100_000;
const ROOT: &str = "\0sgt-root";

#[derive(Clone, Default)]
struct ExpansionCost {
    elements: u64,
    raster_pixels: u64,
    uses: Vec<String>,
}

impl ExpansionCost {
    fn add(&mut self, other: &Self) -> Result<(), String> {
        self.elements = self
            .elements
            .checked_add(other.elements)
            .filter(|value| *value <= MAX_EXPANDED_ELEMENTS)
            .ok_or_else(expansion_error)?;
        self.raster_pixels = self
            .raster_pixels
            .checked_add(other.raster_pixels)
            .filter(|value| *value <= MAX_EXPANDED_RASTER_PIXELS)
            .ok_or_else(expansion_error)?;
        Ok(())
    }
}

pub(super) fn validate(svg: &str) -> Result<(), String> {
    let mut definitions = HashMap::from([(ROOT.to_string(), ExpansionCost::default())]);
    let mut active_ids = Vec::new();
    let mut id_frames = Vec::new();
    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => record_element(
                &element,
                reader.decoder(),
                true,
                &mut definitions,
                &mut active_ids,
                &mut id_frames,
            )?,
            Ok(Event::Empty(element)) => record_element(
                &element,
                reader.decoder(),
                false,
                &mut definitions,
                &mut active_ids,
                &mut id_frames,
            )?,
            Ok(Event::End(_)) => {
                if id_frames.pop().ok_or_else(expansion_error)? {
                    active_ids.pop().ok_or_else(expansion_error)?;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(expansion_error()),
        }
    }
    if !active_ids.is_empty() || !id_frames.is_empty() {
        return Err(expansion_error());
    }
    let mut visiting = HashSet::new();
    let mut memo = HashMap::new();
    expand(ROOT, &definitions, &mut visiting, &mut memo, 0).map(|_| ())
}

fn record_element(
    element: &quick_xml::events::BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
    persistent: bool,
    definitions: &mut HashMap<String, ExpansionCost>,
    active_ids: &mut Vec<String>,
    id_frames: &mut Vec<bool>,
) -> Result<(), String> {
    let mut id = None;
    let mut targets = Vec::new();
    let mut href = None;
    let mut xlink_href = None;
    let name = String::from_utf8_lossy(element.local_name().as_ref()).to_ascii_lowercase();
    for attribute in element.attributes().with_checks(true) {
        let attribute = attribute.map_err(|_| expansion_error())?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).to_ascii_lowercase();
        let value = attribute
            .decode_and_unescape_value(decoder)
            .map_err(|_| expansion_error())?;
        if key == "id" {
            id = Some(value.to_string());
        } else {
            targets.extend(url_reference_targets(&value));
            match key.as_str() {
                "href" => href = Some(value.into_owned()),
                "xlink:href" => xlink_href = Some(value.into_owned()),
                _ => {}
            }
        }
    }
    let effective_href = href
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            xlink_href
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        });
    if let Some(target) = effective_href.and_then(local_href_target) {
        targets.push(target);
    }
    let raster_pixels = if name == "image" {
        effective_href
            .and_then(bounded_embedded_raster_pixels)
            .unwrap_or(0)
    } else {
        0
    };
    if let Some(id) = &id {
        definitions.entry(id.clone()).or_default();
        active_ids.push(id.clone());
    }
    add_literal_cost(definitions, active_ids, raster_pixels, &targets)?;
    if persistent {
        id_frames.push(id.is_some());
    } else if id.is_some() {
        active_ids.pop();
    }
    Ok(())
}

fn add_literal_cost(
    definitions: &mut HashMap<String, ExpansionCost>,
    active_ids: &[String],
    raster_pixels: u64,
    targets: &[String],
) -> Result<(), String> {
    for owner in std::iter::once(ROOT).chain(active_ids.iter().map(String::as_str)) {
        let cost = definitions.get_mut(owner).ok_or_else(expansion_error)?;
        cost.elements = cost
            .elements
            .checked_add(1)
            .filter(|value| *value <= MAX_EXPANDED_ELEMENTS)
            .ok_or_else(expansion_error)?;
        cost.raster_pixels = cost
            .raster_pixels
            .checked_add(raster_pixels)
            .filter(|value| *value <= MAX_EXPANDED_RASTER_PIXELS)
            .ok_or_else(expansion_error)?;
        cost.uses.extend(targets.iter().cloned());
        if owner == ROOT && cost.uses.len() > MAX_REFERENCE_OCCURRENCES {
            return Err(expansion_error());
        }
    }
    Ok(())
}

fn local_href_target(value: &str) -> Option<String> {
    value.trim().strip_prefix('#').map(ToString::to_string)
}

fn url_reference_targets(value: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let lower = value.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative) = lower[offset..].find("url(") {
        let start = offset + relative + 4;
        let Some(end) = lower[start..].find(')').map(|value| value + start) else {
            break;
        };
        let target = value[start..end].trim().trim_matches(['\'', '"']);
        if let Some(target) = target.strip_prefix('#') {
            targets.push(target.to_string());
        }
        offset = end + 1;
    }
    targets
}

fn expand(
    id: &str,
    definitions: &HashMap<String, ExpansionCost>,
    visiting: &mut HashSet<String>,
    memo: &mut HashMap<String, ExpansionCost>,
    depth: usize,
) -> Result<ExpansionCost, String> {
    if depth > MAX_USE_DEPTH || !visiting.insert(id.to_string()) {
        return Err(expansion_error());
    }
    if let Some(cost) = memo.get(id) {
        visiting.remove(id);
        return Ok(cost.clone());
    }
    let Some(definition) = definitions.get(id) else {
        visiting.remove(id);
        return Ok(ExpansionCost::default());
    };
    let mut total = ExpansionCost {
        elements: definition.elements,
        raster_pixels: definition.raster_pixels,
        uses: Vec::new(),
    };
    for target in &definition.uses {
        total.add(&expand(target, definitions, visiting, memo, depth + 1)?)?;
    }
    visiting.remove(id);
    memo.insert(id.to_string(), total.clone());
    Ok(total)
}

fn expansion_error() -> String {
    "SVG reference expansion is too complex to preview safely.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn href_precedence_and_url_occurrences_are_order_independent() {
        assert_eq!(local_href_target("#primary").as_deref(), Some("primary"));
        assert_eq!(
            url_reference_targets("url(#paint) url('#paint')"),
            vec!["paint", "paint"]
        );
        let href = Some("#primary");
        let xlink_href = Some("#fallback");
        let effective = href
            .filter(|value| !value.trim().is_empty())
            .or_else(|| xlink_href.filter(|value| !value.trim().is_empty()));
        assert_eq!(
            effective.and_then(local_href_target).as_deref(),
            Some("primary")
        );
    }

    #[test]
    fn repeated_reference_charges_each_raster_occurrence() {
        let mut definitions = HashMap::from([
            (
                ROOT.to_string(),
                ExpansionCost {
                    elements: 3,
                    raster_pixels: 16_000_000,
                    uses: vec!["raster".into(), "raster".into()],
                },
            ),
            (
                "raster".into(),
                ExpansionCost {
                    elements: 1,
                    raster_pixels: 16_000_000,
                    uses: Vec::new(),
                },
            ),
        ]);
        let mut visiting = HashSet::new();
        let mut memo = HashMap::new();
        assert!(expand(ROOT, &definitions, &mut visiting, &mut memo, 0).is_err());
        definitions.get_mut(ROOT).unwrap().uses.pop();
        assert!(
            expand(
                ROOT,
                &definitions,
                &mut HashSet::new(),
                &mut HashMap::new(),
                0
            )
            .is_ok()
        );
    }

    #[test]
    fn multiplicity_preserving_dag_fails_before_exponential_render_work() {
        let mut definitions = HashMap::from([(
            ROOT.to_string(),
            ExpansionCost {
                elements: 1,
                raster_pixels: 0,
                uses: vec!["n0".into()],
            },
        )]);
        for index in 0..16 {
            definitions.insert(
                format!("n{index}"),
                ExpansionCost {
                    elements: 3,
                    raster_pixels: 0,
                    uses: vec![format!("n{}", index + 1), format!("n{}", index + 1)],
                },
            );
        }
        definitions.insert(
            "n16".into(),
            ExpansionCost {
                elements: 1,
                raster_pixels: 0,
                uses: Vec::new(),
            },
        );
        assert!(
            expand(
                ROOT,
                &definitions,
                &mut HashSet::new(),
                &mut HashMap::new(),
                0
            )
            .is_err()
        );
    }

    #[test]
    fn reference_occurrence_cap_fails_before_graph_expansion() {
        let targets = vec!["missing".to_string(); MAX_REFERENCE_OCCURRENCES + 1];
        let mut definitions = HashMap::from([(ROOT.to_string(), ExpansionCost::default())]);
        assert!(add_literal_cost(&mut definitions, &[], 0, &targets).is_err());
    }
}
