//! Native filesystem dialogs with thread-scoped COM and shell allocation ownership.
use std::ffi::{OsStr, OsString};
use std::marker::PhantomData;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Result, ensure};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FILEOPENDIALOGOPTIONS, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
    IFileDialog, IFileOpenDialog, IShellItem, KNOWN_FOLDER_FLAG, SHCreateItemFromParsingName,
    SHGetKnownFolderPath, SIGDN_FILESYSPATH,
};
#[cfg(not(feature = "recorder-worker"))]
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_OVERWRITEPROMPT, FOS_STRICTFILETYPES,
    FileSaveDialog,
};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::core::{GUID, HRESULT, Interface, PCWSTR, PWSTR};

pub(crate) enum Kind {
    #[cfg(not(feature = "recorder-worker"))]
    Files,
    Directory,
    #[cfg(not(feature = "recorder-worker"))]
    Save,
}

impl Kind {
    /// The recorder worker only picks folders, so it has no file-open kind.
    fn selects_files(&self) -> bool {
        #[cfg(not(feature = "recorder-worker"))]
        return matches!(self, Self::Files);
        #[cfg(feature = "recorder-worker")]
        false
    }
}

pub(crate) struct Filter<'a> {
    pub label: &'a str,
    pub pattern: &'a str,
}

pub(crate) struct Options<'a> {
    pub kind: Kind,
    pub filters: &'a [Filter<'a>],
    pub default_folder: &'a GUID,
    pub initial_folder: Option<&'a Path>,
    pub initial_name: Option<&'a str>,
    pub default_extension: Option<&'a str>,
    pub maximum_selections: usize,
}

struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    fn enter(model: COINIT) -> Result<Self> {
        unsafe { CoInitializeEx(None, model).ok()? };
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct ShellString(PWSTR);
impl Drop for ShellString {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
    }
}
impl ShellString {
    fn path(&self) -> Result<PathBuf> {
        let value = unsafe { self.0.as_wide() };
        ensure!(!value.is_empty(), "The selected path is empty.");
        Ok(PathBuf::from(OsString::from_wide(value)))
    }
}

fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut value: Vec<u16> = value.encode_wide().collect();
    ensure!(
        !value.contains(&0),
        "A dialog setting contains a null character."
    );
    value.push(0);
    Ok(value)
}

fn with_apartment<T>(run: impl FnOnce() -> Result<T>) -> Result<T> {
    let _apartment = Apartment::enter(COINIT_APARTMENTTHREADED)?;
    run()
}

fn flags(options: &Options<'_>) -> FILEOPENDIALOGOPTIONS {
    let mut flags = FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM;
    flags |= match options.kind {
        Kind::Directory => FOS_PICKFOLDERS,
        #[cfg(not(feature = "recorder-worker"))]
        Kind::Save => FOS_OVERWRITEPROMPT | FOS_STRICTFILETYPES,
        #[cfg(not(feature = "recorder-worker"))]
        Kind::Files if options.maximum_selections > 1 => FOS_FILEMUSTEXIST | FOS_ALLOWMULTISELECT,
        #[cfg(not(feature = "recorder-worker"))]
        Kind::Files => FOS_FILEMUSTEXIST,
    };
    flags
}

struct ConfiguredDialog {
    dialog: IFileDialog,
    text: Vec<Vec<u16>>,
}
impl ConfiguredDialog {
    fn text(&mut self, value: &OsStr) -> Result<PCWSTR> {
        self.text.push(wide(value)?);
        Ok(PCWSTR(
            self.text.last().expect("dialog text was pushed").as_ptr(),
        ))
    }
}

fn configured_dialog(options: &Options<'_>) -> Result<ConfiguredDialog> {
    let class = match options.kind {
        #[cfg(not(feature = "recorder-worker"))]
        Kind::Save => &FileSaveDialog,
        _ => &FileOpenDialog,
    };
    let mut configured = ConfiguredDialog {
        dialog: unsafe { CoCreateInstance(class, None, CLSCTX_ALL)? },
        text: Vec::new(),
    };
    unsafe {
        configured.dialog.SetOptions(flags(options))?;
        let filters = options
            .filters
            .iter()
            .map(|filter| {
                Ok(COMDLG_FILTERSPEC {
                    pszName: configured.text(OsStr::new(filter.label))?,
                    pszSpec: configured.text(OsStr::new(filter.pattern))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !filters.is_empty() {
            configured.dialog.SetFileTypes(&filters)?;
            configured.dialog.SetFileTypeIndex(1)?;
        }
        if let Some(initial) = options.initial_folder {
            let initial = configured.text(initial.as_os_str())?;
            if let Ok(folder) = SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(initial, None)
            {
                let _ = configured.dialog.SetFolder(&folder);
            }
        } else if let Ok(value) =
            SHGetKnownFolderPath(options.default_folder, KNOWN_FOLDER_FLAG(0), None)
        {
            let allocated = ShellString(value);
            if let Ok(folder) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(allocated.0.0), None)
            {
                let _ = configured.dialog.SetFolder(&folder);
            }
        }
        if let Some(name) = options.initial_name {
            let name = configured.text(OsStr::new(name))?;
            configured.dialog.SetFileName(name)?;
        }
        if let Some(extension) = options.default_extension {
            let extension = configured.text(OsStr::new(extension))?;
            configured.dialog.SetDefaultExtension(extension)?;
        }
    }
    Ok(configured)
}

fn shown(result: windows::core::Result<()>) -> Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn path(item: &IShellItem) -> Result<PathBuf> {
    ShellString(unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? }).path()
}

pub(crate) fn select(owner: Option<HWND>, options: &Options<'_>) -> Result<Vec<PathBuf>> {
    ensure!(
        options.maximum_selections > 0,
        "The selection limit must be positive."
    );
    ensure!(
        options.kind.selects_files() || options.maximum_selections == 1,
        "This dialog accepts one selection."
    );
    if let Some(owner) = owner {
        ensure!(
            unsafe { IsWindow(Some(owner)).as_bool() },
            "The dialog window is no longer available."
        );
    }
    with_apartment(|| {
        let configured = configured_dialog(options)?;
        let dialog = &configured.dialog;
        if !shown(unsafe { dialog.Show(owner) })? {
            return Ok(Vec::new());
        }
        unsafe {
            if options.kind.selects_files() && options.maximum_selections > 1 {
                let results = dialog.cast::<IFileOpenDialog>()?.GetResults()?;
                let count = results.GetCount()?;
                ensure!(
                    count as usize <= options.maximum_selections,
                    "Select no more than {} files at once.",
                    options.maximum_selections
                );
                (0..count)
                    .map(|index| path(&results.GetItemAt(index)?))
                    .collect()
            } else {
                Ok(vec![path(&dialog.GetResult()?)?])
            }
        }
    })
}

#[cfg(test)]
#[path = "native_file_dialog/tests.rs"]
mod tests;
