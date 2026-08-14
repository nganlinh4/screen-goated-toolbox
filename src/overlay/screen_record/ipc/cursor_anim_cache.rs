use base64::Engine as _;

use super::super::native_export;

pub(super) fn handle_load(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let slot_id = u32::try_from(args["slotId"].as_u64().ok_or("missing slotId")?)
        .map_err(|_| "cursor animation slot is too large")?;
    let svg_hash = args["svgHash"].as_str().ok_or("missing svgHash")?;
    match native_export::anim_cache::load_cache(slot_id, svg_hash) {
        Some(result) => {
            let preview_b64 = result
                .preview_pngs
                .iter()
                .map(|png| base64::engine::general_purpose::STANDARD.encode(png))
                .collect::<Vec<_>>();
            Ok(serde_json::json!({
                "cached": true,
                "loopDuration": result.loop_duration,
                "naturalWidth": result.natural_width,
                "naturalHeight": result.natural_height,
                "previewFrames": preview_b64,
            }))
        }
        None => Ok(serde_json::json!({ "cached": false })),
    }
}

pub(super) fn handle_save(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let slot_id = u32::try_from(args["slotId"].as_u64().ok_or("missing slotId")?)
        .map_err(|_| "cursor animation slot is too large")?;
    let svg_hash = args["svgHash"].as_str().ok_or("missing svgHash")?;
    let loop_duration = args["loopDuration"]
        .as_f64()
        .ok_or("missing loopDuration")?;
    let natural_width = u32::try_from(
        args["naturalWidth"]
            .as_u64()
            .ok_or("missing naturalWidth")?,
    )
    .map_err(|_| "cursor natural width is too large")?;
    let natural_height = u32::try_from(
        args["naturalHeight"]
            .as_u64()
            .ok_or("missing naturalHeight")?,
    )
    .map_err(|_| "cursor natural height is too large")?;

    let mut decoded_bytes = 0usize;
    let mut decode_array = |key: &str| -> Result<Vec<Vec<u8>>, String> {
        let values = args[key].as_array().ok_or(format!("missing {key}"))?;
        if values.is_empty() || values.len() > 240 {
            return Err(format!("{key} must contain 1 to 240 frames"));
        }
        let mut decoded = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            let encoded = value.as_str().ok_or(format!("{key}[{index}] not string"))?;
            if encoded.is_empty() || encoded.len() > 24 * 1024 * 1024 {
                return Err(format!("{key}[{index}] exceeds the frame size limit"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| format!("{key}[{index}] base64: {error}"))?;
            decoded_bytes = decoded_bytes.saturating_add(bytes.len());
            if decoded_bytes > 256 * 1024 * 1024 {
                return Err("cursor animation payload exceeds the 256 MiB limit".to_string());
            }
            decoded.push(bytes);
        }
        Ok(decoded)
    };

    let export_pngs = decode_array("exportPngs")?;
    let preview_frames = decode_array("previewFrames")?;
    native_export::anim_cache::save_cache(
        slot_id,
        svg_hash,
        loop_duration,
        natural_width,
        natural_height,
        &export_pngs,
        &preview_frames,
    )?;
    Ok(serde_json::Value::Null)
}
