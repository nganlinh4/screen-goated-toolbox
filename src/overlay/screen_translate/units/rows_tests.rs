use super::{Cell, PixelRegion};

fn independent(cells: &[Cell]) -> Vec<bool> {
    super::independent(cells, |_, _| true)
}

fn cell(x: u32, y: u32, width: u32) -> Cell {
    Cell {
        rect: PixelRegion {
            x,
            y,
            width,
            height: 20,
        },
        paragraph: false,
        table: false,
    }
}

#[test]
fn repeated_borderless_rows_keep_all_columns_and_end_rows() {
    let cells: Vec<_> = (0..5)
        .flat_map(|row| {
            [
                cell(20, row * 28, 50),
                cell(200, row * 28 + 2, 140),
                cell(440, row * 28, 55),
            ]
        })
        .collect();
    assert!(independent(&cells).into_iter().all(|value| value));
}

#[test]
fn paragraph_evidence_and_wide_prose_columns_are_not_grids() {
    let mut cells: Vec<_> = (0..4)
        .flat_map(|row| [cell(20, row * 28, 50), cell(200, row * 28, 100)])
        .collect();
    for cell in &mut cells {
        cell.paragraph = true;
    }
    assert!(independent(&cells).into_iter().all(|value| !value));
    let prose: Vec<_> = (0..5)
        .flat_map(|row| [cell(20, row * 28, 220), cell(270, row * 28, 210)])
        .collect();
    assert!(independent(&prose).into_iter().all(|value| !value));
}

#[test]
fn isolated_alignments_and_single_column_wrapping_are_not_grids() {
    let cells = [
        cell(20, 0, 50),
        cell(200, 0, 50),
        cell(20, 28, 50),
        cell(200, 28, 50),
    ];
    assert!(independent(&cells).into_iter().all(|value| !value));
    let paragraph = [cell(20, 0, 200), cell(20, 28, 190), cell(20, 56, 50)];
    assert!(independent(&paragraph).into_iter().all(|value| !value));
}

#[test]
fn intervening_text_and_different_row_spacing_do_not_invent_grid_peers() {
    let cells: Vec<_> = (0..4)
        .flat_map(|row| {
            [
                cell(20, row * 28, 50),
                cell(80, row * 28, 100),
                cell(200, row * 70, 50),
            ]
        })
        .collect();
    assert!(independent(&cells).into_iter().all(|value| !value));
}

#[test]
fn explicit_tables_keep_rows_even_inside_a_paragraph_hint() {
    let mut cells = [cell(20, 0, 200), cell(20, 28, 180)];
    for cell in &mut cells {
        cell.table = true;
        cell.paragraph = true;
    }
    assert_eq!(independent(&cells), [true, true]);
}

#[test]
fn established_grid_preserves_interior_missing_peers_but_not_unrelated_prose() {
    let mut cells: Vec<_> = (0..3)
        .flat_map(|row| [cell(20, row * 56, 50), cell(240, row * 56, 60)])
        .collect();
    cells.push(cell(20, 28, 180));
    cells.push(cell(20, 84, 160));
    cells.last_mut().unwrap().paragraph = true;
    cells.push(cell(20, 200, 180));
    let protected = independent(&cells);
    assert!(protected[..7].iter().all(|value| *value));
    assert!(!protected[7]);
    assert!(!protected[8]);
}

#[test]
fn scaling_and_input_order_do_not_change_row_ownership() {
    for scale in [1, 2, 3] {
        let mut cells: Vec<_> = (0..5)
            .flat_map(|row| [cell(20, row * 28, 50), cell(200, row * 28 + 2, 140)])
            .collect();
        for cell in &mut cells {
            cell.rect.x *= scale;
            cell.rect.y *= scale;
            cell.rect.width *= scale;
            cell.rect.height *= scale;
        }
        cells.reverse();
        assert!(independent(&cells).into_iter().all(|value| value));
    }
}
