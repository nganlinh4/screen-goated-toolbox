fn extract_filename_ext(content_disposition: &str) -> Option<String> {
    for part in content_disposition.split(';') {
        let p = part.trim();
        if let Some(raw) = p.strip_prefix("filename=") {
            let name = raw.trim_matches('"').trim_matches('\'');
            if let Some(ext) = name.rsplit('.').next() {
                return Some(ext.to_ascii_lowercase());
            }
        }
    }
    None
}

pub(super) fn detect_download_ext(
    content_type: &str,
    content_disposition: &str,
    url: &str,
) -> Result<&'static str, String> {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("image/png") {
        return Ok("png");
    }
    if ct.contains("image/jpeg") || ct.contains("image/jpg") {
        return Ok("jpg");
    }
    if ct.contains("image/webp") {
        return Ok("webp");
    }
    if ct.contains("text/html") {
        return Err("Downloaded page is HTML, not an image URL".to_string());
    }

    let mut candidate_ext: Option<String> = extract_filename_ext(content_disposition);
    if candidate_ext.is_none() {
        candidate_ext = url
            .rsplit('/')
            .next()
            .and_then(|name| name.rsplit('.').next())
            .map(|s| s.to_ascii_lowercase());
    }

    match candidate_ext.as_deref() {
        Some("png") => Ok("png"),
        Some("jpg") | Some("jpeg") => Ok("jpg"),
        Some("webp") => Ok("webp"),
        Some("heic") | Some("heif") | Some("avif") => Err(
            "Downloaded image format is unsupported (HEIC/HEIF/AVIF). Please use PNG/JPG/WEBP source.".to_string(),
        ),
        Some(other) => Err(format!("Unsupported image format '{}'", other)),
        None => Err(format!(
            "Unsupported content type '{}' (missing image extension)",
            content_type
        )),
    }
}

/// For Google Photos sharing URLs, try multiple strategies to get the actual image URL.
/// For other URLs, return as-is.
pub(super) fn resolve_image_url(url: &str) -> Result<String, String> {
    // Google Drive: convert /file/d/ID/view -> direct download URL
    if url.contains("drive.google.com/file/d/")
        && let Some(start) = url.find("/file/d/")
    {
        let after = &url[start + 8..];
        let file_id = after.split('/').next().unwrap_or(after);
        if !file_id.is_empty() {
            return Ok(format!(
                "https://drive.google.com/uc?export=download&id={file_id}"
            ));
        }
    }

    if !url.contains("photos.google.com") {
        return Ok(url.to_string());
    }

    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
              (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    // Strategy 1: Fetch the page and look for og:image or lh3 URLs in the HTML/JS
    if let Ok(response) = ureq::get(url).header("User-Agent", ua).call()
        && let Ok(body) = response.into_body().read_to_string()
    {
        // Try og:image meta tag
        if let Some(pos) = body.find("og:image") {
            let after = &body[pos..];
            if let Some(c_pos) = after.find("content=\"") {
                let url_start = c_pos + 9;
                if let Some(url_end) = after[url_start..].find('"') {
                    let raw_url = &after[url_start..url_start + url_end];
                    let decoded = raw_url.replace("&amp;", "&");
                    let base = decoded.split('=').next().unwrap_or(&decoded);
                    return Ok(format!("{base}=w2560-h2560"));
                }
            }
        }
        // Try any lh3.googleusercontent.com URL in the page source
        if let Some(pos) = body.find("https://lh3.googleusercontent.com/pw/")
            && let Some(end) = body[pos..].find(['"', '\'', '\\'])
        {
            let raw = &body[pos..pos + end];
            let decoded = raw.replace("\\u003d", "=").replace("&amp;", "&");
            let base = decoded.split('=').next().unwrap_or(&decoded);
            return Ok(format!("{base}=w2560-h2560"));
        }
    }

    // Strategy 2: Extract photo ID from URL path and construct direct lh3 URL
    // URL format: .../photo/AF1Qip.../...
    if let Some(photo_pos) = url.find("/photo/") {
        let after = &url[photo_pos + 7..];
        let photo_id = after.split('?').next().unwrap_or(after);
        if !photo_id.is_empty() {
            let direct = format!("https://lh3.googleusercontent.com/pw/{photo_id}=w2560-h2560");
            // Verify it returns an image (HEAD request)
            if let Ok(resp) = ureq::head(&direct).header("User-Agent", ua).call() {
                let ct = resp
                    .headers()
                    .get("Content-Type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if ct.starts_with("image/") {
                    return Ok(direct);
                }
            }
        }
    }

    Err("Could not resolve a direct image URL from Google Photos. \
         Try using a direct image link instead (e.g. Imgur, Google Drive export, etc.)"
        .to_string())
}
