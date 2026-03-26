pub fn get_error_message(error: &str, lang: &str, model_name: Option<&str>) -> String {
    // Parse NO_API_KEY:provider format
    if error.starts_with("NO_API_KEY") {
        let provider = if error.contains(':') {
            let parts: Vec<&str> = error.split(':').collect();
            if parts.len() > 1 {
                match parts[1] {
                    "groq" => "Groq",
                    "google" => "Google Gemini",
                    "openai" => "OpenAI",
                    other => other,
                }
            } else {
                "API"
            }
        } else {
            "API"
        };

        return match lang {
            "vi" => format!("Bạn chưa nhập {} API key!", provider),
            "ko" => format!("{} API 키를 입력하지 않았습니다!", provider),
            "ja" => format!("{} APIキーが入力されていません!", provider),
            "zh" => format!("您还没有输入 {} API key!", provider),
            _ => format!("You haven't entered a {} API key!", provider),
        };
    }

    // Parse INVALID_API_KEY:provider format
    if error.starts_with("INVALID_API_KEY") {
        let provider = if error.contains(':') {
            let parts: Vec<&str> = error.split(':').collect();
            if parts.len() > 1 {
                match parts[1] {
                    "groq" => "Groq",
                    "google" => "Google Gemini",
                    "openai" => "OpenAI",
                    other => other,
                }
            } else {
                "API"
            }
        } else {
            "API"
        };

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

/// Extracts HTTP status code from error message
fn extract_http_status_code(error: &str) -> Option<u16> {
    // Pattern: "status code XXX" or just a 3-digit code at the end
    if let Some(pos) = error.find("status code ") {
        let after = &error[pos + 12..];
        let code_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        return code_str.parse().ok();
    }

    // Also check for patterns like ": 429" at the end
    let trimmed = error.trim();
    if trimmed.len() >= 3 {
        let last_3: String = trimmed
            .chars()
            .rev()
            .take(3)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if last_3.chars().all(|c| c.is_ascii_digit())
            && let Ok(code) = last_3.parse::<u16>()
            && (400..=599).contains(&code)
        {
            return Some(code);
        }
    }

    // Check for "XXX" anywhere (common error codes)
    [429, 400, 401, 403, 404, 500, 502, 503, 504]
        .into_iter()
        .find(|&code| error.contains(&code.to_string()))
}

/// Extracts provider name from error URL
fn extract_provider_from_error(error: &str) -> String {
    if error.contains("api.groq.com") {
        "Groq".to_string()
    } else if error.contains("generativelanguage.googleapis.com") || error.contains("gemini") {
        "Google Gemini".to_string()
    } else if error.contains("api.openai.com") {
        "OpenAI".to_string()
    } else if error.contains("api.anthropic.com") || error.contains("claude") {
        "Anthropic".to_string()
    } else {
        "API".to_string()
    }
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

pub fn should_advance_retry_chain(error: &str) -> bool {
    if error.contains("NO_API_KEY") || error.contains("INVALID_API_KEY") {
        return true;
    }

    if let Some(code) = extract_http_status_code(error) {
        if matches!(code, 400 | 401 | 403 | 404 | 429) {
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
        || lower_err.contains("broken pipe")
        || lower_err.contains("timed out")
        || lower_err.contains("timeout")
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
    {
        return true;
    }

    matches!(extract_http_status_code(error), Some(401 | 403))
}

#[cfg(test)]
mod tests {
    use super::{should_advance_retry_chain, should_block_retry_provider};

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
    fn blocks_provider_for_auth_failures_only() {
        assert!(should_block_retry_provider("NO_API_KEY:groq"));
        assert!(should_block_retry_provider("INVALID_API_KEY"));
        assert!(should_block_retry_provider("PROVIDER_DISABLED:google"));
        assert!(should_block_retry_provider(
            "request failed with status code 403"
        ));
        assert!(!should_block_retry_provider(
            "request failed with status code 404"
        ));
    }
}
