//! Structured output repeats whole tokens on every row. None of it is a
//! broken word, so none of it may be cut, while a damaged restatement of the
//! same list still must be.

use super::{repetition_onset, salvage};

fn numbered_list(rows: usize) -> String {
    (1..=rows)
        .map(|row| format!("{row}. Item {row}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn legitimate_structured_replies() -> Vec<(&'static str, String)> {
    let join = |rows: Vec<String>| rows.join("\n");
    vec![
        ("numbered list", numbered_list(129)),
        (
            "status table",
            join((1..130).map(|row| format!("Row {row}: OK")).collect()),
        ),
        (
            "code listing",
            join(
                (1..130)
                    .map(|row| format!("let value{row} = compute({row});"))
                    .collect(),
            ),
        ),
        (
            "dated ledger",
            join(
                (1..130)
                    .map(|row| format!("{:02}/11/2026 Payment {row} 100.00", row % 28 + 1))
                    .collect(),
            ),
        ),
        (
            "extension directory",
            join(
                (0..150)
                    .map(|row| format!("Extension {}: Room {}", 100 + row, 200 + row))
                    .collect(),
            ),
        ),
        (
            "page footer",
            join((1..200).map(|row| format!("Page {row} of 300")).collect()),
        ),
        (
            "space-separated identifiers",
            (0..1_500)
                .map(|index| format!("item{index}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    ]
}

#[test]
fn structured_lists_with_doubled_digits_are_never_cut() {
    // `11`, `100` and `1100` used to fold into `1`, `10` and `10`, so two
    // whole rows read back as one broken word and the reply was truncated at
    // the first doubled digit.
    for (name, reply) in legitimate_structured_replies() {
        assert_eq!(
            repetition_onset(&reply),
            None,
            "{name} was cut at {:?}",
            repetition_onset(&reply).map(|onset| &reply[onset.saturating_sub(24)..onset])
        );
    }
}

#[test]
fn a_damaged_restatement_of_a_list_keeps_every_correct_row() {
    let good = numbered_list(12);
    for damage in [
        "1. It\nem 1\n2. I\ntem 2\n3. Ite\nm 3\n4. It\nem 4",
        "1. Ite\nm 1\n2. Item\n 2\n3. It\nem 3\n4. Ite\nm 4\n5. It\nem 5",
    ] {
        let corrupted = format!("{good}\n{damage}");
        assert_eq!(salvage(&corrupted).as_deref(), Some(good.as_str()));
    }
}
