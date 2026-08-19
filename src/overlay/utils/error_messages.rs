/// Provider key named inside a credential error marker such as `NO_API_KEY:groq`.
fn provider_key_from_error(error: &str) -> Option<&'static str> {
    [
        ("groq", "groq"),
        ("openrouter", "openrouter"),
        ("openai", "openai"),
        ("google", "google"),
        ("gemini", "google"),
    ]
    .into_iter()
    .find_map(|(needle, key)| error.contains(needle).then_some(key))
}

fn api_key_provider_name(error: &str) -> &str {
    provider_key_from_error(error).map_or("API", crate::model_config::provider_full_name)
}

fn api_key_notification_message(error: &str, lang: &str) -> Option<String> {
    let provider = api_key_provider_name(error);

    if error.contains("NO_API_KEY") {
        return Some(match lang {
            "vi" => format!("Bạn chưa nhập {} API key!", provider),
            "ko" => format!("{} API 키를 입력하지 않았습니다!", provider),
            "ja" => format!("{} APIキーが入力されていません!", provider),
            "zh" => format!("您还没有输入 {} API key!", provider),
            _ => format!("You haven't entered a {} API key!", provider),
        });
    }

    if error.contains("INVALID_API_KEY") {
        return Some(match lang {
            "vi" => format!("{} API key không hợp lệ!", provider),
            "ko" => format!("{} API 키가 유효하지 않습니다!", provider),
            "ja" => format!("{} APIキーが無効です!", provider),
            "zh" => format!("{} API key 无效!", provider),
            _ => format!("Invalid {} API key!", provider),
        });
    }

    None
}

pub fn show_api_key_error_notification(error: &str, lang: &str) {
    if let Some(message) = api_key_notification_message(error, lang) {
        crate::overlay::auto_copy_badge::show_error_notification(&message);
    }
}

pub fn get_error_message(error: &str, lang: &str, model_name: Option<&str>) -> String {
    // Parse NO_API_KEY:provider format
    if error.contains("NO_API_KEY") {
        let provider = api_key_provider_name(error);

        return match lang {
            "vi" => format!("Bạn chưa nhập {} API key!", provider),
            "ko" => format!("{} API 키를 입력하지 않았습니다!", provider),
            "ja" => format!("{} APIキーが入力されていません!", provider),
            "zh" => format!("您还没有输入 {} API key!", provider),
            _ => format!("You haven't entered a {} API key!", provider),
        };
    }

    // Parse INVALID_API_KEY:provider format
    if error.contains("INVALID_API_KEY") {
        let provider = api_key_provider_name(error);

        return match lang {
            "vi" => format!("{} API key không hợp lệ!", provider),
            "ko" => format!("{} API 키가 유효하지 않습니다!", provider),
            "ja" => format!("{} APIキーが無効です!", provider),
            "zh" => format!("{} API key 无效!", provider),
            _ => format!("Invalid {} API key!", provider),
        };
    }

    // Parse HTTP status codes from API error messages
    // Example: "Error: https://api.groq.com/openai/v1/chat/completions: status code 429"
    if let Some(status_code) = extract_http_status_code(error) {
        let provider = extract_provider_from_error(error);
        return format_http_error(status_code, &provider, model_name, lang);
    }

    // Fallback for other errors
    match lang {
        "vi" => format!("Lỗi: {}", error),
        "ko" => format!("오류: {}", error),
        "ja" => format!("エラー: {}", error),
        "zh" => format!("错误: {}", error),
        _ => format!("Error: {}", error),
    }
}

/// Extracts HTTP status code from error message.
///
/// Accepts the shapes providers actually emit: `status code 429`, `HTTP 402: ...`,
/// `error 500`, or a bare code delimited by non-alphanumeric characters. The
/// boundary check keeps model names such as `llama-3.3-70b` or `qwen3-235b` from
/// being mistaken for status codes.
fn extract_http_status_code(error: &str) -> Option<u16> {
    const CODE_PREFIXES: [&str; 5] = ["status code ", "http ", "http/1.1 ", "error ", "code "];

    let lower = error.to_ascii_lowercase();
    for prefix in CODE_PREFIXES {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(prefix) {
            let after = search_from + pos + prefix.len();
            search_from = after;
            if let Some(code) = parse_status_code_at(&lower, after) {
                return Some(code);
            }
        }
    }

    first_delimited_status_code(&lower)
}

/// Parses a 3-digit HTTP status code starting at `offset`, rejecting longer digit runs.
fn parse_status_code_at(text: &str, offset: usize) -> Option<u16> {
    let bytes = text.as_bytes();
    let digits: Vec<u8> = bytes[offset..]
        .iter()
        .copied()
        .take_while(u8::is_ascii_digit)
        .collect();
    if digits.len() != 3 {
        return None;
    }
    let code: u16 = std::str::from_utf8(&digits).ok()?.parse().ok()?;
    (400..=599).contains(&code).then_some(code)
}

