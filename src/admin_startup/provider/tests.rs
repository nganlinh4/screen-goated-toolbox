use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

mod unc_paths;

const CURRENT: &str = r"C:\Protected Folder\app.exe";
const PRIOR: &str = r"C:\Previous Folder\app.exe";

#[derive(Default)]
struct State {
    snapshot: Snapshot,
    writes: usize,
    fail_read: bool,
    fail_next_read: bool,
    fail_write: Option<(usize, bool)>,
    drift_after_write: Option<(usize, Snapshot)>,
}

#[derive(Default)]
struct Memory(Mutex<State>);
impl Memory {
    fn snapshot(&self) -> Snapshot {
        self.0.lock().unwrap().snapshot.clone()
    }
    fn set(&self, value: Snapshot) {
        self.0.lock().unwrap().snapshot = value;
    }
    fn writes(&self) -> usize {
        self.0.lock().unwrap().writes
    }
}
impl Registry for Memory {
    fn read(&self) -> Result<Snapshot> {
        let mut state = self.0.lock().unwrap();
        if std::mem::take(&mut state.fail_next_read) {
            anyhow::bail!("Transient registry read failure");
        }
        ensure!(!state.fail_read, "Registry access denied");
        Ok(state.snapshot.clone())
    }
    fn write(&self, slot: Slot, value: &Option<RawValue>) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        state.writes += 1;
        let failure = state.fail_write.filter(|(at, _)| *at == state.writes);
        ensure!(
            failure != Some((state.writes, false)),
            "Registry write denied"
        );
        slot.set(&mut state.snapshot, value.clone());
        if let Some((at, drift)) = state.drift_after_write.clone()
            && at == state.writes
        {
            state.snapshot = drift;
        }
        ensure!(
            failure.is_none(),
            "Registry write failed after applying its value"
        );
        Ok(())
    }
}

fn provider(snapshot: Snapshot) -> (CurrentUserStartup, Arc<Memory>) {
    let memory = Arc::new(Memory(Mutex::new(State {
        snapshot,
        ..State::default()
    })));
    let provider = CurrentUserStartup {
        path: PathBuf::from(CURRENT),
        authorized: PathBuf::from(PRIOR),
        registry: memory.clone(),
    };
    (provider, memory)
}

fn legacy(path: &str) -> RawValue {
    let bytes = format!("{path} {}\0", crate::startup_launch::AUTOSTART_ARG)
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    RawValue {
        kind: REG_SZ,
        bytes,
    }
}

fn disabled() -> RawValue {
    let mut value = enabled_approval();
    value.bytes[0] = 3;
    value.bytes[4..].copy_from_slice(&123_456_u64.to_le_bytes());
    value
}

#[test]
fn new_commands_quote_native_unicode_and_unpaired_surrogate_paths() {
    let mut units = r"C:\개인 폴더\app & %!.exe"
        .encode_utf16()
        .collect::<Vec<_>>();
    units.insert(4, 0xD800);
    let path = PathBuf::from(OsString::from_wide(&units));
    let value = run_value(&path).unwrap();
    assert_eq!(value.bytes[..2], [34, 0]);
    assert_eq!(run_target(&value).unwrap(), path);
    assert_eq!(value.bytes[value.bytes.len() - 2..], [0, 0]);
}

#[test]
fn outgoing_run_limits_include_quotes_and_arguments_but_not_the_terminator() {
    let overhead = 3 + crate::startup_launch::AUTOSTART_ARG.encode_utf16().count();
    let limit = MAX_RUN_COMMAND_UNITS - overhead;
    for suffix in [".exe", "😀.exe"] {
        for extra in [0, 1] {
            let path = format!(
                r"C:\{}{suffix}",
                "a".repeat(limit - 3 - suffix.encode_utf16().count() + extra)
            );
            let result = run_value(Path::new(&path));
            if extra == 0 {
                assert_eq!(result.unwrap().bytes.len(), (MAX_RUN_COMMAND_UNITS + 1) * 2);
            } else {
                assert_eq!(
                    result.unwrap_err().downcast_ref::<super::super::Issue>(),
                    Some(&super::super::Issue::PathTooLong)
                );
            }
        }
    }
}

