// --- Localization ---
pub struct LocaleText {
    pub history_btn: &'static str,
    pub history_title: &'static str,
    pub max_items_label: &'static str,
    pub history_empty: &'static str,
    pub clear_all_history_btn: &'static str,
    pub view_image_btn: &'static str,
    pub listen_audio_btn: &'static str,
    pub view_text_btn: &'static str, // NEW

    pub prompt_mode_fixed: &'static str,
    pub prompt_mode_dynamic: &'static str,

    pub get_key_link: &'static str,
    pub gemini_api_key_label: &'static str,
    pub gemini_get_key_link: &'static str,
    pub openrouter_api_key_label: &'static str,
    pub openrouter_get_key_link: &'static str,
    pub use_groq_checkbox: &'static str,
    pub use_gemini_checkbox: &'static str,
    pub use_openrouter_checkbox: &'static str,
    pub cerebras_api_key_label: &'static str,
    pub cerebras_get_key_link: &'static str,
    pub use_cerebras_checkbox: &'static str,

    pub global_settings: &'static str,
    pub preset_name_label: &'static str,

    pub search_placeholder: &'static str,

    pub auto_paste_label: &'static str,
    pub auto_paste_newline_label: &'static str,
    pub startup_label: &'static str,
    pub add_hotkey_button: &'static str,
    pub press_keys: &'static str,
    pub cancel_label: &'static str,
    pub reset_defaults_btn: &'static str,

    pub preset_type_label: &'static str,
    pub preset_type_image: &'static str,
    pub preset_type_audio: &'static str,
    pub preset_type_video: &'static str,
    pub preset_type_text: &'static str, // NEW

    pub force_quit: &'static str, // NEW

    pub audio_source_label: &'static str,
    pub audio_src_mic: &'static str,
    pub audio_src_device: &'static str,
    pub hide_recording_ui_label: &'static str,
    pub auto_stop_recording_label: &'static str, // Silence-based auto-stop
    pub hotkeys_section: &'static str,
    pub start_in_tray_label: &'static str,
    pub footer_admin_running: &'static str,
    pub admin_startup_on: &'static str,
    pub admin_startup_success: &'static str,
    pub admin_startup_fail: &'static str,
    pub graphics_mode_label: &'static str,
    pub graphics_mode_standard: &'static str,
    pub graphics_mode_minimal: &'static str,
    pub usage_statistics_title: &'static str,
    pub usage_statistics_tooltip: &'static str,
    pub usage_model_column: &'static str,
    pub usage_remaining_column: &'static str,
    pub usage_check_link: &'static str,

    pub footer_admin_text: &'static str,
    pub footer_version: &'static str,
    pub check_for_updates_btn: &'static str,
    pub current_version_label: &'static str,
    pub checking_github: &'static str,
    pub up_to_date: &'static str,
    pub check_again_btn: &'static str,
    pub new_version_available: &'static str,
    pub release_notes_label: &'static str,
    pub download_update_btn: &'static str,
    pub downloading_update: &'static str,
    pub update_failed: &'static str,
    pub app_folder_writable_hint: &'static str,
    pub retry_btn: &'static str,
    pub update_success: &'static str,
    pub restart_to_use_new_version: &'static str,
    pub restart_app_btn: &'static str,
    // --- NEW TEXT INPUT FIELDS ---
    pub text_input_mode_label: &'static str,
    pub text_mode_select: &'static str,
    pub text_mode_type: &'static str,
    pub continuous_input_label: &'static str, // Checkbox for continuous input mode
    pub command_mode_label: &'static str, // For prompt mode in text/image presets (different from text_input_mode_label)
    pub text_input_title_default: &'static str,
    pub text_input_placeholder: &'static str,
    pub text_input_footer_submit: &'static str,
    pub text_input_footer_newline: &'static str,
    pub text_input_footer_cancel: &'static str,
    pub add_text_preset_btn: &'static str,
    pub add_image_preset_btn: &'static str,
    pub add_audio_preset_btn: &'static str,
    // --- PROCESSING CHAIN UI ---
    pub node_input_prefix: &'static str,
    pub node_input_audio: &'static str,
    pub node_input_image: &'static str,
    pub node_input_text: &'static str,
    pub node_process_title: &'static str,
    pub node_special_default: &'static str,
    pub node_special_image_to_text: &'static str,
    pub node_special_audio_to_text: &'static str,
    pub node_menu_add_normal: &'static str,
    pub node_menu_add_special_generic: &'static str,
    pub node_menu_add_special_image: &'static str,
    pub node_menu_add_special_audio: &'static str,
    pub input_auto_copy_tooltip: &'static str,
    pub input_auto_speak_tooltip: &'static str,

