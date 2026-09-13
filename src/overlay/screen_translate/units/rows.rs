//! Repeated separated columns establish row ownership without reading text.
use super::super::geometry::PixelRegion;

#[derive(Clone, Copy)]
pub(super) struct Cell {
    pub rect: PixelRegion,
    pub paragraph: bool,
    pub table: bool,
}

#[derive(Clone, Copy)]
struct Row {
    left: usize,
    right: usize,
}

pub(super) fn independent(cells: &[Cell], connected: impl Fn(usize, usize) -> bool) -> Vec<bool> {
    let mut protected: Vec<_> = cells.iter().map(|cell| cell.table).collect();
    let mut rows = Vec::new();
    for (i, cell) in cells.iter().enumerate() {
        if cell.paragraph || cell.table {
            continue;
        }
        let a = cell.rect;
        // Use the nearest peer, never skip intervening text to invent a grid.
        let neighbor = cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| {
                let b = cell.rect;
                b.x >= a.x + a.width && aligned_row(a, b)
            })
            .min_by_key(|(_, cell)| cell.rect.x);
        if let Some((j, peer)) = neighbor {
            let b = peer.rect;
            let em = a.height.max(b.height);
            let gap = b.x - (a.x + a.width);
            if !peer.paragraph
                && !peer.table
                && gap >= em * 2
                && gap * 2 >= a.width.min(b.width)
                && (a.width <= a.height * 6 || b.width <= b.height * 6)
                && connected(i, j)
            {
                rows.push(Row { left: i, right: j });
            }
        }
    }
    rows.sort_by_key(|row| cells[row.left].rect.y);
    let mut before = vec![1; rows.len()];
    let mut after = vec![1; rows.len()];
    let mut parents: Vec<_> = (0..rows.len()).collect();
    // At most one edge per OCR region: quadratic scalar geometry, no raster
    // scans or inference, bounded by the existing capture region limit.
    for i in 0..rows.len() {
        for j in 0..i {
            if continuation(rows[j], rows[i], cells) {
                before[i] = before[i].max(before[j] + 1);
                connect(&mut parents, i, j);
            } else if rows[j].right == rows[i].left || rows[i].right == rows[j].left {
                connect(&mut parents, i, j);
            }
        }
    }
    let mut areas: Vec<Option<[u32; 4]>> = vec![None; rows.len()];
    for i in (0..rows.len()).rev() {
        for j in i + 1..rows.len() {
            if continuation(rows[i], rows[j], cells) {
                after[i] = after[i].max(after[j] + 1);
            }
        }
        if before[i] + after[i] >= 4 {
            let left = cells[rows[i].left].rect;
            let right = cells[rows[i].right].rect;
            let bounds = [
                left.x,
                left.y.min(right.y),
                right.x + right.width,
                (left.y + left.height).max(right.y + right.height),
            ];
            let area = &mut areas[root(&mut parents, i)];
            *area = Some(match *area {
                Some(a) => [
                    a[0].min(bounds[0]),
                    a[1].min(bounds[1]),
                    a[2].max(bounds[2]),
                    a[3].max(bounds[3]),
                ],
                None => bounds,
            });
        }
    }
    // Missing peers and wrapped labels inside an established grid still own
    // their positions. Do not let them bridge protected rows into prose.
    for (i, cell) in cells.iter().enumerate() {
        if cell.paragraph {
            continue;
        }
        let cx = cell.rect.x * 2 + cell.rect.width;
        let cy = cell.rect.y * 2 + cell.rect.height;
        protected[i] |= areas
            .iter()
            .flatten()
            .any(|a| cx >= a[0] * 2 && cx <= a[2] * 2 && cy >= a[1] * 2 && cy <= a[3] * 2);
    }
    protected
}

fn root(parents: &mut [usize], mut i: usize) -> usize {
    while parents[i] != i {
        parents[i] = parents[parents[i]];
        i = parents[i];
    }
    i
}

fn connect(parents: &mut [usize], a: usize, b: usize) {
    let a = root(parents, a);
    let b = root(parents, b);
    parents[a.max(b)] = a.min(b);
}

fn aligned_row(a: PixelRegion, b: PixelRegion) -> bool {
    a.height <= a.width
        && b.height <= b.width
        && a.height.max(b.height) * 2 <= a.height.min(b.height) * 3
        && (a.y * 2 + a.height).abs_diff(b.y * 2 + b.height) <= a.height.min(b.height)
}

fn aligned_column(a: PixelRegion, b: PixelRegion) -> bool {
    let tolerance = a.height.max(b.height);
    a.x.abs_diff(b.x)
        .min((a.x + a.width).abs_diff(b.x + b.width))
        <= tolerance
}

fn continuation(a: Row, b: Row, cells: &[Cell]) -> bool {
    let (al, ar, bl, br) = (
        cells[a.left].rect,
        cells[a.right].rect,
        cells[b.left].rect,
        cells[b.right].rect,
    );
    let em = al.height.max(ar.height).max(bl.height).max(br.height);
    bl.y >= al.y + al.height / 2
        && bl.y - al.y <= em * 3
        && br.y >= ar.y + ar.height / 2
        && br.y - ar.y <= em * 3
        && aligned_column(al, bl)
        && aligned_column(ar, br)
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