#[test]
fn overlong_new_run_commands_cannot_write_or_reach_the_task_operation() {
    let (mut provider, memory) = provider(Snapshot::default());
    provider.path = PathBuf::from(format!(r"C:\{}.exe", "a".repeat(260)));
    let called = AtomicBool::new(false);
    let error = normal(&provider, || {
        called.store(true, Ordering::SeqCst);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<super::super::Issue>(),
        Some(&super::super::Issue::PathTooLong)
    );
    assert_eq!(memory.writes(), 0);
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(memory.snapshot(), Snapshot::default());
}

#[test]
fn oversized_legacy_commands_are_inactive_but_can_be_retired_and_restored_exactly() {
    let path = format!(r"C:\{}.exe", "a".repeat(300));
    let original = Snapshot {
        run: Some(legacy(&path)),
        approved: Some(enabled_approval()),
    };
    let (mut provider, memory) = provider(original.clone());
    provider.authorized = PathBuf::from(path);
    assert!(!provider.is_enabled().unwrap());
    let error = elevated(&provider, || {
        assert!(memory.snapshot().run.is_none());
        Err(anyhow!("Task preparation rejected"))
    })
    .unwrap_err();
    assert!(error.downcast_ref::<UncertainElevation>().is_none());
    assert_eq!(memory.snapshot(), original);
}

#[test]
fn failed_elevation_restores_exact_prior_path_raw_bytes_and_disabled_approval() {
    let original = Snapshot {
        run: Some(legacy(PRIOR)),
        approved: Some(disabled()),
    };
    let (provider, memory) = provider(original.clone());
    let called = AtomicBool::new(false);
    assert!(
        elevated(&provider, || {
            called.store(true, Ordering::Release);
            assert!(memory.snapshot().run.is_none());
            anyhow::bail!("Protected elevated target was rejected")
        })
        .is_err()
    );
    assert!(called.load(Ordering::Acquire));
    assert_eq!(memory.snapshot(), original);
}

#[test]
fn failed_elevation_does_not_create_a_previously_absent_run_entry() {
    for approved in [None, Some(disabled())] {
        let original = Snapshot {
            run: None,
            approved,
        };
        let (provider, memory) = provider(original.clone());
        assert!(elevated(&provider, || anyhow::bail!("Registration failed")).is_err());
        assert_eq!(memory.snapshot(), original);
        assert_eq!(memory.writes(), 0);
    }
}

#[test]
fn ordinary_disable_failure_never_calls_elevated_mutation_and_restores_partial_write() {
    for after_write in [false, true] {
        let original = Snapshot {
            run: Some(legacy(PRIOR)),
            approved: None,
        };
        let (provider, memory) = provider(original.clone());
        memory.0.lock().unwrap().fail_write = Some((1, after_write));
        let called = AtomicBool::new(false);
        assert!(
            elevated(&provider, || {
                called.store(true, Ordering::Release);
                Ok(())
            })
            .is_err()
        );
        assert!(!called.load(Ordering::Acquire));
        assert_eq!(memory.snapshot(), original);
    }
}

#[test]
fn failed_approval_write_rolls_back_run_before_retiring_any_elevated_task() {
    let original = Snapshot {
        run: Some(legacy(PRIOR)),
        approved: Some(disabled()),
    };
    let (provider, memory) = provider(original.clone());
    memory.0.lock().unwrap().fail_write = Some((2, false));
    let called = AtomicBool::new(false);
    assert!(
        normal(&provider, || {
            called.store(true, Ordering::Release);
            Ok(())
        })
        .is_err()
    );
    assert!(!called.load(Ordering::Acquire));
    assert_eq!(memory.snapshot(), original);
}

#[test]
fn failed_task_retirement_restores_absent_disabled_and_other_copy_normal_states() {
    for original in [
        Snapshot::default(),
        Snapshot {
            run: Some(legacy(PRIOR)),
            approved: None,
        },
        Snapshot {
            run: Some(legacy(PRIOR)),
            approved: Some(disabled()),
        },
    ] {
        let (provider, memory) = provider(original.clone());
        assert!(
            normal(&provider, || {
                let written = memory.snapshot();
                assert_eq!(
                    run_target(written.run.as_ref().unwrap()).unwrap(),
                    Path::new(CURRENT)
                );
                assert!(approved_enabled(written.approved.as_ref()).unwrap());
                anyhow::bail!("Elevated registration could not be retired")
            })
            .is_err()
        );
        assert_eq!(memory.snapshot(), original);
    }
}

#[test]
fn successful_switches_verify_the_ordinary_provider_before_scheduler_effects() {
    let (provider, memory) = provider(Snapshot::default());
    normal(&provider, || {
        let written = memory.snapshot();
        assert_eq!(
            run_target(written.run.as_ref().unwrap()).unwrap(),
            Path::new(CURRENT)
        );
        assert!(approved_enabled(written.approved.as_ref()).unwrap());
        Ok(())
    })
    .unwrap();
    elevated(&provider, || {
        assert!(memory.snapshot().run.is_none());
        Ok(())
    })
    .unwrap();
    assert!(memory.snapshot().run.is_none());
}

#[test]
fn unknown_malformed_or_foreign_values_are_preserved_without_scheduler_effects() {
    let owned = legacy(PRIOR);
    let mut invalid = Vec::new();
    invalid.push(Snapshot {
        run: Some(legacy(r"C:\Other\program.exe")),
        approved: None,
    });
    for command in [
        format!("{CURRENT} --other-action\0"),
        format!(
            "{CURRENT} {} --extra\0",
            crate::startup_launch::AUTOSTART_ARG
        ),
        format!("{CURRENT} {}", crate::startup_launch::AUTOSTART_ARG),
        format!("{CURRENT}\0 {}\0", crate::startup_launch::AUTOSTART_ARG),
    ] {
        invalid.push(Snapshot {
            run: Some(RawValue {
                kind: REG_SZ,
                bytes: command.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            }),
            approved: None,
        });
    }
    let mut wrong_type = owned.clone();
    wrong_type.kind = 2;
    invalid.push(Snapshot {
        run: Some(wrong_type),
        approved: None,
    });
    let mut odd = owned.clone();
    odd.bytes.pop();
    invalid.push(Snapshot {
        run: Some(odd),
        approved: None,
    });
    for approval in [
        RawValue {
            kind: REG_BINARY,
            bytes: vec![0; 12],
        },
        RawValue {
            kind: REG_BINARY,
            bytes: vec![2; 13],
        },
        RawValue {
            kind: REG_SZ,
            bytes: vec![0; 12],
        },
    ] {
        invalid.push(Snapshot {
            run: Some(owned.clone()),
            approved: Some(approval),
        });
    }
    for original in invalid {
        let (provider, memory) = provider(original.clone());
        assert!(provider.is_enabled().is_err());
        assert!(
            provider
                .capture()
                .and_then(|state| provider.enable_from(&state))
                .is_err()
        );
        assert!(
            provider
                .capture()
                .and_then(|state| provider.disable_from(&state))
                .is_err()
        );
        let called = AtomicBool::new(false);
        assert!(
            elevated(&provider, || {
                called.store(true, Ordering::Release);
                Ok(())
            })
            .is_err()
        );
        assert!(
            normal(&provider, || {
                called.store(true, Ordering::Release);
                Ok(())
            })
            .is_err()
        );
        assert!(!called.load(Ordering::Acquire));
        assert_eq!(memory.writes(), 0);
        assert_eq!(memory.snapshot(), original);
    }
}

#[test]
fn registry_read_errors_do_not_mean_the_provider_is_absent() {
    let (provider, memory) = provider(Snapshot::default());
    memory.0.lock().unwrap().fail_read = true;
    assert!(provider.is_enabled().is_err());
    assert!(
        provider
            .capture()
            .and_then(|state| provider.enable_from(&state))
            .is_err()
    );
    assert!(
        provider
            .capture()
            .and_then(|state| provider.disable_from(&state))
            .is_err()
    );
    assert_eq!(memory.writes(), 0);
}

#[test]
fn concurrent_registry_drift_is_not_overwritten_by_failure_rollback() {
    let original = Snapshot {
        run: Some(legacy(PRIOR)),
        approved: Some(disabled()),
    };
    let (provider, memory) = provider(original);
    let foreign = Snapshot {
        run: Some(legacy(r"C:\Other\program.exe")),
        approved: None,
    };
    assert!(
        normal(&provider, || {
            memory.set(foreign.clone());
            anyhow::bail!("Task changed during transition")
        })
        .is_err()
    );
    assert_eq!(memory.snapshot(), foreign);
    assert_eq!(memory.writes(), 2);
}

#[test]
fn concurrent_drift_during_registry_write_prevents_scheduler_mutation() {
    let (provider, memory) = provider(Snapshot::default());
    let foreign = Snapshot {
        run: Some(legacy(r"C:\Other\program.exe")),
        approved: None,
    };
    memory.0.lock().unwrap().drift_after_write = Some((1, foreign.clone()));
    let called = AtomicBool::new(false);
    assert!(
        normal(&provider, || {
            called.store(true, Ordering::Release);
            Ok(())
        })
        .is_err()
    );
    assert!(!called.load(Ordering::Acquire));
    assert_eq!(memory.snapshot(), foreign);
    assert_eq!(memory.writes(), 1);
}

#[test]
fn registry_drift_during_a_successful_task_action_is_reported_without_overwriting_it() {
    for elevated_mode in [false, true] {
        let original = Snapshot {
            run: Some(legacy(PRIOR)),
            approved: Some(enabled_approval()),
        };
        let (provider, memory) = provider(original);
        let drift = Snapshot {
            run: Some(legacy(CURRENT)),
            approved: Some(disabled()),
        };
        let called = AtomicBool::new(false);
        let action = || {
            called.store(true, Ordering::Release);
            memory.set(drift.clone());
            Ok(())
        };
        let result = if elevated_mode {
            elevated(&provider, action)
        } else {
            normal(&provider, action)
        };
        assert!(result.is_err());
        assert!(called.load(Ordering::Acquire));
        assert_eq!(memory.snapshot(), drift);
        assert_eq!(memory.writes(), 1);
    }
}

#[test]
fn uncertain_elevation_keeps_the_previously_enabled_ordinary_provider_suspended() {
    let original = Snapshot {
        run: Some(legacy(PRIOR)),
        approved: Some(enabled_approval()),
    };
    let (provider, memory) = provider(original.clone());
    let error = elevated(&provider, || {
        assert!(memory.snapshot().run.is_none());
        Err(uncertain(anyhow!("The task service reply was lost")))
    })
    .unwrap_err();
    assert!(error.downcast_ref::<UncertainElevation>().is_some());
    assert_eq!(
        memory.snapshot(),
        Snapshot {
            run: None,
            approved: original.approved
        }
    );
    assert_eq!(memory.writes(), 1);
}

#[test]
fn uncertain_normal_retirement_restores_the_exact_previous_ordinary_state() {
    for original in [
        Snapshot::default(),
        Snapshot {
            run: Some(legacy(PRIOR)),
            approved: Some(enabled_approval()),
        },
        Snapshot {
            run: Some(legacy(PRIOR)),
            approved: Some(disabled()),
        },
    ] {
        let (provider, memory) = provider(original.clone());
        let error = normal(&provider, || {
            let staged = memory.snapshot();
            assert_eq!(
                run_target(staged.run.as_ref().unwrap()).unwrap(),
                Path::new(CURRENT)
            );
            assert!(approved_enabled(staged.approved.as_ref()).unwrap());
            Err(uncertain(anyhow!("Task retirement could not be confirmed")))
        })
        .unwrap_err();
        assert!(error.downcast_ref::<UncertainElevation>().is_some());
        assert_eq!(memory.snapshot(), original);
    }
}

#[test]
fn recovered_post_apply_read_failure_does_not_restore_a_second_startup_provider() {
    let original = Snapshot {
        run: Some(legacy(PRIOR)),
        approved: Some(enabled_approval()),
    };
    let (provider, memory) = provider(original.clone());
    let error = elevated(&provider, || {
        assert!(memory.snapshot().run.is_none());
        memory.0.lock().unwrap().fail_next_read = true;
        Ok(())
    })
    .unwrap_err();
    assert!(error.downcast_ref::<UncertainElevation>().is_some());
    assert_eq!(
        provider.capture().unwrap(),
        Snapshot {
            run: None,
            approved: original.approved
        }
    );
    assert!(!memory.0.lock().unwrap().fail_next_read);
    assert_eq!(memory.writes(), 1);
}
