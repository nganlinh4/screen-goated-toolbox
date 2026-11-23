use super::*;
use std::cell::{Cell, RefCell};

struct Window {
    owner: u32,
    after_owner: u32,
    root: usize,
    visible: bool,
    cloaked: Option<u32>,
    pid_reads: Cell<u8>,
    queries: RefCell<Vec<&'static str>>,
}

impl Window {
    fn available() -> Self {
        Self {
            owner: 20,
            after_owner: 20,
            root: 10,
            visible: true,
            cloaked: Some(0),
            pid_reads: Cell::new(0),
            queries: RefCell::new(Vec::new()),
        }
    }
}

impl Probe for Window {
    fn pid(&self, _: usize) -> u32 {
        self.queries.borrow_mut().push("pid");
        let count = self.pid_reads.get();
        self.pid_reads.set(count + 1);
        if count == 0 {
            self.owner
        } else {
            self.after_owner
        }
    }
    fn root(&self, _: usize) -> usize {
        self.queries.borrow_mut().push("root");
        self.root
    }
    fn visible(&self, _: usize) -> bool {
        self.queries.borrow_mut().push("visible");
        self.visible
    }
    fn cloaked(&self, _: usize) -> Option<u32> {
        self.queries.borrow_mut().push("cloaked");
        self.cloaked
    }
}

#[test]
fn enumeration_and_stable_identity_share_external_top_level_eligibility() {
    for expected in [None, Some(20)] {
        let window = Window::available();
        assert_eq!(inspect_with(10, expected, 99, &window), Ok(20));
        assert_eq!(
            *window.queries.borrow(),
            ["pid", "root", "visible", "cloaked", "pid"]
        );
    }
}

#[test]
fn own_process_is_rejected_before_any_further_window_query() {
    for expected in [None, Some(20)] {
        let window = Window::available();
        assert_eq!(
            inspect_with(10, expected, 20, &window),
            Err(WindowError::IneligibleTarget {
                hwnd: 10,
                reason: "own_process",
            })
        );
        assert_eq!(*window.queries.borrow(), ["pid"]);
    }
}

#[test]
fn child_or_unresolved_root_is_rejected_in_both_resolution_routes() {
    for root in [0, 11] {
        for expected in [None, Some(20)] {
            let window = Window {
                root,
                ..Window::available()
            };
            assert_eq!(
                inspect_with(10, expected, 99, &window),
                Err(WindowError::IneligibleTarget {
                    hwnd: 10,
                    reason: "not_top_level",
                })
            );
            assert_eq!(*window.queries.borrow(), ["pid", "root"]);
        }
    }
}

#[test]
fn hidden_cloaked_and_unproven_visibility_never_admit_a_target() {
    for expected in [None, Some(20)] {
        let hidden = Window {
            visible: false,
            ..Window::available()
        };
        assert_eq!(
            inspect_with(10, expected, 99, &hidden),
            Err(WindowError::IneligibleTarget {
                hwnd: 10,
                reason: "hidden",
            })
        );
        assert!(!hidden.queries.borrow().contains(&"cloaked"));
        for cloaked in [Some(1), Some(2), Some(4), Some(7), Some(0x8000_0000), None] {
            let window = Window {
                cloaked,
                ..Window::available()
            };
            assert_eq!(
                inspect_with(10, expected, 99, &window),
                Err(WindowError::IneligibleTarget {
                    hwnd: 10,
                    reason: if cloaked.is_some() {
                        "cloaked"
                    } else {
                        "visibility_unavailable"
                    },
                })
            );
        }
    }
}

#[test]
fn mismatched_missing_or_changed_owners_cannot_become_stable_targets() {
    for owner in [0, 21] {
        let window = Window {
            owner,
            ..Window::available()
        };
        assert_eq!(
            inspect_with(10, Some(20), 99, &window),
            Err(WindowError::StaleStableTarget {
                hwnd: 10,
                expected_pid: 20,
                actual_pid: owner,
            })
        );
        assert_eq!(*window.queries.borrow(), ["pid"]);
    }
    let missing = Window {
        owner: 0,
        ..Window::available()
    };
    assert_eq!(
        inspect_with(10, None, 99, &missing),
        Err(WindowError::IneligibleTarget {
            hwnd: 10,
            reason: "owner_unavailable",
        })
    );
    for after_owner in [0, 21] {
        for expected in [None, Some(20)] {
            let window = Window {
                after_owner,
                ..Window::available()
            };
            assert_eq!(
                inspect_with(10, expected, 99, &window),
                Err(WindowError::StaleStableTarget {
                    hwnd: 10,
                    expected_pid: 20,
                    actual_pid: after_owner,
                })
            );
        }
    }
}

#[test]
fn native_invalid_handle_is_stale_without_enumerating_or_acting_on_windows() {
    assert_eq!(
        inspect(HWND::default(), Some(20)),
        Err(WindowError::StaleStableTarget {
            hwnd: 0,
            expected_pid: 20,
            actual_pid: 0,
        })
    );
}