    pub tips_title: &'static str,
    pub tips_list: Vec<&'static str>,
    pub tips_click_hint: &'static str,
    pub restore_preset_btn: &'static str,
    pub restore_preset_tooltip: &'static str,
    // --- COMPOUND SEARCH UI ---
    pub search_doing: &'static str,            // "Doing" / "Đang"
    pub search_searching: &'static str,        // "searching" / "tìm kiếm"
    pub search_query_label: &'static str,      // "Search queries:" / "Truy vấn tìm kiếm:"
    pub search_found_sources: &'static str,    // "FOUND {} SOURCES" / "ĐÃ TÌM THẤY {} NGUỒN"
    pub search_sources_label: &'static str, // "Reference sources (by relevance):" / "Nguồn tham khảo (theo độ liên quan):"
    pub search_no_title: &'static str,      // "(No title)" / "(Không có tiêu đề)"
    pub search_synthesizing: &'static str,  // "SYNTHESIZING INFO..." / "ĐANG TỔNG HỢP THÔNG TIN..."
    pub search_analyzed_sources: &'static str, // "Analyzed {} sources" / "Đã phân tích {} nguồn"
    pub search_processing: &'static str, // "Processing and summarizing results..." / "Đang xử lý và tóm tắt kết quả..."
    // --- MASTER PRESET UI ---
    pub controller_checkbox_label: &'static str, // "Bộ điều khiển" / "Controller" / "컨트롤러"

    // --- GLOBAL SETTINGS UI HEADERS ---
    pub api_keys_header: &'static str,
    pub groq_label: &'static str,
    pub software_update_header: &'static str,
    pub startup_display_header: &'static str,
    pub favorite_overlay_opacity_label: &'static str,
    // --- MODEL THINKING INDICATOR ---
    pub model_thinking: &'static str,
    // --- REALTIME OVERLAY ---
    pub realtime_listening: &'static str,
    pub realtime_device: &'static str,
    pub realtime_waiting: &'static str,
    pub realtime_translation: &'static str,
    pub realtime_mic: &'static str,
    pub ollama_url_guide: &'static str,
    pub tts_settings_button: &'static str,
    pub tts_settings_title: &'static str,
    pub tts_method_label: &'static str,
    pub tts_method_standard: &'static str,
    pub tts_method_fast: &'static str,
    pub tts_method_edge: &'static str,
    pub tts_google_translate_title: &'static str,
    pub tts_google_translate_desc: &'static str,
    pub tts_edge_title: &'static str,
    pub tts_edge_desc: &'static str,
    pub tts_pitch_label: &'static str,
    pub tts_rate_label: &'static str,
    pub tts_voice_per_language_label: &'static str,
    pub tts_loading_voices: &'static str,
    pub tts_failed_load_voices: &'static str,
    pub tts_retry_label: &'static str,
    pub tts_initializing_voices: &'static str,
    pub tts_add_language_label: &'static str,
    pub tts_reset_to_defaults_label: &'static str,
    pub tts_speed_label: &'static str,
    pub tts_speed_normal: &'static str,
    pub tts_speed_slow: &'static str,
    pub tts_speed_fast: &'static str,
    pub _tts_voice_label: &'static str,
    pub tts_preview_texts: Vec<&'static str>,
    pub tts_male: &'static str,
    pub tts_female: &'static str,
    pub tts_instructions_label: &'static str,
    pub tts_instructions_hint: &'static str,
    pub tts_add_condition: &'static str,
    // Realtime TTS modal
    pub realtime_tts_title: &'static str,
    pub realtime_tts_speed: &'static str,
    pub realtime_tts_auto: &'static str,
    // App selection modal
    pub app_select_title: &'static str,
    pub app_select_hint: &'static str,
    // --- TRAY MENU ---
    pub tray_settings: &'static str,
    pub tray_quit: &'static str,
    pub tray_favorite_bubble: &'static str,
    pub tray_favorite_bubble_disabled: &'static str,
    // --- FAVORITE BUBBLE ---
    pub favorites_empty: &'static str,
    pub favorites_keep_open: &'static str,
    pub recording_subtext: &'static str,
    pub recording_paused: &'static str,
    // --- AUTO COPY BADGE ---
    pub auto_copied_badge: &'static str,
    pub auto_copied_image_badge: &'static str,
    pub live_translate_loading: &'static str,
    pub text_input_loading: &'static str,
    pub recording_loading: &'static str,
    pub markdown_view_loading: &'static str,
    pub preset_wheel_loading: &'static str,
    pub prompt_dj_loading: &'static str,
    pub tray_popup_loading: &'static str,
    pub update_available_notification: &'static str,
    pub cannot_type_no_caret: &'static str,
    // --- DROP OVERLAY ---
    pub drop_overlay_text: &'static str,
    // --- REALTIME EGUI SPECIFIC ---
    pub device_mode_warning: &'static str,
    pub select_app_btn: &'static str,
    pub toggle_translation_tooltip: &'static str,
    pub toggle_transcription_tooltip: &'static str,
    pub font_minus_tooltip: &'static str,
    pub font_plus_tooltip: &'static str,
    pub google_gtx_label: &'static str,
    pub opacity_label: &'static str,
    pub downloaded_successfully: &'static str,
    pub download_recording_tooltip: &'static str,
    // --- HELP ASSISTANT ---
    pub help_assistant_btn: &'static str,
    pub help_assistant_title: &'static str,
    pub help_assistant_question_label: &'static str,
    pub help_assistant_placeholder: &'static str,
    pub help_assistant_ask_btn: &'static str,
    pub help_assistant_loading: &'static str,
    pub help_assistant_answer_label: &'static str,
    pub help_assistant_hint: &'static str,

