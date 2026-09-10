use super::policy::{Event, Policy};

fn apply(policy: &mut Policy, event: &Event, text: &mut String, caret: &mut usize) -> usize {
    let writes = if let Some(replacement) = policy.plan(event) {
        let start = *caret - replacement.old.len();
        assert_eq!(&text[start..*caret], replacement.old);
        text.replace_range(start..*caret, &replacement.new);
        *caret = start + replacement.new.len();
        1
    } else {
        0
    };
    policy.accept(event);
    writes
}

#[test]
fn active_provider_response_does_not_require_new_microphone_input() {
    let mut policy = Policy::new(true);
    let mut text = "Note:  suffix".to_owned();
    let mut caret = 6;
    apply(
        &mut policy,
        &Event::Interim("first draft".into()),
        &mut text,
        &mut caret,
    );
    // Microphone inactivity is not an editor event. Provider updates remain
    // valid until an explicit drain/finish or ownership cancellation.
    apply(
        &mut policy,
        &Event::Interim("revised café 🙂".into()),
        &mut text,
        &mut caret,
    );
    assert_eq!(text, "Note: revised café 🙂 suffix");
    apply(
        &mut policy,
        &Event::Final("Final sentence.".into()),
        &mut text,
        &mut caret,
    );
    assert_eq!(text, "Note: Final sentence. suffix");
}

#[test]
fn planning_without_dispatch_acknowledgement_does_not_advance_owned_text() {
    let mut policy = Policy::new(true);
    policy.accept(&Event::Interim("confirmed".into()));
    assert_eq!(
        policy
            .plan(&Event::Interim("not yet written".into()))
            .unwrap()
            .old,
        "confirmed"
    );
    assert_eq!(
        policy.plan(&Event::Final("Final.".into())).unwrap().old,
        "confirmed"
    );
}

#[test]
fn matching_final_has_no_mutation_and_next_segment_starts_at_current_caret() {
    let mut policy = Policy::new(true);
    let mut text = "prefix  suffix".to_owned();
    let mut caret = 7;
    assert_eq!(
        apply(
            &mut policy,
            &Event::Interim("yes".into()),
            &mut text,
            &mut caret
        ),
        1
    );
    assert_eq!(
        apply(
            &mut policy,
            &Event::Final("yes".into()),
            &mut text,
            &mut caret
        ),
        0
    );
    assert_eq!(
        apply(
            &mut policy,
            &Event::Interim(" next".into()),
            &mut text,
            &mut caret
        ),
        1
    );
    assert_eq!(
        apply(
            &mut policy,
            &Event::Final(" next".into()),
            &mut text,
            &mut caret
        ),
        0
    );
    assert_eq!(
        apply(
            &mut policy,
            &Event::Final(" next".into()),
            &mut text,
            &mut caret
        ),
        1
    );
    assert_eq!(text, "prefix yes next next suffix");
}

#[test]
fn finish_removes_only_uncommitted_tail_and_repeated_finish_is_inert() {
    let mut policy = Policy::new(true);
    let mut text = "prefix  suffix".to_owned();
    let mut caret = 7;
    apply(
        &mut policy,
        &Event::Final("Saved.".into()),
        &mut text,
        &mut caret,
    );
    apply(
        &mut policy,
        &Event::Interim(" unfinished".into()),
        &mut text,
        &mut caret,
    );
    assert_eq!(apply(&mut policy, &Event::Finish, &mut text, &mut caret), 1);
    assert_eq!(apply(&mut policy, &Event::Finish, &mut text, &mut caret), 0);
    assert_eq!(text, "prefix Saved. suffix");
}