/// Scans for a standalone 3-digit code that is not embedded in a longer token.
fn first_delimited_status_code(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    for start in 0..bytes.len() {
        if !bytes[start].is_ascii_digit() {
            continue;
        }
        let preceded_by_token = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
        if preceded_by_token {
            continue;
        }
        let end = start + 3;
        if end > bytes.len() || bytes[end..].first().is_some_and(u8::is_ascii_alphanumeric) {
            continue;
        }
        if let Some(code) = parse_status_code_at(text, start) {
            return Some(code);
        }
    }
    None
}

/// Extracts the provider name from an error URL or host fragment.
fn extract_provider_from_error(error: &str) -> String {
    [
        ("api.groq.com", "groq"),
        ("generativelanguage.googleapis.com", "google"),
        ("gemini", "google"),
        ("api.openai.com", "openai"),
        ("openrouter.ai", "openrouter"),
        ("api.anthropic.com", "anthropic"),
        ("claude", "anthropic"),
    ]
    .into_iter()
    .find_map(|(needle, key)| error.contains(needle).then_some(key))
    .map_or_else(
        || "API".to_string(),
        |key| crate::model_config::provider_full_name(key).to_string(),
    )
}

/// Formats HTTP error with localized message
fn format_http_error(
    status_code: u16,
    provider: &str,
    model_name: Option<&str>,
    lang: &str,
) -> String {
    // Format the model/provider info for display
    let model_info = if let Some(model) = model_name {
        format!("{} ({})", model, provider)
    } else {
        provider.to_string()
    };

    match status_code {
        429 => match lang {
            "vi" => format!(
                "Lỗi 429: Đã vượt quá hạn mức của mô hình {} (Rate Limit). Vui lòng chờ một lát rồi thử lại.",
                model_info
            ),
            "ko" => format!(
                "오류 429: {} 모델의 요청 제한 초과 (Rate Limit). 잠시 후 다시 시도해 주세요.",
                model_info
            ),
            "ja" => format!(
                "エラー 429: {} のレート制限を超えました。しばらくしてから再試行してください。",
                model_info
            ),
            "zh" => format!(
                "错误 429: {} 模型请求超出限制 (Rate Limit)。请稍后再试。",
                model_info
            ),
            _ => format!(
                "Error 429: Rate limit exceeded for model {}. Please wait a moment and try again.",
                model_info
            ),
        },
        402 => match lang {
            "vi" => format!(
                "Lỗi 402: Tài khoản {} đã hết hạn mức thanh toán. Vui lòng kiểm tra mục thanh toán của nhà cung cấp.",
                model_info
            ),
            "ko" => format!(
                "오류 402: {} 계정의 결제 한도가 소진되었습니다. 제공업체의 결제 페이지를 확인해 주세요.",
                model_info
            ),
            "ja" => format!(
                "エラー 402: {} のご利用枠が不足しています。プロバイダーの請求ページをご確認ください。",
                model_info
            ),
            "zh" => format!(
                "错误 402: {} 账户额度不足。请检查服务商的账单页面。",
                model_info
            ),
            _ => format!(
                "Error 402: Payment required for {}. Please check your billing page with the provider.",
                model_info
            ),
        },
        400 => match lang {
            "vi" => format!(
                "Lỗi 400: Yêu cầu không hợp lệ đến {}. Vui lòng kiểm tra lại cài đặt.",
                model_info
            ),
            "ko" => format!(
                "오류 400: {}에 대한 잘못된 요청입니다. 설정을 확인해 주세요.",
                model_info
            ),
            "ja" => format!(
                "エラー 400: {} へのリクエストが無効です。設定を確認してください。",
                model_info
            ),
            "zh" => format!("错误 400: {} 请求无效。请检查设置。", model_info),
            _ => format!(
                "Error 400: Bad request to {}. Please check your settings.",
                model_info
            ),
        },
        401 => match lang {
            "vi" => format!(
                "Lỗi 401: API key của {} không hợp lệ hoặc đã hết hạn.",
                provider
            ),
            "ko" => format!(
                "오류 401: {} API 키가 유효하지 않거나 만료되었습니다.",
                provider
            ),
            "ja" => format!(
                "エラー 401: {} の API キーが無効または期限切れです。",
                provider
            ),
            "zh" => format!("错误 401: {} API 密钥无效或已过期。", provider),
            _ => format!("Error 401: {} API key is invalid or expired.", provider),
        },
        403 => match lang {
            "vi" => format!(
                "Lỗi 403: Không có quyền truy cập {}. Vui lòng kiểm tra API key.",
                provider
            ),
            "ko" => format!(
                "오류 403: {}에 대한 접근 권한이 없습니다. API 키를 확인해 주세요.",
                provider
            ),
            "ja" => format!(
                "エラー 403: {} へのアクセス権限がありません。API キーを確認してください。",
                provider
            ),
            "zh" => format!("错误 403: 无权访问 {}。请检查 API 密钥。", provider),
            _ => format!(
                "Error 403: Access forbidden to {}. Please check your API key.",
                provider
            ),
        },
        404 => match lang {
            "vi" => format!(
                "Lỗi 404: Không tìm thấy mô hình {} trên {}.",
                model_name.unwrap_or("này"),
                provider
            ),
            "ko" => format!(
                "오류 404: {}에서 {} 모델을 찾을 수 없습니다.",
                provider,
                model_name.unwrap_or("해당")
            ),
            "ja" => format!(
                "エラー 404: {} で {} が見つかりません。",
                provider,
                model_name.unwrap_or("このモデル")
            ),
            "zh" => format!(
                "错误 404: 在 {} 上找不到模型 {}。",
                provider,
                model_name.unwrap_or("此")
            ),
            _ => format!(
                "Error 404: Model {} not found on {}.",
                model_name.unwrap_or("this"),
                provider
            ),
        },
        500 => match lang {
            "vi" => format!(
                "Lỗi 500: Máy chủ {} gặp lỗi nội bộ. Vui lòng thử lại sau.",
                provider
            ),
            "ko" => format!(
                "오류 500: {} 서버 내부 오류입니다. 나중에 다시 시도해 주세요.",
                provider
            ),
            "ja" => format!(
                "エラー 500: {} サーバー内部エラー。後で再試行してください。",
                provider
            ),
            "zh" => format!("错误 500: {} 服务器内部错误。请稍后再试。", provider),
            _ => format!(
                "Error 500: {} internal server error. Please try again later.",
                provider
            ),
        },
        502 => match lang {
            "vi" => format!(
                "Lỗi 502: Bad Gateway - {} đang gặp sự cố. Vui lòng thử lại sau.",
                provider
            ),
            "ko" => format!(
                "오류 502: Bad Gateway - {}에 문제가 발생했습니다. 나중에 다시 시도해 주세요.",
                provider
            ),
            "ja" => format!(
                "エラー 502: Bad Gateway - {} に問題が発生しています。後で再試行してください。",
                provider
            ),
            "zh" => format!(
                "错误 502: Bad Gateway - {} 遇到问题。请稍后再试。",
                provider
            ),
            _ => format!(
                "Error 502: Bad Gateway - {} is having issues. Please try again later.",
                provider
            ),
        },
        503 => match lang {
            "vi" => format!(
                "Lỗi 503: Dịch vụ {} đang quá tải hoặc bảo trì. Vui lòng thử lại sau.",
                provider
            ),
            "ko" => format!(
                "오류 503: {} 서비스가 과부하 상태이거나 점검 중입니다. 나중에 다시 시도해 주세요.",
                provider
            ),
            "ja" => format!(
                "エラー 503: {} サービスが過負荷またはメンテナンス中です。後で再試行してください。",
                provider
            ),
            "zh" => format!("错误 503: {} 服务过载或维护中。请稍后再试。", provider),
            _ => format!(
                "Error 503: {} service is overloaded or under maintenance. Please try again later.",
                provider
            ),
        },
        504 => match lang {
            "vi" => format!(
                "Lỗi 504: Hết thời gian chờ phản hồi từ {}. Vui lòng thử lại.",
                model_info
            ),
            "ko" => format!(
                "오류 504: {} 응답 시간 초과. 다시 시도해 주세요.",
                model_info
            ),
            "ja" => format!(
                "エラー 504: {} からの応答がタイムアウトしました。再試行してください。",
                model_info
            ),
            "zh" => format!("错误 504: {} 响应超时。请重试。", model_info),
            _ => format!(
                "Error 504: Gateway timeout from {}. Please try again.",
                model_info
            ),
        },
        _ => match lang {
            "vi" => format!(
                "Lỗi {}: Có lỗi xảy ra với {} (HTTP {}).",
                status_code, model_info, status_code
            ),
            "ko" => format!(
                "오류 {}: {}에서 오류가 발생했습니다 (HTTP {}).",
                status_code, model_info, status_code
            ),
            "ja" => format!(
                "エラー {}: {} でエラーが発生しました (HTTP {}).",
                status_code, model_info, status_code
            ),
            "zh" => format!(
                "错误 {}: {} 发生错误 (HTTP {}).",
                status_code, model_info, status_code
            ),
            _ => format!(
                "Error {}: An error occurred with {} (HTTP {}).",
                status_code, model_info, status_code
            ),
        },
    }
}

