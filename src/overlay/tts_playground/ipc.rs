//! IPC dispatcher between the React WebView and Rust.
//!
//! Handles WebView commands for window chrome, config patches, reference
//! management, generation, playback, and exports.

use serde::Deserialize;
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    HTCAPTION, PostMessageW, SW_MINIMIZE, SendMessageW, ShowWindow, WM_CLOSE, WM_NCLBUTTONDOWN,
};

use crate::config::save_config;

use super::state::{self, parse_method, parse_mode};

#[derive(Deserialize)]
struct IpcEnvelope {
    #[serde(default)]
    id: String,
    cmd: String,
    #[serde(default)]
    args: Value,
}

pub(super) fn handle_ipc(hwnd: HWND, body: &str) {
    let envelope: IpcEnvelope = match serde_json::from_str(body) {
        Ok(env) => env,
        Err(err) => {
            eprintln!("[tts-playground] invalid ipc: {err}");
            return;
        }
    };
    super::runtime::tick_position();
    let reply = dispatch(hwnd, &envelope.cmd, &envelope.args);
    send_reply(&envelope.id, reply);
    state::sync_to_webview();
}

fn dispatch(hwnd: HWND, cmd: &str, args: &Value) -> Result<Value, String> {
    match cmd {
        "set_mode" => {
            let value = args.get("mode").and_then(Value::as_str).unwrap_or("");
            if let Some(mode) = parse_mode(value) {
                with_config(|cfg| cfg.tts_playground.mode = mode);
            }
            Ok(Value::Null)
        }
        "set_method" => {
            let value = args.get("method").and_then(Value::as_str).unwrap_or("");
            if let Some(method) = parse_method(value) {
                with_config(|cfg| cfg.tts_playground.method = method);
            }
            Ok(Value::Null)
        }
        "set_draft_text" => {
            let value = args.get("text").and_then(Value::as_str).unwrap_or("");
            with_config(|cfg| cfg.tts_playground.draft_text = value.to_string());
            Ok(Value::Null)
        }
        "patch_gemini" => {
            apply_patch(args, |cfg, patch| {
                let pg = &mut cfg.tts_playground;
                if let Some(v) = patch.get("model").and_then(Value::as_str) {
                    pg.gemini_model = v.to_string();
                }
                if let Some(v) = patch.get("voice").and_then(Value::as_str) {
                    pg.gemini_voice = v.to_string();
                }
                if let Some(v) = patch.get("speed").and_then(Value::as_str) {
                    pg.gemini_speed = v.to_string();
                }
                if let Some(v) = patch.get("instruction").and_then(Value::as_str) {
                    pg.gemini_instruction = v.to_string();
                }
                if let Some(list) = patch.get("conditions").and_then(Value::as_array) {
                    pg.gemini_language_conditions = list
                        .iter()
                        .filter_map(|item| {
                            let language = item.get("language")?.as_str()?;
                            let name = item.get("name").and_then(Value::as_str).unwrap_or(language);
                            let instruction = item
                                .get("instruction")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            Some(crate::config::TtsLanguageCondition::new(
                                language,
                                name,
                                instruction,
                            ))
                        })
                        .collect();
                }
            });
            Ok(Value::Null)
        }
        "patch_edge" => {
            apply_patch(args, |cfg, patch| {
                let pg = &mut cfg.tts_playground;
                if let Some(v) = patch.get("pitch").and_then(Value::as_i64) {
                    pg.edge_pitch = v as i32;
                    pg.edge_settings.pitch = v as i32;
                }
                if let Some(v) = patch.get("rate").and_then(Value::as_i64) {
                    pg.edge_rate = v as i32;
                    pg.edge_settings.rate = v as i32;
                }
                if let Some(list) = patch.get("voices").and_then(Value::as_array) {
                    pg.edge_settings.voice_configs = list
                        .iter()
                        .filter_map(|v| {
                            let language = v.get("language")?.as_str()?.to_string();
                            let voice = v.get("voice")?.as_str()?.to_string();
                            Some(crate::config::EdgeTtsVoiceConfig::new(
                                &language, &language, &voice,
                            ))
                        })
                        .collect();
                }
            });
            Ok(Value::Null)
        }
        "patch_google" => {
            apply_patch(args, |cfg, patch| {
                if let Some(v) = patch.get("speed").and_then(Value::as_str) {
                    cfg.tts_playground.google_speed = v.to_string();
                }
            });
            Ok(Value::Null)
        }
        "patch_step_audio" => {
            apply_patch(args, |cfg, patch| {
                if let Some(v) = patch.get("reference").and_then(Value::as_str) {
                    cfg.tts_playground.step_audio_settings.reference_voice_id = v.to_string();
                }
            });
            Ok(Value::Null)
        }
        "patch_magpie" => {
            apply_patch(args, |cfg, patch| {
                let s = &mut cfg.tts_playground.magpie_settings;
                if let Some(v) = patch.get("voice").and_then(Value::as_str) {
                    s.voice = v.to_string();
                }
                if let Some(list) = patch.get("voices").and_then(Value::as_array) {
                    s.voice_configs = list
                        .iter()
                        .filter_map(|item| {
                            let language = item.get("language")?.as_str()?.to_string();
                            let voice = item.get("voice")?.as_str()?.to_string();
                            Some(crate::config::MagpieVoiceConfig::new(
                                &language, &language, &voice,
                            ))
                        })
                        .collect();
                }
            });
            Ok(Value::Null)
        }
        "patch_kokoro" => {
            apply_patch(args, |cfg, patch| {
                let s = &mut cfg.tts_playground.kokoro_settings;
                if let Some(v) = patch.get("speed").and_then(Value::as_f64) {
                    s.speed = v as f32;
                }
                if let Some(v) = patch.get("threads").and_then(Value::as_i64) {
                    s.num_threads = v as i32;
                }
                if let Some(v) = patch.get("voice").and_then(Value::as_str) {
                    s.voice = v.to_string();
                }
                if let Some(list) = patch.get("voices").and_then(Value::as_array) {
                    s.voice_configs = list
                        .iter()
                        .filter_map(|item| {
                            let language = item.get("language")?.as_str()?.to_string();
                            let voice = item.get("voice")?.as_str()?.to_string();
                            Some(crate::config::KokoroVoiceConfig::new(
                                &language, &language, &voice,
                            ))
                        })
                        .collect();
                }
            });
            Ok(Value::Null)
        }
        "patch_supertonic" => {
            apply_patch(args, |cfg, patch| {
                let s = &mut cfg.tts_playground.supertonic_settings;
                if let Some(v) = patch.get("speed").and_then(Value::as_f64) {
                    s.speed = v as f32;
                }
                if let Some(v) = patch.get("threads").and_then(Value::as_i64) {
                    s.num_threads = v as i32;
                }
                if let Some(v) = patch.get("steps").and_then(Value::as_i64) {
                    s.num_steps = v as i32;
                }
                if let Some(list) = patch.get("voices").and_then(Value::as_array) {
                    s.voice_configs = list
                        .iter()
                        .filter_map(|item| {
                            let language = item.get("language")?.as_str()?.to_string();
                            let voice = item.get("voice")?.as_str()?.to_string();
                            Some(crate::config::SupertonicVoiceConfig::new(
                                &language, &language, &voice,
                            ))
                        })
                        .collect();
                }
            });
            Ok(Value::Null)
        }
        "patch_vieneu" => {
            apply_patch(args, |cfg, patch| {
                if let Some(v) = patch.get("reference").and_then(Value::as_str) {
                    cfg.tts_playground.vieneu_settings.reference_voice_id = v.to_string();
                }
            });
            Ok(Value::Null)
        }
        "patch_audio_edit" => {
            apply_patch(args, |cfg, patch| {
                let s = &mut cfg.tts_playground.step_audio_edit_settings;
                if let Some(v) = patch.get("sourcePath").and_then(Value::as_str) {
                    s.source_audio_path = v.to_string();
                }
                if let Some(v) = patch.get("sourceText").and_then(Value::as_str) {
                    s.source_text = v.to_string();
                }
                if let Some(v) = patch.get("editType").and_then(Value::as_str) {
                    s.edit_type = v.to_string();
                }
                if let Some(v) = patch.get("editInfo").and_then(Value::as_str) {
                    s.edit_info = v.to_string();
                }
                if let Some(v) = patch.get("targetText").and_then(Value::as_str) {
                    s.target_text = v.to_string();
                }
            });
            Ok(Value::Null)
        }
        "set_s2s_target_language" => {
            let lang = args.get("language").and_then(Value::as_str).unwrap_or("");
            with_config(|cfg| cfg.realtime_target_language = lang.to_string());
            Ok(Value::Null)
        }
        "generate" => {
            super::runtime::start_generation();
            Ok(Value::Null)
        }
        "cancel_generation" => {
            super::runtime::cancel_generation();
            Ok(Value::Null)
        }
        "clear" => {
            super::runtime::clear_current();
            Ok(Value::Null)
        }
        "play" => {
            super::runtime::play();
            Ok(Value::Null)
        }
        "pause" => {
            super::runtime::pause();
            Ok(Value::Null)
        }
        "stop" => {
            super::runtime::stop();
            Ok(Value::Null)
        }
        "replay" => {
            super::runtime::replay();
            Ok(Value::Null)
        }
        "seek" => {
            let sec = args.get("sec").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            super::runtime::seek(sec);
            Ok(Value::Null)
        }
        "download_wav" => super::runtime::download_wav().map(|opt| match opt {
            Some(path) => Value::String(path),
            None => Value::Null,
        }),
        "download_mp3" => {
            super::runtime::start_mp3_export();
            Ok(Value::Null)
        }
        "play_recent" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::play_recent(id);
            Ok(Value::Null)
        }
        "delete_recent" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::delete_recent(id);
            Ok(Value::Null)
        }
        "preview_voice" => {
            let speaker = args.get("speaker").and_then(Value::as_str).unwrap_or("");
            super::runtime::preview_voice(speaker);
            Ok(Value::Null)
        }
        "reset_provider" => {
            let provider = args.get("provider").and_then(Value::as_str).unwrap_or("");
            super::runtime::reset_provider(provider);
            Ok(Value::Null)
        }
        "pick_source_audio" => super::runtime::pick_source_audio().map(|opt| match opt {
            Some(path) => Value::String(path),
            None => Value::Null,
        }),
        "start_mic_recording" => {
            super::runtime::start_mic_recording();
            Ok(Value::Null)
        }
        "stop_mic_recording" => {
            super::runtime::stop_mic_recording();
            Ok(Value::Null)
        }
        "start_reference_mic" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::start_reference_mic(id);
            Ok(Value::Null)
        }
        "stop_reference_mic" => {
            super::runtime::stop_reference_mic();
            Ok(Value::Null)
        }
        "use_current_as_source" => {
            super::runtime::use_current_as_source();
            Ok(Value::Null)
        }
        "add_reference" => {
            super::runtime::add_reference();
            Ok(Value::Null)
        }
        "update_reference" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            let label = args.get("label").and_then(Value::as_str);
            let transcript = args.get("transcript").and_then(Value::as_str);
            super::runtime::update_reference(id, label, transcript);
            Ok(Value::Null)
        }
        "delete_reference" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::delete_reference(id);
            Ok(Value::Null)
        }
        "pick_reference_audio" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::pick_reference_audio(id).map(|opt| match opt {
                Some(path) => Value::String(path),
                None => Value::Null,
            })
        }
        "recognize_reference" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::recognize_reference(id);
            Ok(Value::Null)
        }
        "play_reference" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            super::runtime::play_reference(id);
            Ok(Value::Null)
        }
        "use_reference" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            let target = args
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("playground");
            super::runtime::use_reference(id, target);
            Ok(Value::Null)
        }
        "close_window" => {
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
            Ok(Value::Null)
        }
        "minimize_window" => {
            unsafe {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            Ok(Value::Null)
        }
        "start_drag" => {
            unsafe {
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(windows::Win32::Foundation::WPARAM(HTCAPTION as usize)),
                    Some(windows::Win32::Foundation::LPARAM(0)),
                );
            }
            Ok(Value::Null)
        }
        _ => Err(format!("unknown cmd: {cmd}")),
    }
}

fn apply_patch(args: &Value, f: impl FnOnce(&mut crate::config::Config, &Value)) {
    let patch = args
        .get("patch")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    with_config(|cfg| f(cfg, &patch));
}

fn with_config(f: impl FnOnce(&mut crate::config::Config)) {
    if let Ok(mut app) = crate::APP.lock() {
        f(&mut app.config);
        save_config(&app.config);
    }
}

fn send_reply(id: &str, result: Result<Value, String>) {
    if id.is_empty() {
        return;
    }
    let payload = match result {
        Ok(value) => json!({ "id": id, "result": value }),
        Err(err) => json!({ "id": id, "error": err }),
    };
    let script =
        format!("window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload} }}));");
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(&script);
        }
    });
}
