use std::path::PathBuf;

pub(super) fn pick_image_dialog() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        pick_image_dialog_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

pub(super) fn pick_images_dialog() -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        pick_images_dialog_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

pub(super) fn pick_output_dir_dialog() -> Result<Option<PathBuf>, String> {
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
        let image_pattern = wide("*.png;*.jpg;*.jpeg;*.webp;*.bmp");
        let all_name = wide("All files");
        let all_pattern = wide("*.*");
        let file_types = [
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(image_name.as_ptr()),
                pszSpec: PCWSTR(image_pattern.as_ptr()),
            },
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(all_name.as_ptr()),
                pszSpec: PCWSTR(all_pattern.as_ptr()),
            },
        ];
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
        let image_pattern = wide("*.png;*.jpg;*.jpeg;*.webp;*.bmp");
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
