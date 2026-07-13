use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use super::arguments::StartupArgs;

pub(super) struct SingleInstanceGuard(HANDLE);

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(super) struct PrimaryInstance {
    pub(super) guard: Option<SingleInstanceGuard>,
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
                    drop(SingleInstanceGuard(handle));
                    InstanceOutcome::Primary(PrimaryInstance {
                        guard: None,
                        owns_activation: false,
                    })
                }
                Ok(handle) => InstanceOutcome::Primary(PrimaryInstance {
                    guard: Some(SingleInstanceGuard(handle)),
                    owns_activation: true,
                }),
                Err(error) => coordination_unavailable(error),
            };
        }

        let handle = match instance {
            Ok(handle) => handle,
            Err(error) => return coordination_unavailable(error),
        };
        if !already_exists {
            return InstanceOutcome::Primary(PrimaryInstance {
                guard: Some(SingleInstanceGuard(handle)),
                owns_activation: true,
            });
        }

        let pending_file = args.process_with_sgt_file();
        crate::app_activation::notify_primary_instance(pending_file.as_deref());
        drop(SingleInstanceGuard(handle));
        InstanceOutcome::SecondaryNotified
    }
}

fn coordination_unavailable(error: windows::core::Error) -> InstanceOutcome {
    crate::log_info!(
        "[Activation] Single-instance mutex unavailable; cross-process activation disabled: {error}"
    );
    primary_without_coordination()
}

fn primary_without_coordination() -> InstanceOutcome {
    InstanceOutcome::Primary(PrimaryInstance {
        guard: None,
        owns_activation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::{InstanceOutcome, primary_without_coordination};

    #[test]
    fn mutex_failure_never_claims_restore_listener_ownership() {
        let InstanceOutcome::Primary(instance) = primary_without_coordination() else {
            panic!("mutex failure must keep the process running without coordination");
        };

        assert!(instance.guard.is_none());
        assert!(!instance.owns_activation);
    }
}
