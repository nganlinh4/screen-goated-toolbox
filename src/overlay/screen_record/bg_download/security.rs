use std::sync::LazyLock;

pub(super) const MAX_BACKGROUND_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKGROUND_PIXELS: u64 = 67_108_864;
const MAX_BACKGROUND_URL_BYTES: usize = 8 * 1024;

static BACKGROUND_HTTP_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into()
});

pub(crate) fn validate_background_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 128 {
        return Err("background id must contain 1 to 128 characters".to_string());
    }
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(
            "background id may contain only ASCII letters, digits, '-' and '_'".to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_download_url(url: &str) -> Result<(), String> {
    if url.len() > MAX_BACKGROUND_URL_BYTES || url.contains(['\r', '\n', '\0']) {
        return Err("Background URL is invalid or too long".to_string());
    }
    let parsed =
        url::Url::parse(url).map_err(|error| format!("Invalid background URL: {error}"))?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Background downloads require an HTTPS URL without credentials".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Background URL has no host".to_string())?;
    let allowed = ["google.com", "googleusercontent.com", "4kwallpapers.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
    if !allowed {
        return Err("Background URL host is not allowed".to_string());
    }
    Ok(())
}

fn redirect_target(
    current: &str,
    response: &ureq::http::Response<ureq::Body>,
) -> Result<String, String> {
    let location = response
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "Background redirect has no valid Location header".to_string())?;
    url::Url::parse(current)
        .and_then(|base| base.join(location))
        .map(|target| target.to_string())
        .map_err(|error| format!("Invalid background redirect: {error}"))
}

pub(super) fn get_background_response(
    url: &str,
    user_agent: &str,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    request_with_redirects(url, user_agent, false, "download")
}

pub(super) fn head_background_response(
    url: &str,
    user_agent: &str,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    request_with_redirects(url, user_agent, true, "request")
}

fn request_with_redirects(
    url: &str,
    user_agent: &str,
    head: bool,
    label: &str,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut current = url.to_string();
    for _ in 0..=5 {
        validate_download_url(&current)?;
        let response = if head {
            BACKGROUND_HTTP_AGENT.head(&current)
        } else {
            BACKGROUND_HTTP_AGENT.get(&current)
        }
        .header("User-Agent", user_agent)
        .call()
        .map_err(|error| error.to_string())?;
        if !response.status().is_redirection() {
            if response.status().is_success() {
                return Ok(response);
            }
            return Err(format!(
                "Background {label} returned HTTP {}",
                response.status()
            ));
        }
        current = redirect_target(&current, &response)?;
    }
    Err(format!("Background {label} exceeded the redirect limit"))
}

pub(super) fn validate_download_request(id: &str, url: &str) -> Result<(), String> {
    validate_background_id(id)?;
    let entry = super::downloadable_backgrounds()
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "Unknown downloadable background".to_string())?;
    if entry.download_url != url {
        return Err("Background URL does not match the embedded catalog".to_string());
    }
    validate_download_url(url)
}

pub(super) fn validate_image_bytes(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() as u64 > MAX_BACKGROUND_BYTES {
        return Err("Background image exceeds the 64 MiB limit".to_string());
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("Detect image format failed: {error}"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| format!("Read image dimensions failed: {error}"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_BACKGROUND_PIXELS {
        return Err("Background image dimensions exceed the supported limit".to_string());
    }
    Ok((width, height))
}