/// Billing/credit exhaustion markers. These are provider-wide: every model behind
/// the same account fails identically, so retrying siblings only wastes requests.
pub fn is_billing_exhausted_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("payment required")
        || lower.contains("insufficient credit")
        || lower.contains("insufficient_quota")
        || lower.contains("insufficient funds")
        || lower.contains("out of credits")
        || lower.contains("credit balance is too low")
}

pub fn should_advance_retry_chain(error: &str) -> bool {
    if error.contains("NO_API_KEY") || error.contains("INVALID_API_KEY") {
        return true;
    }

    if is_billing_exhausted_error(error) {
        return true;
    }

    if let Some(code) = extract_http_status_code(error) {
        if matches!(
            code,
            400 | 401 | 402 | 403 | 404 | 408 | 409 | 413 | 422 | 425 | 429
        ) {
            return true;
        }
        if (500..=599).contains(&code) {
            return true;
        }
        return false;
    }

    let lower_err = error.to_lowercase();
    if lower_err.contains("rate limit")
        || lower_err.contains("too many requests")
        || lower_err.contains("quota exceeded")
        || lower_err.contains("peer disconnected")
        || lower_err.contains("connection reset")
        || lower_err.contains("connection aborted")
        || lower_err.contains("connection closed")
        || lower_err.contains("broken pipe")
        || lower_err.contains("timed out")
        || lower_err.contains("timeout")
        || lower_err.contains("deadline exceeded")
        || lower_err.contains("not found")
        || lower_err.contains("unsupported")
        || lower_err.contains("not support")
    {
        return true;
    }

    false
}

