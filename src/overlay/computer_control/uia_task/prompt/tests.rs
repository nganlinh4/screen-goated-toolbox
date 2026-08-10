use std::collections::HashSet;

use serde_json::{Value, json};
use sgt_computer_control_protocol::{HostPrivilege, SetupPlan};

const TEST_CATALOG: &str = include_str!("../../phone_control_catalog.json");
const TEST_PROMPT: &str = include_str!("../prompt_core.txt");
const PLATFORM_DEVICE_TOKEN: &str = "{{PLATFORM_DEVICE}}";
const CONTROLLER_RULES: &str = "ROUTING: highest-fidelity evidence. Accessible: observe, then act on current @id. Pixel-only: vision targets/marks. Prefer direct browser/system/file/integration providers. Raw input needs known focus/effect. Change route after typed failure.";
const SESSION_RULES: &str = "Interpret communicative intent, not grammatical form. If the requested outcome is too uncertain to choose an effect safely, ask one concise clarification and do not act.";

fn test_plan(
    privilege: HostPrivilege,
    voice: bool,
    search: bool,
    integration_declarations: &[Value],
) -> SetupPlan {
    let catalog: Value = serde_json::from_str(TEST_CATALOG).unwrap();
    let mut declarations = catalog["functionDeclarations"].as_array().unwrap().clone();
    declarations.extend_from_slice(integration_declarations);
    let mut tools = Vec::new();
    if search {
        tools.push(json!({"googleSearch": {}}));
    }
    tools.push(json!({"functionDeclarations": declarations}));
    let core = TEST_PROMPT.replace(PLATFORM_DEVICE_TOKEN, "Windows computer");
    let privilege_text = match privilege {
        HostPrivilege::Elevated => {
            "PRIVILEGE: you are running ELEVATED (full administrator) - run_command has admin rights, so do system tasks directly."
        }
        HostPrivilege::Standard => {
            "PRIVILEGE: you are running as a STANDARD user (not elevated). run_command still does most things; but admin-only tasks fail with Access Denied. Request the required consequential elevation at its effect boundary, then verify."
        }
    };
    SetupPlan {
        system_instruction: format!(
            "{core}\n{CONTROLLER_RULES}\n{SESSION_RULES}\n{privilege_text}"
        ),
        tools: Value::Array(tools),
        realtime_input_config: super::expected_realtime_input_config(voice),
    }
}

fn build_setup(resume: Option<&str>, voice: bool, search: bool) -> Value {
    build_setup_with_context(resume, voice, search, None)
}

fn build_setup_with_context(
    resume: Option<&str>,
    voice: bool,
    search: bool,
    reconnect_context: Option<&str>,
) -> Value {
    build_setup_with_declarations(resume, voice, search, reconnect_context, Vec::new())
}

fn build_setup_with_declarations(
    resume: Option<&str>,
    voice: bool,
    search: bool,
    reconnect_context: Option<&str>,
    integration_declarations: Vec<Value>,
) -> Value {
    let privilege = if super::executor::is_elevated() {
        HostPrivilege::Elevated
    } else {
        HostPrivilege::Standard
    };
    super::assemble_setup(
        test_plan(privilege, voice, search, &integration_declarations),
        resume,
        voice,
        search,
        reconnect_context,
        privilege,
        &integration_declarations,
    )
    .unwrap()
}

fn declarations(setup: &Value) -> &[Value] {
    setup["setup"]["tools"]
        .as_array()
        .and_then(|tools| {
            tools
                .iter()
                .find_map(|tool| tool.get("functionDeclarations"))
        })
        .and_then(Value::as_array)
        .expect("function declarations")
}

#[test]
fn canonical_setup_matches_shared_worker_fixture() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/computer-control-engine/protocol-v1.json"
    )))
    .unwrap();
    for case in fixture["setupCases"].as_array().unwrap() {
        let input = &case["input"];
        let expected = &case["expected"];
        let integration_declarations = input["integrationDeclarations"].as_array().unwrap().clone();
        let setup = build_setup_with_declarations(
            None,
            input["voiceMode"].as_bool().unwrap(),
            input["searchEnabled"].as_bool().unwrap(),
            None,
            integration_declarations,
        );
        let declarations = declarations(&setup);
        assert_eq!(
            declarations.len(),
            expected["staticToolCount"].as_u64().unwrap() as usize
                + expected["dynamicToolNames"].as_array().unwrap().len()
        );
        assert_eq!(
            setup["setup"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| { tool.get("googleSearch").is_some() }),
            expected["searchEnabled"].as_bool().unwrap()
        );
        assert_eq!(
            setup["setup"].get("realtimeInputConfig").is_some(),
            expected["voiceMode"].as_bool().unwrap()
        );
        for name in expected["dynamicToolNames"].as_array().unwrap() {
            assert!(declarations.iter().any(|declaration| {
                declaration.get("name").and_then(Value::as_str) == name.as_str()
            }));
        }
        let instruction = setup["setup"]["systemInstruction"].to_string();
        let privilege_marker = if super::executor::is_elevated() {
            "ELEVATED"
        } else {
            "STANDARD user"
        };
        assert!(instruction.contains(privilege_marker));
    }
}

