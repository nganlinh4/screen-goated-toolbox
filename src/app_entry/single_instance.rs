use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use super::arguments::StartupArgs;

pub(super) struct PrimaryInstance {
    pub(super) guard: Option<HANDLE>,
    pub(super) owns_activation: bool,
}

pub(super) enum InstanceOutcome {
    Primary(PrimaryInstance),
    SecondaryNotified,
}

pub(super) fn acquire(args: &StartupArgs, bypass_existing: bool) -> InstanceOutcome {
    unsafe {
        let mutex_name = crate::app_activation::single_instance_mutex_name_wide();
        let instance = CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr()));
        // Capture this immediately: another Win32 call would overwrite the
        // creation status used to identify the singleton owner.
        let already_exists = instance.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS;

        if bypass_existing {
            return match instance {
                Ok(handle) if already_exists => {
                    let _ = CloseHandle(handle);
                    InstanceOutcome::Primary(PrimaryInstance {
                        guard: None,
                        owns_activation: false,
                    })
                }
                Ok(handle) => InstanceOutcome::Primary(PrimaryInstance {
                    guard: Some(handle),
                    owns_activation: true,
                }),
                Err(_) => InstanceOutcome::Primary(PrimaryInstance {
                    guard: None,
                    owns_activation: true,
                }),
            };
        }

        let Ok(handle) = instance else {
            return InstanceOutcome::Primary(PrimaryInstance {
                guard: None,
                owns_activation: true,
            });
        };
        if !already_exists {
            return InstanceOutcome::Primary(PrimaryInstance {
                guard: Some(handle),
                owns_activation: true,
            });
        }

        let pending_file = args.process_with_sgt_file();
        crate::app_activation::notify_primary_instance(pending_file.as_deref());
        let _ = CloseHandle(handle);
        InstanceOutcome::SecondaryNotified
    }
}
