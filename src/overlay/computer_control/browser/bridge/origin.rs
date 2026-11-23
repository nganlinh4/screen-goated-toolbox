//! Classify a browser Origin without treating malformed headers as absent.
use tungstenite::http::HeaderMap;

pub(super) fn allowed(headers: &HeaderMap, pinned: Option<&str>, pairing: bool) -> bool {
    let mut origins = headers.get_all("origin").iter();
    let Some(value) = origins.next() else {
        return true;
    };
    if origins.next().is_some() {
        return false;
    }
    let Ok(text) = value.to_str() else {
        return false;
    };
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    let Some(id) = url.host_str() else {
        return false;
    };
    if url.scheme() != "chrome-extension"
        || id.len() != 32
        || !id.bytes().all(|b| (b'a'..=b'p').contains(&b))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    pairing || pinned.is_none_or(|expected| expected == id)
}

pub(super) fn matches_claim(headers: &HeaderMap, claimed: Option<&str>) -> bool {
    let Some(value) = headers.get("origin") else {
        return true;
    };
    let Some(id) = value
        .to_str()
        .ok()
        .and_then(|s| url::Url::parse(s).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
    else {
        return false;
    };
    claimed == Some(id.as_str())
}

#[cfg(test)]
#[path = "origin_tests.rs"]
mod tests;