    // --- PROMPT DJ ---
    pub prompt_dj_btn: &'static str,
    pub prompt_dj_title: &'static str,
    pub screen_record_btn: &'static str,
    pub screen_record_title: &'static str,
    pub pointer_gallery_btn: &'static str,
    // --- PARAKEET DOWNLOAD MODAL ---
    pub parakeet_downloading_title: &'static str,
    pub parakeet_downloading_message: &'static str,
    pub parakeet_downloading_file: &'static str, // "Downloading {}..."
    pub parakeet_supports_english_only: &'static str,
    // --- OVERLAY BUTTONS TOOLTIPS ---
    pub overlay_copy_tooltip: &'static str,
    pub overlay_undo_tooltip: &'static str,
    pub overlay_redo_tooltip: &'static str,
    pub overlay_edit_tooltip: &'static str,
    pub overlay_refine_placeholder: &'static str, // NEW
    pub overlay_markdown_tooltip: &'static str,
    pub overlay_download_tooltip: &'static str,
    pub overlay_speaker_tooltip: &'static str,
    pub overlay_broom_tooltip: &'static str,
    pub overlay_back_tooltip: &'static str,
    pub overlay_forward_tooltip: &'static str,
    pub overlay_opacity_tooltip: &'static str,
    pub download_feature_btn: &'static str,
    pub download_feature_title: &'static str,
    pub download_delete_deps_btn: &'static str, // "delete yt-dlp (xx MB), ffmpeg (xx MB) and Deno (xx MB)"
    pub download_url_label: &'static str,
    pub download_format_label: &'static str,
    pub download_start_btn: &'static str,
    pub download_open_file_btn: &'static str,
    pub download_open_folder_btn: &'static str,
    pub download_status_starting: &'static str,
    pub download_status_finished: &'static str,
    pub download_status_error: &'static str,
    pub download_deps_missing: &'static str,
    pub download_deps_ytdlp: &'static str,
    pub download_deps_ffmpeg: &'static str,
    pub download_deps_download_btn: &'static str,
    pub download_status_ready: &'static str,
    pub download_status_extracting: &'static str,
    pub download_cancel_btn: &'static str,
    pub download_file_label: &'static str,
    pub download_size_label: &'static str,
    pub download_change_folder_btn: &'static str,
    // Format: "{percent}% of {total} at {speed}, ETA {eta}"
    pub download_progress_info_fmt: &'static str,
    pub download_advanced_header: &'static str,
    pub download_opt_metadata: &'static str,
    pub download_opt_sponsorblock: &'static str,
    pub download_opt_subtitles: &'static str,
    pub download_opt_playlist: &'static str,
    pub download_opt_cookies: &'static str,
    pub download_scan_ignore_btn: &'static str,    // NEW
    pub download_quality_label_text: &'static str, // NEW
    pub download_quality_best: &'static str,       // NEW
    pub download_scanning_label: &'static str,     // NEW
    pub download_no_cookie_option: &'static str,   // NEW
    pub download_show_log_btn: &'static str,       // NEW
    pub download_hide_log_btn: &'static str,       // NEW
    pub download_subtitle_label: &'static str,     // NEW
    pub download_subtitle_auto: &'static str,
    pub download_subs_found_header: &'static str, // NEW
    pub download_subs_none_found: &'static str,   // NEW
    pub download_deno_required_title: &'static str,
    pub download_deno_required_body: &'static str,
    pub download_deno_required_question: &'static str,
    pub download_deno_downloading_fmt: &'static str,
    pub download_deno_extracting: &'static str,
    pub download_deno_failed_fmt: &'static str,
    pub download_deno_yes_btn: &'static str,
    pub download_deno_no_btn: &'static str,

