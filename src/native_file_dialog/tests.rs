use super::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoTaskMemAlloc};
use windows::Win32::UI::Shell::FOLDERID_Downloads;

fn thread(test: impl FnOnce() + Send + 'static) {
    std::thread::spawn(test).join().unwrap();
}

#[test]
fn nested_success_balances_each_initialization_reference() {
    thread(|| {
        let outer = Apartment::enter(COINIT_APARTMENTTHREADED).unwrap();
        let nested = Apartment::enter(COINIT_APARTMENTTHREADED).unwrap();
        drop(nested);
        assert!(Apartment::enter(COINIT_MULTITHREADED).is_err());
        drop(outer);
        let mta = Apartment::enter(COINIT_MULTITHREADED).unwrap();
        drop(mta);
        assert!(Apartment::enter(COINIT_APARTMENTTHREADED).is_ok());
    });
}

#[test]
fn failed_initialization_preserves_the_caller_and_skips_the_operation() {
    thread(|| {
        let outer = Apartment::enter(COINIT_MULTITHREADED).unwrap();
        let ran = std::cell::Cell::new(false);
        let failure = with_apartment(|| {
            ran.set(true);
            Ok(())
        });
        assert!(failure.is_err());
        assert!(!ran.get());
        let nested = Apartment::enter(COINIT_MULTITHREADED).unwrap();
        drop(nested);
        assert!(Apartment::enter(COINIT_APARTMENTTHREADED).is_err());
        drop(outer);
        assert!(Apartment::enter(COINIT_APARTMENTTHREADED).is_ok());
    });
}

#[test]
fn early_errors_drop_operation_resources_before_the_apartment() {
    thread(|| {
        struct Resource(std::rc::Rc<std::cell::Cell<bool>>);
        impl Drop for Resource {
            fn drop(&mut self) {
                assert!(Apartment::enter(COINIT_MULTITHREADED).is_err());
                self.0.set(true);
            }
        }
        let released = std::rc::Rc::new(std::cell::Cell::new(false));
        let failure: Result<()> = with_apartment(|| {
            let _resource = Resource(released.clone());
            anyhow::bail!("The selection could not be read.");
        });
        assert!(failure.is_err());
        assert!(released.get());
        assert!(Apartment::enter(COINIT_MULTITHREADED).is_ok());
    });
}

#[test]
fn native_dialog_configuration_releases_interfaces_on_success_and_error() {
    thread(|| {
        let directory = Options {
            kind: Kind::Directory,
            filters: &[],
            default_folder: &FOLDERID_Downloads,
            initial_folder: None,
            initial_name: None,
            default_extension: None,
            maximum_selections: 1,
        };
        #[cfg(not(feature = "recorder-worker"))]
        with_apartment(|| {
            let files = Options {
                kind: Kind::Files,
                filters: &[Filter {
                    label: "Files",
                    pattern: "*.*",
                }],
                maximum_selections: 3,
                ..directory
            };
            let configured = configured_dialog(&files)?;
            let actual = unsafe { configured.dialog.GetOptions()? };
            ensure!(
                actual.contains(FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)
            );
            ensure!(unsafe { configured.dialog.GetFileTypeIndex()? } == 1);
            Ok(())
        })
        .unwrap();
        assert!(Apartment::enter(COINIT_MULTITHREADED).is_ok());
        let failed: Result<()> = with_apartment(|| {
            let configured = configured_dialog(&directory)?;
            ensure!(unsafe { configured.dialog.GetOptions()? }.contains(FOS_PICKFOLDERS));
            anyhow::bail!("The selection could not be read.");
        });
        assert!(failed.is_err());
        assert!(Apartment::enter(COINIT_MULTITHREADED).is_ok());
        #[cfg(not(feature = "recorder-worker"))]
        with_apartment(|| {
            let save = Options {
                kind: Kind::Save,
                initial_name: Some("document.html"),
                default_extension: Some("html"),
                ..directory
            };
            let configured = configured_dialog(&save)?;
            ensure!(
                unsafe { configured.dialog.GetOptions()? }
                    .contains(FOS_OVERWRITEPROMPT | FOS_STRICTFILETYPES)
            );
            let name = ShellString(unsafe { configured.dialog.GetFileName()? });
            ensure!(name.path()? == PathBuf::from("document.html"));
            Ok(())
        })
        .unwrap();
        assert!(Apartment::enter(COINIT_MULTITHREADED).is_ok());
    });
}

#[test]
fn shell_paths_preserve_native_utf16_and_reject_empty_results() {
    fn allocated(value: &[u16]) -> ShellString {
        unsafe {
            let pointer = CoTaskMemAlloc(std::mem::size_of_val(value)).cast::<u16>();
            assert!(!pointer.is_null());
            pointer.copy_from_nonoverlapping(value.as_ptr(), value.len());
            ShellString(PWSTR(pointer))
        }
    }
    let encoded = [67, 58, 92, 0xd800, 46, 119, 97, 118, 0];
    let value = allocated(&encoded);
    let path = value.path().unwrap();
    assert_eq!(
        path.as_os_str().encode_wide().collect::<Vec<_>>(),
        encoded[..encoded.len() - 1]
    );
    assert!(allocated(&[0]).path().is_err());
}

#[test]
fn only_user_cancellation_is_an_empty_selection() {
    assert!(shown(Ok(())).unwrap());
    let cancelled = windows::core::Error::from_hresult(HRESULT::from_win32(ERROR_CANCELLED.0));
    assert!(!shown(Err(cancelled)).unwrap());
    let failed = windows::core::Error::from_hresult(HRESULT(0x80004005_u32 as i32));
    assert!(shown(Err(failed)).is_err());
}

#[test]
fn invalid_limits_and_embedded_null_settings_fail_without_display() {
    let options = Options {
        kind: Kind::Directory,
        filters: &[],
        default_folder: &FOLDERID_Downloads,
        initial_folder: None,
        initial_name: None,
        default_extension: None,
        maximum_selections: 0,
    };
    assert!(select(None, &options).is_err());
    assert!(wide(OsStr::new("name\0suffix")).is_err());
    assert!(
        select(
            Some(HWND::default()),
            &Options {
                maximum_selections: 1,
                ..options
            }
        )
        .is_err()
    );
}
