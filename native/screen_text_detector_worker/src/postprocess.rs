use sgt_screen_text_detector_protocol::{DetectedRegion, MAX_REGIONS};

const PIXEL_THRESHOLD: f32 = 0.3;
const BOX_THRESHOLD: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;
const MIN_SIDE: u32 = 3;

#[derive(Clone, Copy, Debug)]
struct ComponentBox {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

#[derive(Clone, Copy, Debug)]
struct ProbabilityComponent {
    bounds: ComponentBox,
    probability_sum: f32,
    pixel_count: u32,
}

impl ProbabilityComponent {
    fn confidence(self) -> f32 {
        self.probability_sum / self.pixel_count.max(1) as f32
    }
}

impl ComponentBox {
    fn width(self) -> u32 {
        self.right - self.left + 1
    }

    fn height(self) -> u32 {
        self.bottom - self.top + 1
    }
}

pub(crate) fn extract_regions(
    probabilities: &[f32],
    map_width: u32,
    map_height: u32,
    image_width: u32,
    image_height: u32,
) -> Vec<DetectedRegion> {
    let expected = map_width as usize * map_height as usize;
    if probabilities.len() != expected || expected == 0 {
        return Vec::new();
    }
    let mut visited = vec![false; expected];
    let mut regions = Vec::new();
    for index in 0..expected {
        if regions.len() >= MAX_REGIONS || visited[index] || probabilities[index] <= PIXEL_THRESHOLD
        {
            continue;
        }
        let component = visit_component(probabilities, &mut visited, index, map_width, map_height);
        let bounds = component.bounds;
        if bounds.width().min(bounds.height()) < MIN_SIDE {
            continue;
        }
        let confidence = component.confidence();
        if confidence < BOX_THRESHOLD {
            continue;
        }
        let expanded = expand_box(bounds, map_width, map_height);
        if expanded.width().min(expanded.height()) < MIN_SIDE + 2 {
            continue;
        }
        regions.push(scale_box(
            expanded,
            confidence,
            map_width,
            map_height,
            image_width,
            image_height,
        ));
    }
    regions.retain(|region| region.left < region.right && region.top < region.bottom);
    sort_reading_order(&mut regions);
    regions
}

fn visit_component(
    probabilities: &[f32],
    visited: &mut [bool],
    start: usize,
    width: u32,
    height: u32,
) -> ProbabilityComponent {
    let mut stack = vec![start];
    visited[start] = true;
    let mut bounds = ComponentBox {
        left: start as u32 % width,
        top: start as u32 / width,
        right: start as u32 % width,
        bottom: start as u32 / width,
    };
    let mut probability_sum = 0.0_f32;
    let mut pixel_count = 0_u32;
    while let Some(index) = stack.pop() {
        probability_sum += probabilities[index];
        pixel_count += 1;
        let x = index as u32 % width;
        let y = index as u32 / width;
        bounds.left = bounds.left.min(x);
        bounds.top = bounds.top.min(y);
        bounds.right = bounds.right.max(x);
        bounds.bottom = bounds.bottom.max(y);
        let x0 = x.saturating_sub(1);
        let y0 = y.saturating_sub(1);
        let x1 = (x + 1).min(width - 1);
        let y1 = (y + 1).min(height - 1);
        for neighbor_y in y0..=y1 {
            for neighbor_x in x0..=x1 {
                let neighbor = (neighbor_y * width + neighbor_x) as usize;
                if !visited[neighbor] && probabilities[neighbor] > PIXEL_THRESHOLD {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
    }
    ProbabilityComponent {
        bounds,
        probability_sum,
        pixel_count,
    }
}

fn expand_box(bounds: ComponentBox, width: u32, height: u32) -> ComponentBox {
    let box_width = bounds.width() as f32;
    let box_height = bounds.height() as f32;
    let distance = box_width * box_height * UNCLIP_RATIO / (2.0 * (box_width + box_height));
    ComponentBox {
        left: (bounds.left as f32 - distance).floor().max(0.0) as u32,
        top: (bounds.top as f32 - distance).floor().max(0.0) as u32,
        right: (bounds.right as f32 + distance)
            .ceil()
            .min((width - 1) as f32) as u32,
        bottom: (bounds.bottom as f32 + distance)
            .ceil()
            .min((height - 1) as f32) as u32,
    }
}

fn scale_box(
    bounds: ComponentBox,
    confidence: f32,
    map_width: u32,
    map_height: u32,
    image_width: u32,
    image_height: u32,
) -> DetectedRegion {
    let scale_x = image_width as f64 / map_width as f64;
    let scale_y = image_height as f64 / map_height as f64;
    DetectedRegion {
        left: (bounds.left as f64 * scale_x).round() as u32,
        top: (bounds.top as f64 * scale_y).round() as u32,
        right: (((bounds.right + 1) as f64 * scale_x).round() as u32).min(image_width),
        bottom: (((bounds.bottom + 1) as f64 * scale_y).round() as u32).min(image_height),
        confidence: confidence.clamp(0.0, 1.0),
        text: String::new(),
        text_confidence: 0.0,
        alternatives: Vec::new(),
    }
}

fn sort_reading_order(regions: &mut [DetectedRegion]) {
    let mut heights = regions
        .iter()
        .map(|region| region.bottom.saturating_sub(region.top).max(1))
        .collect::<Vec<_>>();
    heights.sort_unstable();
    let row_quantum = heights.get(heights.len() / 2).copied().unwrap_or(4).max(4) / 2;
    regions.sort_by_key(|region| {
        (
            region.top / row_quantum,
            region.left,
            region.top,
            region.bottom,
            region.right,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_probability_regions_expand_and_sort() {
        let mut map = vec![0.0_f32; 32 * 32];
        for y in 4..8 {
            for x in 10..20 {
                map[y * 32 + x] = 0.9;
            }
        }
        for y in 20..24 {
            for x in 2..12 {
                map[y * 32 + x] = 0.8;
            }
        }
        let regions = extract_regions(&map, 32, 32, 320, 160);
        assert_eq!(regions.len(), 2);
        assert!(regions[0].top < regions[1].top);
        assert!(regions[0].left < 100 && regions[0].right > 200);
        assert!(regions.iter().all(|region| region.confidence >= 0.79));
    }

    #[test]
    fn confidence_scores_the_connected_text_mask_not_empty_bounding_space() {
        let mut map = vec![0.0_f32; 32 * 32];
        for y in 4..20 {
            map[y * 32 + 4] = 0.9;
            map[y * 32 + 20] = 0.9;
        }
        for x in 4..=20 {
            map[4 * 32 + x] = 0.9;
        }

        let regions = extract_regions(&map, 32, 32, 320, 320);
        assert_eq!(regions.len(), 1);
        assert!(regions[0].confidence >= 0.89);
    }

    #[test]
    fn reading_order_is_total_for_overlapping_row_tolerances() {
        let region = |left, top, right, bottom| DetectedRegion {
            left,
            top,
            right,
            bottom,
            confidence: 0.9,
            text: String::new(),
            text_confidence: 0.0,
            alternatives: Vec::new(),
        };
        let mut regions = vec![
            region(60, 16, 90, 40),
            region(10, 0, 40, 10),
            region(20, 8, 50, 24),
            region(5, 16, 35, 40),
        ];
        sort_reading_order(&mut regions);
        assert_eq!(
            regions.iter().map(|item| item.left).collect::<Vec<_>>(),
            [10, 20, 5, 60]
        );
    }
}