    // --- DOWNLOADED TOOLS MODAL ---
    pub downloaded_tools_button: &'static str,
    pub downloaded_tools_title: &'static str,
    pub tool_parakeet: &'static str,
    pub tool_ytdlp: &'static str,
    pub tool_deno: &'static str,
    pub tool_ffmpeg: &'static str,
    pub tool_status_installed: &'static str, // "Installed ({})"
    pub tool_status_missing: &'static str,
    pub tool_action_download: &'static str,
    pub tool_action_delete: &'static str,
    pub tool_desc_parakeet: &'static str,
    pub tool_desc_ytdlp: &'static str,
    pub tool_desc_deno: &'static str,
    pub tool_desc_ffmpeg: &'static str,
    pub tool_downloadable_backgrounds: &'static str,
    pub tool_desc_downloadable_backgrounds: &'static str,
    pub tool_bg_downloaded_count_fmt: &'static str,
    pub tool_bg_action_download_all: &'static str,
    pub tool_bg_action_download_rest: &'static str,
    pub tool_bg_action_delete_downloaded: &'static str,
    pub tool_bg_action_delete_all: &'static str,
    pub tool_bg_downloading_fmt: &'static str,
    pub tool_downloadable_pointer_collections: &'static str,
    pub tool_desc_downloadable_pointer_collections: &'static str,
    pub tool_pointer_downloaded_count_fmt: &'static str,
    pub pointer_restore_original_btn: &'static str,
    pub pointer_restore_success: &'static str,
    pub pointer_action_stop: &'static str,
    pub pointer_action_resume: &'static str,
    pub pointer_action_start_download: &'static str,
    pub pointer_action_apply: &'static str,
    pub pointer_action_retry: &'static str,
    pub pointer_size_label: &'static str,
    pub pointer_status_queued: &'static str,
    pub pointer_status_downloading_fmt: &'static str,
    pub pointer_status_paused_fmt: &'static str,
    pub pointer_status_ready: &'static str,
    pub pointer_status_applying: &'static str,
    pub pointer_status_applied: &'static str,
    pub pointer_status_error: &'static str,
    pub pointer_apply_success_fmt: &'static str,
    pub pointer_download_paused: &'static str,
    pub tool_update_checking: &'static str,
    pub tool_update_latest: &'static str,
    pub tool_update_check_again: &'static str,
    pub tool_update_error: &'static str,
    pub tool_update_retry: &'static str,
    pub tool_update_check_btn: &'static str,
    pub tool_update_available: &'static str,
    // --- CONTINUOUS MODE ---
    pub continuous_mode_activated: &'static str, // "✨ Cấu hình \"{preset}\" sẽ hoạt động liên tục, bấm ESC hay {hotkey} để thoát"
}

mod en;
mod ko;
mod vi;

impl LocaleText {
    pub fn get(lang_code: &str) -> Self {
        match lang_code {
            "vi" => vi::get(),
            "ko" => ko::get(),
            _ => en::get(),
        }
    }
}