#[test]
fn host_rejects_worker_policy_or_catalog_changes() {
    let privilege = HostPrivilege::Standard;
    let mut plan = test_plan(privilege, true, true, &[]);
    plan.tools[1]["functionDeclarations"][0]["description"] = json!("changed");
    assert!(super::validate_plan(&plan, true, true, privilege, &[]).is_err());

    let mut plan = test_plan(privilege, true, true, &[]);
    plan.system_instruction.push('!');
    assert!(super::validate_plan(&plan, true, true, privilege, &[]).is_err());

    let plan = test_plan(privilege, true, true, &[]);
    assert!(super::validate_plan(&plan, false, true, privilege, &[]).is_err());
    assert!(super::validate_plan(&plan, true, false, privilege, &[]).is_err());

    let dynamic = json!({
        "name": "future_integration_tool",
        "description": "Future capability",
        "parameters": {"type": "object"}
    });
    let plan = test_plan(privilege, false, false, std::slice::from_ref(&dynamic));
    assert!(super::validate_plan(&plan, false, false, privilege, &[]).is_err());
}

#[test]
fn setup_catalog_has_unique_named_tools() {
    let setup = build_setup(None, false, false);
    let declarations = declarations(&setup);
    let mut names = HashSet::new();
    for declaration in declarations {
        let name = declaration["name"].as_str().expect("tool name");
        assert!(names.insert(name), "duplicate tool declaration: {name}");
        assert!(
            declaration["description"]
                .as_str()
                .is_some_and(|d| !d.trim().is_empty()),
            "missing description: {name}"
        );
    }
    eprintln!(
        "setup profile: {} tools, {} system bytes, {} declaration bytes, {} total bytes",
        declarations.len(),
        setup["setup"]["systemInstruction"].to_string().len(),
        serde_json::to_string(declarations).unwrap().len(),
        setup.to_string().len()
    );
    assert_eq!(
        declarations.len(),
        62,
        "built-in capability was added or lost"
    );
    assert!(
        serde_json::to_string(declarations).unwrap().len() <= 22_500,
        "function catalog exceeded its reviewed prompt budget"
    );
    assert!(
        setup["setup"]["systemInstruction"].to_string().len() <= 5_250,
        "system instruction exceeded its reviewed prompt budget"
    );
    assert!(
        setup.to_string().len() <= 42_000,
        "base Live setup exceeded its reviewed prompt budget"
    );
}

#[test]
fn canonical_catalog_exactly_drives_static_setup_declarations() {
    let setup = build_setup_with_declarations(None, false, true, None, Vec::new());
    let catalog: Value = serde_json::from_str(TEST_CATALOG).expect("canonical catalog");
    let tools = setup["setup"]["tools"].as_array().expect("setup tools");

    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0], serde_json::json!({"googleSearch": {}}));
    assert_eq!(
        declarations(&setup),
        catalog["functionDeclarations"]
            .as_array()
            .expect("canonical declarations")
    );
}

#[test]
fn canonical_prompt_core_is_platform_parameterized_once() {
    assert_eq!(TEST_PROMPT.matches(PLATFORM_DEVICE_TOKEN).count(), 1);
    let setup = build_setup(None, false, false);
    let instruction = setup["setup"]["systemInstruction"]["parts"][0]["text"]
        .as_str()
        .expect("system instruction");

    assert!(instruction.starts_with("Operate the user's Windows computer."));
    assert!(!instruction.contains(PLATFORM_DEVICE_TOKEN));
}

#[test]
fn exact_tab_close_requires_a_tab_id() {
    let setup = build_setup(None, false, false);
    let close = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "browser_close_tab")
        .expect("browser_close_tab declaration");
    assert_eq!(
        close["parameters"]["required"],
        serde_json::json!(["tab_id"])
    );
    assert_eq!(
        close["parameters"]["properties"]["tab_id"]["type"],
        "integer"
    );
}

#[test]
fn raw_keyboard_tools_require_stable_window_targets() {
    let setup = build_setup(None, false, false);
    for name in ["type_text", "key_combination"] {
        let declaration = declarations(&setup)
            .iter()
            .find(|declaration| declaration["name"] == name)
            .unwrap();
        assert!(
            declaration["parameters"]["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("target"))
        );
    }
}

#[test]
fn new_tabs_expose_structural_lifetime_with_a_persistent_default() {
    let setup = build_setup(None, false, false);
    let open = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "browser_open_tab")
        .expect("browser_open_tab declaration");
    assert_eq!(open["parameters"]["required"], serde_json::json!(["url"]));
    assert_eq!(
        open["parameters"]["properties"]["lifetime"]["enum"],
        serde_json::json!(["turn", "persistent"])
    );
}

#[test]
fn navigation_requires_an_explicit_structural_lifetime() {
    let setup = build_setup(None, false, false);
    let navigate = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "browser_navigate")
        .expect("browser_navigate declaration");
    assert_eq!(
        navigate["parameters"]["required"],
        serde_json::json!(["url", "lifetime"])
    );
    assert_eq!(
        navigate["parameters"]["properties"]["lifetime"]["enum"],
        serde_json::json!(["turn", "persistent"])
    );
}

