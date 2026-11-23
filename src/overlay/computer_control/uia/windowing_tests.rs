use super::{WindowCandidate, WindowError, parse_stable_target, resolve_named_candidate};
use windows::Win32::Foundation::HWND;

fn candidate(raw: usize, pid: u32, title: &str, exe: &str) -> WindowCandidate {
    WindowCandidate {
        hwnd: HWND(raw as *mut _),
        pid,
        title: title.to_string(),
        exe: exe.to_string(),
    }
}

#[test]
fn title_substrings_are_not_window_targets() {
    let candidates = [candidate(1, 10, "Notes 10", "host.exe")];
    assert!(matches!(
        resolve_named_candidate("Notes 1", &candidates),
        Err(WindowError::NotFound { .. })
    ));
}

#[test]
fn ambiguous_exact_titles_fail_with_stable_choices() {
    let candidates = [
        candidate(1, 10, "Notes", "host-a.exe"),
        candidate(2, 11, "Notes", "host-b.exe"),
    ];
    let error = resolve_named_candidate(" notes ", &candidates).unwrap_err();
    assert_eq!(
        error,
        WindowError::Ambiguous {
            target: " notes ".to_string(),
            matches: vec!["@hwnd:1:10".to_string(), "@hwnd:2:11".to_string()],
        }
    );
}

#[test]
fn exact_executable_stem_resolves_uniquely() {
    let candidates = [
        candidate(1, 10, "Document", "editor.exe"),
        candidate(2, 11, "Editor guide", "browser.exe"),
    ];
    assert_eq!(
        resolve_named_candidate("ＥＤＩＴＯＲ", &candidates)
            .unwrap()
            .hwnd
            .0 as usize,
        1
    );
}

#[test]
fn stable_target_requires_both_nonzero_identity_parts() {
    assert_eq!(
        parse_stable_target("@hwnd:123:77").unwrap(),
        Some((123, 77))
    );
    for invalid in ["@hwnd:123", "@hwnd:0:77", "@hwnd:123:0", "@hwnd:x:77"] {
        assert!(matches!(
            parse_stable_target(invalid),
            Err(WindowError::InvalidStableTarget { .. })
        ));
    }
}
