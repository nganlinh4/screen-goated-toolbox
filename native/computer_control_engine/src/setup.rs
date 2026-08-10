use std::collections::HashSet;

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use sgt_computer_control_protocol::{HostPrivilege, SetupPlan, SetupRequest};

const PROMPT_CORE: &str =
    include_str!("../../../src/overlay/computer_control/uia_task/prompt_core.txt");
const TOOL_CATALOG: &str =
    include_str!("../../../src/overlay/computer_control/phone_control_catalog.json");
const PLATFORM_DEVICE_TOKEN: &str = "{{PLATFORM_DEVICE}}";
const STATIC_TOOL_COUNT: usize = 62;
const CONTROLLER_RULES: &str = "ROUTING: highest-fidelity evidence. Accessible: observe, then act on current @id. Pixel-only: vision targets/marks. Prefer direct browser/system/file/integration providers. Raw input needs known focus/effect. Change route after typed failure.";
const SESSION_RULES: &str = "Interpret communicative intent, not grammatical form. If the requested outcome is too uncertain to choose an effect safely, ask one concise clarification and do not act.";

pub(super) fn build(request: SetupRequest) -> Result<SetupPlan> {
    let static_declarations = static_declarations()?;
    let mut names =
        HashSet::with_capacity(static_declarations.len() + request.integration_declarations.len());
    for declaration in &static_declarations {
        names.insert(declaration_name(declaration)?.to_string());
    }
    let mut declarations = static_declarations;
    for declaration in request.integration_declarations {
        let name = declaration_name(&declaration)?;
        if !names.insert(name.to_string()) {
            bail!("Computer Control setup contains duplicate tool {name}");
        }
        validate_declaration(&declaration)?;
        declarations.push(declaration);
    }

    let mut tools = Vec::with_capacity(2);
    if request.search_enabled {
        tools.push(json!({"googleSearch": {}}));
    }
    tools.push(json!({"functionDeclarations": declarations}));
    let realtime_input_config = request.voice_mode.then(|| {
        json!({
            "automaticActivityDetection": {
                "startOfSpeechSensitivity": "START_SENSITIVITY_HIGH",
                "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH",
                "prefixPaddingMs": 30,
                "silenceDurationMs": 250
            },
            "activityHandling": "START_OF_ACTIVITY_INTERRUPTS"
        })
    });
    Ok(SetupPlan {
        system_instruction: system_instruction(request.privilege)?,
        tools: Value::Array(tools),
        realtime_input_config,
    })
}

fn static_declarations() -> Result<Vec<Value>> {
    let catalog: Value = serde_json::from_str(TOOL_CATALOG)?;
    if catalog.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        bail!("unsupported Computer Control tool catalog schema");
    }
    let declarations = catalog
        .get("functionDeclarations")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| anyhow!("Computer Control tool catalog is missing declarations"))?;
    if declarations.len() != STATIC_TOOL_COUNT {
        bail!("Computer Control tool catalog declaration count drifted");
    }
    for declaration in &declarations {
        validate_declaration(declaration)?;
    }
    Ok(declarations)
}

fn system_instruction(privilege: HostPrivilege) -> Result<String> {
    if PROMPT_CORE.matches(PLATFORM_DEVICE_TOKEN).count() != 1 {
        bail!("Computer Control prompt platform token drifted");
    }
    let core = PROMPT_CORE.replace(PLATFORM_DEVICE_TOKEN, "Windows computer");
    let privilege = match privilege {
        HostPrivilege::Elevated => {
            "PRIVILEGE: you are running ELEVATED (full administrator) - run_command has admin rights, so do system tasks directly."
        }
        HostPrivilege::Standard => {
            "PRIVILEGE: you are running as a STANDARD user (not elevated). run_command still does most things; but admin-only tasks fail with Access Denied. Request the required consequential elevation at its effect boundary, then verify."
        }
    };
    Ok(format!(
        "{core}\n{CONTROLLER_RULES}\n{SESSION_RULES}\n{privilege}"
    ))
}