pub fn should_block_retry_provider(error: &str) -> bool {
    if error.contains("NO_API_KEY")
        || error.contains("INVALID_API_KEY")
        || error.contains("PROVIDER_DISABLED")
        || error.contains("STRUCTURED_OUTPUT_REJECTED")
    {
        return true;
    }

    if is_billing_exhausted_error(error) {
        return true;
    }

    matches!(extract_http_status_code(error), Some(401..=403))
}

#[cfg(test)]
mod tests {
    use super::{
        extract_http_status_code, should_advance_retry_chain, should_block_retry_provider,
    };

    #[test]
    fn extracts_status_codes_from_provider_phrasings() {
        assert_eq!(
            extract_http_status_code("OpenRouter API Error HTTP 402: Payment required"),
            Some(402)
        );
        assert_eq!(
            extract_http_status_code("request failed with status code 429"),
            Some(429)
        );
        assert_eq!(
            extract_http_status_code("upstream returned: 503"),
            Some(503)
        );
        assert_eq!(
            extract_http_status_code("Error 422: unprocessable"),
            Some(422)
        );
    }

    #[test]
    fn ignores_digits_embedded_in_model_names() {
        assert_eq!(extract_http_status_code("model qwen3-235b failed"), None);
        assert_eq!(extract_http_status_code("llama-3.3-70b-versatile"), None);
        assert_eq!(extract_http_status_code("connection reset by peer"), None);
    }

    #[test]
    fn advances_chain_for_billing_and_payload_failures() {
        assert!(should_advance_retry_chain(
            "OpenRouter API Error HTTP 402: Payment required to access this resource. Visit your billing tab."
        ));
        assert!(should_advance_retry_chain("insufficient_quota"));
        assert!(should_advance_retry_chain(
            "request failed with status code 422"
        ));
        assert!(should_advance_retry_chain("deadline exceeded"));
    }

    #[test]
    fn blocks_provider_for_billing_exhaustion() {
        assert!(should_block_retry_provider(
            "OpenRouter API Error HTTP 402: Payment required to access this resource. Visit your billing tab."
        ));
        assert!(should_block_retry_provider("credit balance is too low"));
        assert!(!should_block_retry_provider(
            "Error 429: quota exceeded, please retry"
        ));
    }

    #[test]
    fn advances_chain_for_auth_and_not_found_failures() {
        assert!(should_advance_retry_chain("NO_API_KEY:google"));
        assert!(should_advance_retry_chain("INVALID_API_KEY"));
        assert!(should_advance_retry_chain(
            "request failed with status code 401"
        ));
        assert!(should_advance_retry_chain(
            "request failed with status code 404"
        ));
        assert!(should_advance_retry_chain("unsupported model"));
    }

    #[test]
    fn blocks_provider_for_provider_wide_failures() {
        assert!(should_block_retry_provider("NO_API_KEY:groq"));
        assert!(should_block_retry_provider("INVALID_API_KEY"));
        assert!(should_block_retry_provider("PROVIDER_DISABLED:google"));
        assert!(should_block_retry_provider(
            "STRUCTURED_OUTPUT_REJECTED:google:HTTP 400 INVALID_ARGUMENT"
        ));
        assert!(should_block_retry_provider(
            "request failed with status code 403"
        ));
        assert!(!should_block_retry_provider(
            "request failed with status code 404"
        ));
    }
}
