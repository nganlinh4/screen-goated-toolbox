use super::*;

#[test]
fn value_only_native_controls_keep_editing_capability_without_inventing_a_value() {
    let mut control = UiElement {
        name: String::new(),
        automation_id: "field".into(),
        runtime_id: vec![1],
        control_type: "ComboBox",
        left: 0,
        top: 0,
        right: 20,
        bottom: 20,
        enabled: true,
        protected: false,
        state: None,
        value: None.into(),
        required: true,
    };
    control.value.supported = true;
    control.value.writable = true;
    control.value.incomplete = true;
    let identity = SurfaceIdentity::Native {
        hwnd: 1,
        pid: 2,
        generation: 3,
    };
    let world = build_world(&[control.clone()], identity.clone());
    assert_eq!(world.elements.len(), 1);
    assert!(world.elements[0].editable);
    assert!(world.elements[0].value.is_none());
    assert!(world.to_model_text().contains("required·unobserved"));
    assert!(!world.to_model_text().contains("required·empty"));
    control.value.text = Some("bounded preview".into());
    control.value.writable = false;
    let world = build_world(&[control], identity);
    assert!(!world.elements[0].editable);
    assert!(world.elements[0].value.is_none());
    assert_eq!(
        world.elements[0].value_preview.as_deref(),
        Some("bounded preview")
    );
    assert!(world.to_model_text().contains("[text preview]"));
}

#[test]
fn unsupported_activation_keeps_zero_effect_evidence() {
    let failure = native_activation_failure(uia::ActivationError::test_failure(
        uia::FailureKind::Unsupported,
        false,
    ));
    assert_eq!(failure.0["code"], "ERR_NATIVE_ACTIVATION_UNSUPPORTED");
    assert_eq!(failure.0["effect_may_have_occurred"], false);

    let transported: anyhow::Error = failure.into();
    let typed = typed_value(&transported).expect("typed native failure");
    assert_eq!(typed["dispatch_ok"], false);
    assert_eq!(typed["effect_may_have_occurred"], false);
}

#[test]
fn failed_input_receipt_survives_semantic_action_transport() {
    let error = checked_execution(json!({
        "ok": false,
        "error": "input stopped",
        "started_utf16_units": 2,
        "effect_may_have_occurred": true,
        "retryable": false,
        "input_injection": {"inserted": 4},
    }))
    .unwrap_err();
    let receipt = typed_value(&error).unwrap();
    assert_eq!(receipt["started_utf16_units"], 2);
    assert_eq!(receipt["input_injection"]["inserted"], 4);
    assert_eq!(receipt["effect_may_have_occurred"], true);
    assert_eq!(receipt["retryable"], false);

    let unknown = checked_execution(json!({"ok": false, "error": "failed"})).unwrap_err();
    assert_eq!(
        typed_value(&unknown).unwrap()["effect_may_have_occurred"],
        true
    );

    let none = checked_execution(json!({
        "ok": false,
        "effect_may_have_occurred": false,
        "cancelled": true,
    }))
    .unwrap_err();
    let after_focus = typed_value(&after_prior_native_input(none)).unwrap();
    assert_eq!(after_focus["prior_input_dispatched"], true);
    assert_eq!(after_focus["effect_may_have_occurred"], true);
    assert_eq!(after_focus["retryable"], false);
}

