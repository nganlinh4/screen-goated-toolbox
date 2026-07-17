//! Capability adapter for browser-backed search discovery.
//!
//! Research consumes ordinary HTTP(S) candidate URLs. Provider-specific search
//! endpoints and redirect transports stay behind this boundary so generic source
//! selection never relies on path substrings or a site's page semantics.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use url::Url;

pub(super) struct SearchProvider {
    name: &'static str,
    endpoint: &'static str,
    query_parameter: &'static str,
    transport_host: &'static str,
    redirect_path: &'static str,
    redirect_parameters: &'static [&'static str],
    encoded_redirect: Option<EncodedRedirect>,
}

#[derive(Clone, Copy)]
struct EncodedRedirect {
    parameter: &'static str,
    prefix: &'static str,
}

const DEFAULT_PROVIDER: SearchProvider = SearchProvider {
    name: "primary",
    endpoint: "https://www.google.com/search",
    query_parameter: "q",
    transport_host: "www.google.com",
    redirect_path: "/url",
    redirect_parameters: &["q", "url"],
    encoded_redirect: None,
};

const FALLBACK_PROVIDER: SearchProvider = SearchProvider {
    name: "fallback",
    endpoint: "https://www.bing.com/search",
    query_parameter: "q",
    transport_host: "www.bing.com",
    redirect_path: "/ck/a",
    redirect_parameters: &[],
    encoded_redirect: Some(EncodedRedirect {
        parameter: "u",
        prefix: "a1",
    }),
};

const RECOVERY_PROVIDER: SearchProvider = SearchProvider {
    name: "recovery",
    endpoint: "https://html.duckduckgo.com/html/",
    query_parameter: "q",
    transport_host: "duckduckgo.com",
    redirect_path: "/l/",
    redirect_parameters: &["uddg"],
    encoded_redirect: None,
};

const PROVIDERS: [SearchProvider; 3] = [DEFAULT_PROVIDER, FALLBACK_PROVIDER, RECOVERY_PROVIDER];

pub(super) fn providers() -> &'static [SearchProvider] {
    &PROVIDERS
}

impl SearchProvider {
    pub(super) fn name(&self) -> &'static str {
        self.name
    }

    pub(super) fn search_url(&self, query: &str) -> anyhow::Result<String> {
        let mut url = Url::parse(self.endpoint)?;
        url.query_pairs_mut()
            .append_pair(self.query_parameter, query);
        Ok(url.to_string())
    }

    /// Convert one provider-page link into an ordinary external candidate.
    /// Links owned by the exact transport host are either explicitly unwrapped
    /// or discarded; external links retain their complete path and query.
    pub(super) fn candidate_url(&self, raw: &str) -> Option<String> {
        let parsed = Url::parse(raw).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return None;
        }
        if !self.is_transport_host(&parsed) {
            return Some(raw.to_string());
        }
        if parsed.path() != self.redirect_path {
            return None;
        }
        let direct = parsed.query_pairs().find_map(|(key, value)| {
            self.redirect_parameters
                .contains(&key.as_ref())
                .then(|| value.into_owned())
                .filter(|target| valid_external_target(target))
        });
        direct.or_else(|| self.decode_redirect(&parsed))
    }

    fn decode_redirect(&self, url: &Url) -> Option<String> {
        let encoding = self.encoded_redirect?;
        let encoded = url
            .query_pairs()
            .find_map(|(key, value)| (key == encoding.parameter).then(|| value.into_owned()))?;
        if valid_external_target(&encoded) {
            return Some(encoded);
        }
        let payload = encoded.strip_prefix(encoding.prefix)?;
        let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let target = String::from_utf8(bytes).ok()?;
        valid_external_target(&target).then_some(target)
    }

    fn is_transport_host(&self, url: &Url) -> bool {
        url.domain()
            .is_some_and(|host| host.eq_ignore_ascii_case(self.transport_host))
    }
}

fn valid_external_target(target: &str) -> bool {
    Url::parse(target)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PROVIDER: SearchProvider = SearchProvider {
        name: "test",
        endpoint: "https://search.invalid/find",
        query_parameter: "query",
        transport_host: "search.invalid",
        redirect_path: "/out",
        redirect_parameters: &["target"],
        encoded_redirect: None,
    };

    #[test]
    fn provider_builds_search_url_without_manual_query_encoding() {
        assert_eq!(
            TEST_PROVIDER.search_url("two words & symbols").unwrap(),
            "https://search.invalid/find?query=two+words+%26+symbols"
        );
    }

    #[test]
    fn fallback_provider_is_bounded_and_keeps_direct_external_candidates() {
        assert_eq!(providers().len(), 3);
        assert_eq!(providers()[1].name(), "fallback");
        assert_eq!(
            providers()[1].search_url("two words").unwrap(),
            "https://www.bing.com/search?q=two+words"
        );
        assert_eq!(
            providers()[1]
                .candidate_url("https://source.invalid/fact")
                .as_deref(),
            Some("https://source.invalid/fact")
        );
    }

    #[test]
    fn recovery_provider_unwraps_its_public_redirect() {
        let target = "https://docs.example.com/reference?section=limits";
        let transport = format!(
            "https://duckduckgo.com/l/?uddg={}",
            urlencoding::encode(target)
        );
        assert_eq!(providers()[2].name(), "recovery");
        assert_eq!(
            providers()[2].search_url("two words").unwrap(),
            "https://html.duckduckgo.com/html/?q=two+words"
        );
        assert_eq!(
            providers()[2].candidate_url(&transport).as_deref(),
            Some(target)
        );
    }

    #[test]
    fn fallback_provider_decodes_its_transport_link() {
        let target = "https://docs.example.com/reference?section=limits";
        let encoded = format!("a1{}", URL_SAFE_NO_PAD.encode(target));
        let transport = format!(
            "https://www.bing.com/ck/a?u={}",
            urlencoding::encode(&encoded)
        );
        assert_eq!(
            providers()[1].candidate_url(&transport).as_deref(),
            Some(target)
        );
    }

    #[test]
    fn only_exact_provider_transport_links_are_filtered_or_unwrapped() {
        let target = "https://source.invalid/search?section=preferences";
        let wrapped = format!(
            "https://search.invalid/out?target={}",
            urlencoding::encode(target)
        );
        assert_eq!(
            TEST_PROVIDER.candidate_url(&wrapped).as_deref(),
            Some(target)
        );
        assert_eq!(TEST_PROVIDER.candidate_url(target).as_deref(), Some(target));
        let lookalike = format!(
            "https://not-search.invalid/out?target={}",
            urlencoding::encode(target)
        );
        assert_eq!(
            TEST_PROVIDER.candidate_url(&lookalike).as_deref(),
            Some(lookalike.as_str())
        );
        assert!(
            TEST_PROVIDER
                .candidate_url("https://search.invalid/preferences")
                .is_none()
        );
    }
}
