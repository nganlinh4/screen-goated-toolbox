use super::{Owner, owner_matches};

#[test]
fn selector_rejects_retired_session_engine_renderer_and_operation() {
    let owner = Owner {
        id: 1,
        session: 2,
        engine: 3,
        renderer: 4,
    };
    assert!(owner_matches(Some(owner), owner, 2, 3, false, true));
    assert!(!owner_matches(Some(owner), owner, 5, 3, false, true));
    assert!(!owner_matches(Some(owner), owner, 2, 6, false, true));
    assert!(!owner_matches(Some(owner), owner, 2, 3, true, true));
    assert!(!owner_matches(Some(owner), owner, 2, 3, false, false));
    assert!(!owner_matches(
        Some(Owner { id: 7, ..owner }),
        owner,
        2,
        3,
        false,
        true
    ));
}
