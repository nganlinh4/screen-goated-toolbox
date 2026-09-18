/// Retry an oversized output reservation once at the provider-declared allowance.
/// Ordinary quota exhaustion and unknown error formats keep normal fallback.
pub(crate) fn output_limit_retry(
    status: u16,
    attempt: u8,
    body: &str,
    current: Option<u64>,
) -> Option<u32> {
    if status != 429 || attempt != 0 {
        return None;
    }
    let (kind, limit) = request_allowance(status, body)?;
    (kind == "OTPM" && current.is_none_or(|value| value > u64::from(limit))).then_some(limit)
}

pub(crate) fn failure_prefix(status: u16, body: &str) -> &'static str {
    if request_allowance(status, body).is_some() {
        "PROVIDER_REQUEST_LIMIT:"
    } else {
        ""
    }
}

fn request_allowance(status: u16, body: &str) -> Option<(String, u32)> {
    if status != 429 {
        return None;
    }
    let root: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = root.get("error")?;
    if error["code"] != "rate_limit_exceeded" || error["type"] != "tokens" {
        return None;
    }
    let message = error["message"].as_str()?;
    let (prefix, fields) = message.split_once("): Limit ")?;
    let (_, kind) = prefix.rsplit_once('(')?;
    if !matches!(kind, "OTPM" | "ITPM" | "TPM") {
        return None;
    }
    let (limit, requested) = fields.split_once(", Requested ")?;
    let limit: u32 = limit.parse().ok()?;
    let requested: u64 = requested.split('.').next()?.parse().ok()?;
    (limit > 0 && limit <= i32::MAX as u32 && requested > u64::from(limit))
        .then_some((kind.to_string(), limit))
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_output_allowance_retry_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/groq-output-allowance.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                !super::failure_prefix(
                    case["status"].as_u64().unwrap() as u16,
                    &case["body"].to_string()
                )
                .is_empty(),
                case["request_scoped"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
            let result = super::output_limit_retry(
                case["status"].as_u64().unwrap() as u16,
                case["attempt"].as_u64().unwrap() as u8,
                &case["body"].to_string(),
                case["current"].as_u64(),
            );
            assert_eq!(
                result.map(u64::from),
                case["expected"].as_u64(),
                "{}",
                case["name"]
            );
        }
    }
}
