pub fn pick_export_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG;
    use windows::Win32::UI::Shell::{
        FOLDERID_Downloads, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
        FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
        SHGetKnownFolderPath, SIGDN_FILESYSPATH,
    };
    use windows::core::PCWSTR;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|e| e.to_string())?;

        let _ = dialog.SetOptions(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM);

        if let Some(dir) = initial_dir.filter(|d| !d.trim().is_empty()) {
            let dir_w: Vec<u16> = std::ffi::OsStr::new(&dir)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(dir_w.as_ptr()), None)
            {
                let _ = dialog.SetFolder(&folder_item);
            }
        } else if let Ok(downloads_path) =
            SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(downloads_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }

        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(None);
        }

        let result = dialog.GetResult().map_err(|e| {
            CoUninitialize();
            e.to_string()
        })?;

        let path = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| {
            CoUninitialize();
            e.to_string()
        })?;

        let path_str = path.to_string().unwrap_or_default();
        CoTaskMemFree(Some(path.0 as *const _));
        CoUninitialize();

        if path_str.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path_str))
        }
    }
}
