use super::*;

#[test]
fn shared_completion_contract() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/gemini-completion.json"
    )))
    .unwrap();
    for kind in ["stream", "unary"] {
        for case in fixture[kind].as_array().unwrap() {
            let result = if kind == "stream" {
                consume(
                    std::io::Cursor::new(case["body"].as_str().unwrap()),
                    &None,
                    |_, _| {},
                )
            } else {
                parse(&case["body"])
            };
            match result {
                Ok(value) => assert_eq!(
                    Some(value.content.as_str()),
                    case["output"].as_str(),
                    "{}",
                    case["name"]
                ),
                Err(error) => {
                    let error = error.to_string();
                    assert!(
                        error.contains(case["error"].as_str().unwrap()),
                        "{}: {error}",
                        case["name"]
                    );
                    assert!(crate::overlay::utils::should_advance_retry_chain(&error));
                }
            }
        }
    }
}

#[test]
fn cancelled_response_is_never_committed() {
    let token = Some(Arc::new(AtomicBool::new(true)));
    let result = consume(std::io::Cursor::new(""), &token, |_, _| {});
    assert_eq!(result.err().unwrap().to_string(), "Cancelled");
}
