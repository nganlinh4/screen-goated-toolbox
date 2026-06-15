// --- MEDIA SERVER REQUEST GATE ---
// Per-process security gate for the local media HTTP server: token minting,
// constant-time token comparison, query-param extraction, and the per-request
// token + loopback-Host check applied before any filesystem work.

use super::MEDIA_SERVER_TOKEN;

/// Mints a 256-bit cryptographically-strong token as a lowercase hex string.
/// Used as the per-process secret gate for the local media HTTP server.
pub(super) fn mint_media_server_token() -> String {
    let mut bytes = [0u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        // Extremely unlikely on Windows; fall back to a time+pid seed so the
        // server still starts (degraded, but never panics). Still far better
        // than an unguarded server.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            ^ (std::process::id() as u128);
        let seed_bytes = seed.to_le_bytes();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = seed_bytes[i % seed_bytes.len()].wrapping_add(i as u8);
        }
    }
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

pub(super) struct GateRejection {
    pub(super) body: &'static str,
}

/// Returns the value of the named query parameter from a raw URL, percent-decoded.
pub(super) fn query_param(url: &str, name: &str) -> Option<String> {
    let qs = url.split_once('?').map(|(_, q)| q)?;
    qs.split('&').find_map(|kv| {
        let raw = kv.strip_prefix(name)?.strip_prefix('=')?;
        Some(urlencoding::decode(raw).unwrap_or_default().into_owned())
    })
}

/// Constant-time-ish comparison of two tokens (length-checked, byte-folded) to
/// avoid trivially leaking the token length / prefix via early-exit timing.
fn tokens_match(provided: &str, expected: &str) -> bool {
    let a = provided.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validates the per-request security gate before any filesystem work:
/// 1. the secret token must match (from `X-SGT-Token` header OR `token=` query
///    param — the GET media path uses the param form because `<video>`/`<audio>`
///    `src` cannot send headers);
/// 2. the `Host` header (if present) must be a loopback host (DNS-rebinding
///    defense). Requests without a Host header are allowed through the Host check
///    because the token alone is sufficient and some clients omit it on loopback.
pub(super) fn check_request_gate(request: &tiny_http::Request) -> Result<(), GateRejection> {
    let expected = match MEDIA_SERVER_TOKEN.get() {
        Some(token) => token.as_str(),
        // Token not yet minted: fail closed.
        None => {
            return Err(GateRejection {
                body: "Forbidden",
            });
        }
    };

    let header_value = |name: &str| -> Option<String> {
        request
            .headers()
            .iter()
            .find(|h| h.field.to_string().eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str().to_owned())
    };

    // Host gate (defense-in-depth vs DNS rebinding). Strip any :port suffix.
    if let Some(host) = header_value("host") {
        let host_only = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(&host);
        let host_only = host_only.trim_matches(|c| c == '[' || c == ']'); // IPv6 brackets
        let is_loopback = host_only.eq_ignore_ascii_case("127.0.0.1")
            || host_only.eq_ignore_ascii_case("localhost")
            || host_only == "::1";
        if !is_loopback {
            return Err(GateRejection {
                body: "Forbidden host",
            });
        }
    }

    // Token gate: accept from header or query param.
    let provided = header_value("x-sgt-token")
        .or_else(|| query_param(request.url(), "token"))
        .unwrap_or_default();
    if !tokens_match(&provided, expected) {
        return Err(GateRejection {
            body: "Forbidden",
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{mint_media_server_token, query_param, tokens_match};

    #[test]
    fn tokens_match_accepts_identical() {
        assert!(tokens_match("abc123", "abc123"));
        assert!(tokens_match("", ""));
    }

    #[test]
    fn tokens_match_rejects_length_and_single_bit() {
        assert!(!tokens_match("abc", "abcd")); // length mismatch
        assert!(!tokens_match("", "x"));
        assert!(!tokens_match("abc123", "abc124")); // single-char difference
    }

    #[test]
    fn query_param_extracts_and_percent_decodes() {
        assert_eq!(
            query_param("/m?path=a%2Fb&token=xy", "token").as_deref(),
            Some("xy")
        );
        assert_eq!(
            query_param("/m?path=a%2Fb&token=xy", "path").as_deref(),
            Some("a/b")
        );
    }

    #[test]
    fn query_param_handles_missing_and_no_query() {
        assert_eq!(query_param("/m?token=xy", "nope"), None);
        assert_eq!(query_param("/m", "token"), None);
    }

    #[test]
    fn query_param_not_fooled_by_prefix_sharing_key() {
        // A key that merely starts with the name (e.g. `tokenfoo`) must not satisfy
        // a lookup for `token`; the explicit `=` after the name guards this.
        assert_eq!(
            query_param("/m?tokenfoo=bad&token=good", "token").as_deref(),
            Some("good")
        );
    }

    #[test]
    fn minted_token_is_64_lowercase_hex_chars() {
        let t = mint_media_server_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }
}
