use serde_json::Value;

pub(in crate::overlay::image_to_svg) fn issue_static_asset(path: &str) -> Result<Value, String> {
    let path = std::fs::canonicalize(path)
        .map_err(|_| "Vector result is no longer available.".to_string())?;
    if !super::is_known_result_path(&path) {
        return Err("Vector result is not available in this session.".to_string());
    }
    crate::overlay::creation_preview_protocol::issue_static_svg(&path.to_string_lossy())
}
