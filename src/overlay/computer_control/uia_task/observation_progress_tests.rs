use super::*;
use crate::overlay::computer_control::controller::world::SurfaceIdentity;

#[test]
fn failed_empty_and_changed_surface_observations_cannot_prove_unchanged_state() {
    let surface = SurfaceIdentity::Native {
        hwnd: 1,
        pid: 2,
        generation: 3,
    };
    let replacement = SurfaceIdentity::Native {
        hwnd: 1,
        pid: 2,
        generation: 4,
    };
    let signature = super::super::state_signature::scoped_signature;
    assert!(signature(&surface, "same", false).is_none());
    assert!(signature(&surface, "", true).is_none());
    assert_ne!(
        signature(&surface, "same", true),
        signature(&replacement, "same", true)
    );
    assert_eq!(
        signature(&surface, "same", true),
        signature(&surface, "same", true)
    );
}

#[test]
fn visual_comparison_distinguishes_change_from_missing_or_incompatible_samples() {
    let before = vec![0; 576];
    assert_eq!(visual_unchanged(Some(&before), &before, 10), Some(true));
    assert_eq!(
        visual_unchanged(Some(&before), &vec![255; 576], 10),
        Some(false)
    );
    assert_eq!(visual_unchanged(None, &before, 10), None);
    assert_eq!(visual_unchanged(Some(&[]), &[], 10), None);
    assert_eq!(visual_unchanged(Some(&before), &[0; 12], 10), None);
}

#[test]
fn pixel_progress_never_latches_a_no_effect_or_advice_condition() {
    let mut recent = Vec::new();
    for _ in 0..20 {
        let (_, stuck) = record_observed_action(
            &mut recent,
            "click_at",
            &json!({"x":5,"y":5}),
            true,
            Some(false),
            false,
        );
        assert!(!stuck);
        assert!(recent.is_empty());
    }
}

#[test]
fn only_repeated_observed_unchanged_actions_count_and_unknown_resets_the_chain() {
    let args = json!({"x":5,"y":5});
    for (observed, visual, exempt) in [
        (false, Some(true), false),
        (true, None, false),
        (true, Some(false), false),
        (true, Some(true), true),
    ] {
        let mut recent = Vec::new();
        assert!(!record_observed_action(&mut recent, "click_at", &args, true, Some(true), false).1);
        assert!(!record_observed_action(&mut recent, "click_at", &args, true, Some(true), false).1);
        assert!(
            !record_observed_action(&mut recent, "click_at", &args, observed, visual, exempt).1
        );
        assert!(recent.is_empty());
        assert!(!record_observed_action(&mut recent, "click_at", &args, true, Some(true), false).1);
        assert!(!record_observed_action(&mut recent, "click_at", &args, true, Some(true), false).1);
        assert!(record_observed_action(&mut recent, "click_at", &args, true, Some(true), false).1);
    }
}
