use std::path::PathBuf;

pub(crate) const MAX_BATCH_IMAGES: usize = 100;
pub(crate) const MAX_BATCH_BYTES: u64 = 512 * 1024 * 1024;
#[cfg(test)]
const MAX_REFERENCE_IMAGES: usize = 20;
#[cfg(test)]
const MAX_REFERENCE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;

pub(crate) fn pick_image_dialog() -> Result<Option<PathBuf>, String> {
    let selected = raw_pick_image_dialog()?;
    selected
        .map(|path| admit_image_paths(vec![path], 1, MAX_BATCH_BYTES))
        .transpose()
        .map(|paths| paths.and_then(|mut paths| paths.pop()))
}

pub(crate) fn pick_images_dialog() -> Result<Vec<PathBuf>, String> {
    admit_image_paths(raw_pick_images_dialog()?, MAX_BATCH_IMAGES, MAX_BATCH_BYTES)
}

fn raw_pick_image_dialog() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        pick_image_dialog_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

fn raw_pick_images_dialog() -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        pick_images_dialog_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

pub(crate) fn is_supported_image_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp"
            )
        })
}

pub(crate) fn admit_image_paths(
    paths: Vec<PathBuf>,
    max_count: usize,
    max_total_bytes: u64,
) -> Result<Vec<PathBuf>, String> {
    if paths.len() > max_count {
        return Err(format!("Select no more than {max_count} images at once."));
    }
    let mut total = 0_u64;
    for path in &paths {
        if !is_supported_image_path(path) {
            return Err("Images must be PNG, JPEG, or WebP.".to_string());
        }
        let metadata =
            std::fs::metadata(path).map_err(|_| "A selected image is unavailable.".to_string())?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES {
            return Err("A selected image is empty or exceeds 25 MiB.".to_string());
        }
        total = total
            .checked_add(metadata.len())
            .filter(|total| *total <= max_total_bytes)
            .ok_or_else(|| "The selected images are too large in total.".to_string())?;
    }
    Ok(paths)
}

pub(crate) fn pick_output_dir_dialog() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        pick_output_dir_dialog_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn pick_image_dialog_windows() -> Result<Option<PathBuf>, String> {
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOLDERID_Pictures, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
        FileOpenDialog, IFileOpenDialog, IShellItem, KNOWN_FOLDER_FLAG,
        SHCreateItemFromParsingName, SHGetKnownFolderPath, SIGDN_FILESYSPATH,
    };
    use windows::core::PCWSTR;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|err| err.to_string())?;
        let _ = dialog.SetOptions(FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM);
        let image_name = wide("Image files");
        let image_pattern = wide("*.png;*.jpg;*.jpeg;*.webp");
        let file_types = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(image_name.as_ptr()),
            pszSpec: PCWSTR(image_pattern.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&file_types);
        if let Ok(pictures_path) =
            SHGetKnownFolderPath(&FOLDERID_Pictures, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(pictures_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }
        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(None);
        }
        let result = dialog.GetResult().map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let path = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let path_str = path.to_string().unwrap_or_default();
        CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();
        Ok((!path_str.is_empty()).then(|| PathBuf::from(path_str)))
    }
}

#[cfg(windows)]
fn pick_images_dialog_windows() -> Result<Vec<PathBuf>, String> {
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOLDERID_Pictures, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
        FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog, IShellItem, KNOWN_FOLDER_FLAG,
        SHCreateItemFromParsingName, SHGetKnownFolderPath, SIGDN_FILESYSPATH,
    };
    use windows::core::PCWSTR;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|err| err.to_string())?;
        let _ = dialog.SetOptions(
            FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM,
        );
        let image_name = wide("Image files");
        let image_pattern = wide("*.png;*.jpg;*.jpeg;*.webp");
        let file_types = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(image_name.as_ptr()),
            pszSpec: PCWSTR(image_pattern.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&file_types);
        if let Ok(pictures_path) =
            SHGetKnownFolderPath(&FOLDERID_Pictures, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(pictures_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }
        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(Vec::new());
        }
        let results = dialog.GetResults().map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let count = results.GetCount().map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let item = results.GetItemAt(index).map_err(|err| err.to_string())?;
            let path = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|err| err.to_string())?;
            let path_str = path.to_string().unwrap_or_default();
            CoTaskMemFree(Some(path.0 as *const _));
            if !path_str.is_empty() {
                paths.push(PathBuf::from(path_str));
            }
        }
        CoUninitialize();
        Ok(paths)
    }
}

#[cfg(windows)]
fn pick_output_dir_dialog_windows() -> Result<Option<PathBuf>, String> {
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOLDERID_Downloads, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
        FileOpenDialog, IFileOpenDialog, IShellItem, KNOWN_FOLDER_FLAG,
        SHCreateItemFromParsingName, SHGetKnownFolderPath, SIGDN_FILESYSPATH,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|err| err.to_string())?;
        let _ = dialog.SetOptions(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM);
        if let Ok(downloads_path) =
            SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) = SHCreateItemFromParsingName::<
                windows::core::PCWSTR,
                _,
                IShellItem,
            >(windows::core::PCWSTR(downloads_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }
        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(None);
        }
        let result = dialog.GetResult().map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let path = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(|err| {
            CoUninitialize();
            err.to_string()
        })?;
        let path_str = path.to_string().unwrap_or_default();
        CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();
        Ok((!path_str.is_empty()).then(|| PathBuf::from(path_str)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_rejects_the_101st_batch_item_before_io() {
        let paths = (0..=MAX_BATCH_IMAGES)
            .map(|index| PathBuf::from(format!("{index}.png")))
            .collect();
        assert!(admit_image_paths(paths, MAX_BATCH_IMAGES, MAX_BATCH_BYTES).is_err());
    }

    #[test]
    fn admission_rejects_unsupported_extensions() {
        assert!(!is_supported_image_path(
            PathBuf::from("image.bmp").as_path()
        ));
        assert!(!is_supported_image_path(
            PathBuf::from("image.svg").as_path()
        ));
        assert!(is_supported_image_path(
            PathBuf::from("image.WEBP").as_path()
        ));
    }

    #[test]
    fn admission_counts_duplicate_occurrences_toward_the_aggregate_limit() {
        let path = std::env::temp_dir().join(format!(
            "sgt-admission-aggregate-{}.png",
            std::process::id()
        ));
        std::fs::File::create(&path)
            .and_then(|file| file.set_len(MAX_IMAGE_BYTES))
            .unwrap();
        assert!(
            admit_image_paths(
                vec![
                    path.clone(),
                    path.clone(),
                    path.clone(),
                    path.clone(),
                    path.clone(),
                ],
                MAX_REFERENCE_IMAGES,
                MAX_REFERENCE_BYTES,
            )
            .is_err()
        );
        std::fs::remove_file(path).unwrap();
    }
}