#[test]
fn partial_native_fill_is_preserved_by_single_and_sequence_actions() {
    struct PartialFillSurface {
        world: WorldState,
        executions: usize,
    }

    impl Surface for PartialFillSurface {
        fn identity(&mut self) -> Result<SurfaceIdentity> {
            Ok(self.world.identity.clone())
        }

        fn observe(&mut self) -> Result<WorldState> {
            Ok(self.world.clone())
        }

        fn execute(
            &mut self,
            _: &IndexedElement,
            verb: Verb,
            _: Option<&str>,
            _: &ActCtx,
            _: &SurfaceIdentity,
        ) -> Result<Value> {
            assert_eq!(verb, Verb::Fill);
            self.executions += 1;
            checked_execution(json!({
                "ok": false,
                "error": "input interrupted",
                "started_utf16_units": 2,
                "typed_partial": true,
                "effect_may_have_occurred": true,
                "retryable": false,
                "input_injection": {"inserted": 4},
            }))
            .map_err(after_prior_native_input)
        }

        fn read_back(&mut self, _: &IndexedElement, _: &SurfaceIdentity) -> ReadBack {
            panic!("failed input must not be verified as a completed fill")
        }
    }

    let world = WorldState {
        identity: SurfaceIdentity::Native {
            hwnd: 9,
            pid: 4,
            generation: 1,
        },
        elements: vec![IndexedElement {
            id: 1,
            role: "textbox".into(),
            name: "field".into(),
            value: Some(String::new()),
            value_preview: None,
            editable: true,
            protected: false,
            state: None,
            enabled: true,
            required: false,
            submit: false,
            form: None,
            risk: None,
            handle: ElHandle::Native {
                cx: 1,
                cy: 1,
                provider_name: "field".into(),
                automation_id: "field-id".into(),
                runtime_id: vec![1],
            },
        }],
        title: None,
        url: None,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let profile = super::super::super::human_input::HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    for sequence in [false, true] {
        let mut surface = PartialFillSurface {
            world: world.clone(),
            executions: 0,
        };
        let mut controller = super::super::Controller {
            last: Some(world.clone()),
            ..Default::default()
        };
        let result = if sequence {
            controller.do_steps_on(
                &[
                    json!({"id": 1, "verb": "fill", "value": "first"}),
                    json!({"id": 1, "verb": "fill", "value": "second"}),
                ],
                &act,
                &mut surface,
            )
        } else {
            controller.act_on(1, "fill", Some("first"), false, &act, &mut surface)
        };
        assert_eq!(surface.executions, 1);
        assert_eq!(result["ok"], false);
        assert_eq!(result["effect_may_have_occurred"], true);
        let receipt = if sequence {
            &result["receipts"][0]
        } else {
            &result
        };
        assert_eq!(receipt["started_utf16_units"], 2);
        assert_eq!(receipt["typed_partial"], true);
        assert_eq!(receipt["input_injection"]["inserted"], 4);
        assert_eq!(receipt["prior_input_dispatched"], true);
        assert_eq!(receipt["retryable"], false);
    }
}

#[test]
fn activation_failure_typing_preserves_stage_and_effect_boundary() {
    for (kind, code) in [
        (uia::FailureKind::Setup, "ERR_NATIVE_ACTIVATION_SETUP"),
        (
            uia::FailureKind::TargetQuery,
            "ERR_NATIVE_ACTIVATION_TARGET_QUERY",
        ),
        (
            uia::FailureKind::CapabilityQuery,
            "ERR_NATIVE_ACTIVATION_CAPABILITY_QUERY",
        ),
        (
            uia::FailureKind::Cancelled,
            "ERR_NATIVE_ACTIVATION_CANCELLED",
        ),
        (uia::FailureKind::StaleTarget, "ERR_NATIVE_STALE_TARGET"),
    ] {
        let failure = native_activation_failure(uia::ActivationError::test_failure(kind, false));
        assert_eq!(failure.0["code"], code);
        assert_eq!(failure.0["effect_may_have_occurred"], false);
    }

    let dispatched = native_activation_failure(uia::ActivationError::test_failure(
        uia::FailureKind::Dispatch,
        true,
    ));
    assert_eq!(dispatched.0["code"], "ERR_NATIVE_ACTIVATION_FAILED");
    assert_eq!(dispatched.0["effect_may_have_occurred"], true);
}

#[test]
fn dry_activation_never_foregrounds_a_window() {
    assert!(!activation_may_focus(Verb::Activate, true));
    assert!(activation_may_focus(Verb::Activate, false));
}

#[test]
fn native_select_is_rejected_before_execution() {
    let element = IndexedElement {
        id: 1,
        role: "combobox".to_string(),
        name: "choice".to_string(),
        value: None,
        value_preview: None,
        editable: false,
        protected: false,
        state: None,
        enabled: true,
        required: false,
        submit: false,
        form: None,
        risk: None,
        handle: ElHandle::Native {
            cx: 10,
            cy: 10,
            provider_name: "choice".to_string(),
            automation_id: "choice-id".to_string(),
            runtime_id: vec![1, 10],
        },
    };

    assert!(
        NativeSurface::new(None)
            .validate(&element, Verb::Select, Some("option"))
            .is_err()
    );
}

#[test]
fn fill_keyboard_guard_retains_exact_clicked_element_identity() {
    let identity = SurfaceIdentity::Native {
        hwnd: 41,
        pid: 73,
        generation: 5,
    };
    let args = guarded_args(
        json!({"text": "replacement"}),
        &identity,
        Some((
            12,
            34,
            uia::ExpectedNativeElement {
                role: "textbox",
                provider_name: "Query",
                automation_id: "query-input",
                runtime_id: &[42, 7],
            },
        )),
    );
    let element = &args["expected_input_target"]["element"];
    assert_eq!(element["screen_px"], json!([12, 34]));
    assert_eq!(element["role"], "textbox");
    assert_eq!(element["provider_name"], "Query");
    assert_eq!(element["automation_id"], "query-input");
    assert_eq!(element["runtime_id"], json!([42, 7]));
}