#[test]
fn exact_text_edit_requires_hash_and_counted_replacements() {
    let setup = build_setup(None, false, false);
    let edit = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "edit_text_file")
        .expect("edit_text_file declaration");
    assert_eq!(
        edit["parameters"]["required"],
        serde_json::json!(["path", "expected_sha256", "replacements"])
    );
    assert_eq!(
        edit["parameters"]["properties"]["replacements"]["items"]["required"],
        serde_json::json!(["old_text", "new_text", "expected_count"])
    );
    assert_eq!(
        edit["parameters"]["properties"]["replacements"]["minItems"],
        1
    );
    assert!(
        edit["parameters"]["properties"]["expected_sha256"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("read_text_file"))
    );
    assert!(
        edit["parameters"]["properties"]
            .get("structural_change_token")
            .is_none()
    );
    let structural = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "edit_text_file_structure")
        .expect("edit_text_file_structure declaration");
    assert_eq!(
        structural["parameters"]["required"],
        serde_json::json!(["path", "expected_sha256", "replacements"])
    );
    assert!(structural["parameters"]["properties"]["structural_change_token"].is_object());
}

#[test]
fn terminal_summary_is_bounded() {
    let setup = build_setup(None, false, false);
    let done = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "done")
        .expect("done declaration");
    assert_eq!(
        done["parameters"]["properties"]["summary"]["maxLength"],
        320
    );
}

#[test]
fn steering_corrections_preserve_unmodified_verified_facts() {
    let setup = build_setup(None, false, false);
    let instruction = setup["setup"]["systemInstruction"].to_string();
    assert!(instruction.contains("Corrections preserve all other verified facts and constraints"));
}

#[test]
fn reconnect_history_is_setup_context_not_a_synthetic_user_turn() {
    let setup = build_setup_with_context(
        None,
        false,
        false,
        Some("User: continue the prior subject\nAssistant: fallible earlier claim"),
    );
    let instruction = setup["setup"]["systemInstruction"].to_string();
    assert!(instruction.contains("RECONNECTED SESSION HISTORY"));
    assert!(instruction.contains("continue the prior subject"));
    assert!(!setup.to_string().contains("realtimeInput"));
}

#[test]
fn requested_source_identity_and_literal_deliverable_fields_stay_explicit() {
    let setup = build_setup(None, false, false);
    let instruction = setup["setup"]["systemInstruction"].to_string();
    assert!(instruction.contains("including official/first-party"));
    assert!(instruction.contains("requested links/IDs literally"));
    assert!(instruction.contains("receipt-proven effects"));
}

#[test]
fn mutations_require_a_turn_local_baseline_for_protected_current_work() {
    let setup = build_setup(None, false, false);
    let instruction = setup["setup"]["systemInstruction"].to_string();
    assert!(instruction.contains("record its exact baseline this turn"));
    assert!(instruction.contains("Another reference is not a baseline"));
}

#[test]
fn directory_listing_distinguishes_metadata_from_content_coverage() {
    let setup = build_setup(None, false, false);
    let list = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "list_files")
        .expect("list_files declaration");
    let description = list["description"].as_str().unwrap();
    assert!(description.contains("names/metadata"));
    assert!(description.contains("read each in-scope file"));
}

#[test]
fn research_can_request_a_structural_domain_boundary() {
    let setup = build_setup(None, false, false);
    let research = declarations(&setup)
        .iter()
        .find(|declaration| declaration["name"] == "research_web")
        .expect("research_web declaration");
    assert!(
        research["parameters"]["properties"]["source_policy"]["enum"]
            .as_array()
            .is_some_and(|values| values.contains(&serde_json::json!("domain_restricted")))
    );
    assert_eq!(
        research["parameters"]["properties"]["allowed_domains"]["items"]["type"],
        "string"
    );
    assert_eq!(
        research["parameters"]["properties"]["source_urls"]["items"]["type"],
        "string"
    );
    assert!(
        research["parameters"]["required"]
            .as_array()
            .is_some_and(|fields| fields.contains(&serde_json::json!("purpose")))
    );
}

#[test]
fn search_fallback_keeps_the_complete_integration_catalog() {
    let declaration = serde_json::json!({
        "name": "future_integration_tool",
        "description": "Future connected provider capability.",
        "parameters": {"type": "object", "properties": {}}
    });
    let with_search =
        build_setup_with_declarations(None, false, true, None, vec![declaration.clone()]);
    let without_search = build_setup_with_declarations(None, false, false, None, vec![declaration]);

    assert!(
        with_search["setup"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool.get("googleSearch").is_some()))
    );
    assert!(
        without_search["setup"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().all(|tool| tool.get("googleSearch").is_none()))
    );
    for setup in [&with_search, &without_search] {
        assert!(
            declarations(setup)
                .iter()
                .any(|item| item["name"] == "future_integration_tool")
        );
    }
}