fn declaration_name(declaration: &Value) -> Result<&str> {
    declaration
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 128
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
        .ok_or_else(|| anyhow!("Computer Control tool declaration has an invalid name"))
}

fn validate_declaration(declaration: &Value) -> Result<()> {
    declaration_name(declaration)?;
    let object = declaration
        .as_object()
        .ok_or_else(|| anyhow!("Computer Control tool declaration is not an object"))?;
    if !object
        .get("description")
        .and_then(Value::as_str)
        .is_some_and(|description| !description.is_empty() && description.len() <= 8 * 1024)
    {
        bail!("Computer Control tool declaration has an invalid description");
    }
    if !object.get("parameters").is_some_and(Value::is_object) {
        bail!("Computer Control tool declaration has invalid parameters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SetupRequest {
        SetupRequest {
            privilege: HostPrivilege::Standard,
            voice_mode: true,
            search_enabled: true,
            integration_declarations: Vec::new(),
        }
    }

    #[test]
    fn normal_setup_has_full_unique_catalog_and_voice_policy() {
        let plan = build(request()).unwrap();
        let declarations = plan.tools[1]["functionDeclarations"].as_array().unwrap();
        assert_eq!(declarations.len(), STATIC_TOOL_COUNT);
        let names = declarations
            .iter()
            .map(|declaration| declaration["name"].as_str().unwrap())
            .collect::<HashSet<_>>();
        assert_eq!(names.len(), STATIC_TOOL_COUNT);
        assert!(plan.realtime_input_config.is_some());
        assert!(plan.system_instruction.contains("Windows computer"));
    }

    #[test]
    fn integrations_extend_instead_of_replacing_the_catalog() {
        let mut request = request();
        request.integration_declarations.push(json!({
            "name": "future_integration_tool",
            "description": "Future capability",
            "parameters": {"type": "object"}
        }));
        let plan = build(request).unwrap();
        let declarations = plan.tools[1]["functionDeclarations"].as_array().unwrap();
        assert_eq!(declarations.len(), STATIC_TOOL_COUNT + 1);
    }

    #[test]
    fn duplicate_integration_names_fail_closed() {
        let mut request = request();
        request.integration_declarations.push(json!({
            "name": "observe",
            "description": "Collision",
            "parameters": {"type": "object"}
        }));
        assert!(build(request).is_err());
    }

    #[test]
    fn setup_variants_match_the_shared_protocol_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/computer-control-engine/protocol-v1.json"
        ))
        .unwrap();
        for case in fixture["setupCases"].as_array().unwrap() {
            let request: SetupRequest = serde_json::from_value(case["input"].clone()).unwrap();
            let expected = &case["expected"];
            let dynamic_count = expected["dynamicToolNames"].as_array().unwrap().len();
            let plan = build(request).unwrap();
            let declarations = plan
                .tools
                .as_array()
                .unwrap()
                .iter()
                .find_map(|tool| tool.get("functionDeclarations"))
                .and_then(Value::as_array)
                .unwrap();
            assert_eq!(
                declarations.len(),
                expected["staticToolCount"].as_u64().unwrap() as usize + dynamic_count,
                "case {}",
                case["id"]
            );
            assert_eq!(
                plan.tools
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool.get("googleSearch").is_some()),
                expected["searchEnabled"].as_bool().unwrap()
            );
            assert_eq!(
                plan.realtime_input_config.is_some(),
                expected["voiceMode"].as_bool().unwrap()
            );
            assert!(
                plan.system_instruction
                    .contains(expected["privilegeMarker"].as_str().unwrap())
            );
            for name in expected["dynamicToolNames"].as_array().unwrap() {
                assert!(declarations.iter().any(|declaration| {
                    declaration.get("name").and_then(Value::as_str) == name.as_str()
                }));
            }
        }
    }
}
