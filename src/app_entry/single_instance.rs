use std::os::windows::io::OwnedHandle;
use std::time::Duration;

#[path = "instance_channel.rs"]
mod instance_channel;
#[path = "instance_security.rs"]
mod instance_security;

use super::arguments::StartupArgs;
use instance_security::{InstanceNames, ObjectSecurity};

/// Covers a running instance that is still between taking the singleton and
/// serving its activation pipe, plus admission into its activation queue.
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct SingleInstanceGuard {
    _mutex: OwnedHandle,
}

pub(super) struct PrimaryInstance {
    pub(super) guard: Option<SingleInstanceGuard>,
    pub(super) owns_activation: bool,
}

pub(super) enum InstanceOutcome {
    Primary(PrimaryInstance),
    SecondaryNotified,
    SecondaryFailed,
}

enum MutexDecision<G, E> {
    Primary {
        guard: Option<G>,
        owns_activation: bool,
    },
    Existing(G),
    Unavailable(E),
}

fn decide_mutex<G, E>(created: Result<(G, bool), E>, bypass_existing: bool) -> MutexDecision<G, E> {
    match created {
        Ok((_guard, true)) if bypass_existing => MutexDecision::Primary {
            guard: None,
            owns_activation: false,
        },
        Ok((guard, true)) => MutexDecision::Existing(guard),
        Ok((guard, false)) => MutexDecision::Primary {
            guard: Some(guard),
            owns_activation: true,
        },
        Err(_error) if bypass_existing => MutexDecision::Primary {
            guard: None,
            owns_activation: false,
        },
        Err(error) => MutexDecision::Unavailable(error),
    }
}

fn current_names() -> anyhow::Result<(InstanceNames, String)> {
    let sid = instance_security::current_user_sid()?;
    let base = crate::app_activation::single_instance_mutex_name_wide();
    Ok((instance_security::instance_names(&base, &sid)?, sid))
}

pub(super) fn acquire(args: &StartupArgs, bypass_existing: bool) -> InstanceOutcome {
    let coordination = current_names();
    let created = coordination
        .as_ref()
        .map_err(|error| anyhow::anyhow!("{error:#}"))
        .and_then(|(names, sid)| {
            let security = ObjectSecurity::for_mutex(sid)?;
            instance_security::create_singleton_mutex(&names.mutex, &security)
                .map(|(mutex, existed)| (SingleInstanceGuard { _mutex: mutex }, existed))
        });
    let _existing_guard = match decide_mutex(created, bypass_existing) {
        MutexDecision::Primary {
            guard,
            owns_activation,
        } => {
            if owns_activation && let Ok((names, sid)) = &coordination {
                serve_activations(names, sid);
            }
            return InstanceOutcome::Primary(PrimaryInstance {
                guard,
                owns_activation,
            });
        }
        MutexDecision::Unavailable(error) => return coordination_unavailable(error),
        MutexDecision::Existing(guard) => guard,
    };
    let Ok((names, _)) = &coordination else {
        return coordination_unavailable(anyhow::anyhow!("singleton names are unavailable"));
    };

    let delivery = args
        .validated_process_with_sgt_file()
        .and_then(|path| instance_channel::encode_request(path.as_deref()))
        .and_then(|request| instance_channel::deliver(&names.pipe, request, HANDOFF_TIMEOUT));
    match delivery {
        Ok(()) => InstanceOutcome::SecondaryNotified,
        Err(error) => {
            crate::log_info!("[Activation] Could not deliver request: {error}");
            report_activation_failure();
            InstanceOutcome::SecondaryFailed
        }
    }
}

/// The owner admits handed-off requests through its own activation queue and
/// restore event, which no other process needs to open.
fn serve_activations(names: &InstanceNames, sid: &str) {
    let started = ObjectSecurity::for_pipe(sid).and_then(|security| {
        instance_channel::start_server(&names.pipe, &security, |request| {
            match crate::app_activation::admit_activation(request.as_deref()) {
                Ok(()) => true,
                Err(error) => {
                    crate::log_info!("[Activation] Could not admit handed-off request: {error}");
                    false
                }
            }
        })
    });
    if let Err(error) = started {
        crate::log_info!("[Activation] Later launches cannot reach this instance: {error:#}");
    }
}

pub(super) fn report_activation_failure() {
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            windows::core::w!(
                "The file or window request could not be confirmed. It may still be processed; check the running app before retrying."
            ),
            windows::core::w!("Screen Goated Toolbox"),
            windows::Win32::UI::WindowsAndMessaging::MB_OK
                | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
}

fn coordination_unavailable(error: anyhow::Error) -> InstanceOutcome {
    crate::log_info!(
        "[Activation] Single-instance mutex unavailable; refusing uncoordinated startup: {error:#}"
    );
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            windows::core::w!(
                "Screen Goated Toolbox could not establish single-instance coordination. It did not start or deliver the request. Close any running copy or resolve the Windows permissions issue, then try again."
            ),
            windows::core::w!("Screen Goated Toolbox"),
            windows::Win32::UI::WindowsAndMessaging::MB_OK
                | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
    InstanceOutcome::SecondaryFailed
}

#[cfg(test)]
mod tests {
    use super::{MutexDecision, decide_mutex};

    #[test]
    fn denied_mutex_stops_normal_startup_without_claiming_delivery() {
        assert!(matches!(
            decide_mutex::<(), _>(Err("access denied"), false),
            MutexDecision::Unavailable("access denied")
        ));
        assert!(matches!(
            decide_mutex::<(), _>(Err("resource unavailable"), false),
            MutexDecision::Unavailable("resource unavailable")
        ));
    }

    #[test]
    fn isolated_bypass_remains_uncoordinated_on_denial_or_existing_mutex() {
        assert!(matches!(
            decide_mutex::<(), _>(Err("access denied"), true),
            MutexDecision::Primary {
                guard: None,
                owns_activation: false
            }
        ));
        assert!(matches!(
            decide_mutex::<_, ()>(Ok(((), true)), true),
            MutexDecision::Primary {
                guard: None,
                owns_activation: false
            }
        ));
    }

    #[test]
    fn a_created_mutex_is_owned_and_an_existing_one_requests_delivery() {
        assert!(matches!(
            decide_mutex::<_, ()>(Ok(((), false)), false),
            MutexDecision::Primary {
                guard: Some(()),
                owns_activation: true
            }
        ));
        assert!(matches!(
            decide_mutex::<_, ()>(Ok(((), true)), false),
            MutexDecision::Existing(())
        ));
    }

    #[test]
    fn existing_mutex_guard_is_retained_through_delivery() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Guard(Rc<Cell<bool>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Rc::new(Cell::new(false));
        {
            let MutexDecision::Existing(_guard) =
                decide_mutex::<_, ()>(Ok((Guard(dropped.clone()), true)), false)
            else {
                panic!("existing mutex should retain its handle");
            };
            assert!(!dropped.get(), "handle must remain open during delivery");
        }
        assert!(dropped.get());
    }
}
