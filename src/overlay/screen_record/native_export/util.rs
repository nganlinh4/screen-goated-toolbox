pub fn pick_export_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    use crate::native_file_dialog::{Kind, Options, select};
    use windows::Win32::UI::Shell::FOLDERID_Downloads;
    let initial_dir = initial_dir.filter(|directory| !directory.trim().is_empty());
    let mut paths = select(None, &Options {
        kind: Kind::Directory,
        filters: &[],
        default_folder: &FOLDERID_Downloads,
        initial_folder: initial_dir.as_deref().map(std::path::Path::new),
        initial_name: None, default_extension: None,
        maximum_selections: 1,
    }).map_err(|error| error.to_string())?;
    paths.pop().map(|path| path.into_os_string().into_string()
        .map_err(|_| "The selected folder cannot be represented as text.".to_string())).transpose()
}
