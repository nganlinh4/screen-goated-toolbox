use super::Selection;
use crate::native_file_dialog::{Filter, Kind, Options, select as select_paths};
use anyhow::Result;
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{FOLDERID_Downloads, FOLDERID_Pictures};

pub(super) fn select(owner: HWND, selection: Selection) -> Result<Vec<PathBuf>> {
    let images = matches!(selection, Selection::Images);
    let filters = [Filter {
        label: "Image files",
        pattern: "*.png;*.jpg;*.jpeg;*.webp",
    }];
    select_paths(
        Some(owner),
        &Options {
            kind: if images { Kind::Files } else { Kind::Directory },
            filters: if images { &filters } else { &[] },
            default_folder: if images {
                &FOLDERID_Pictures
            } else {
                &FOLDERID_Downloads
            },
            initial_folder: None,
            initial_name: None,
            default_extension: None,
            maximum_selections: if images { super::MAX_BATCH_IMAGES } else { 1 },
        },
    )
}
