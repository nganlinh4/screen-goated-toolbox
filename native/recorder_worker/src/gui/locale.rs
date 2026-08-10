pub struct AuxiliaryLocaleText {
    pub win_select_title: &'static str,
    pub win_select_subtitle: &'static str,
    pub win_select_display_only_badge: &'static str,
    pub win_select_display_only_title: &'static str,
    pub win_select_display_only_message: &'static str,
    pub win_select_display_only_action: &'static str,
    pub win_select_count: &'static str,
    pub managed_tools: ManagedToolsLocaleText,
}

pub struct ManagedToolsLocaleText {
    pub tool_local_asr_worker: &'static str,
    pub tool_onnx_runtime: &'static str,
}

pub struct BadgeLocaleText {
    pub downloading_component_fmt: &'static str,
    pub preparing_component_fmt: &'static str,
    pub downloading_runtime_fmt: &'static str,
    pub fetching_runtime_manifest: &'static str,
    pub downloading_file_fmt: &'static str,
    pub preparing_runtime_fmt: &'static str,
    pub downloading_sherpa_runtime: &'static str,
    pub required_for_offline_tts_fmt: &'static str,
    pub downloading_model_fmt: &'static str,
    pub preparing_model_fmt: &'static str,
    pub export_failed: &'static str,
    pub subtitles_saved: &'static str,
    pub feature_needs_webview2_fmt: &'static str,
    pub feature_screen_record: &'static str,
    pub install_webview2_hint: &'static str,
}

pub struct GlobalSettingsLocaleText {
    pub model_thinking: &'static str,
}

pub struct PresetBasicsLocaleText {
    pub cancel_label: &'static str,
}

pub struct RealtimeLocaleText {
    pub app_select_title: &'static str,
    pub app_select_count: &'static str,
}

pub struct ShellLocaleText {
    pub auto_copied_badge: &'static str,
}

pub struct TtsPlaygroundLocaleText {
    pub screen_record_audio_ffmpeg_downloading: &'static str,
    pub screen_record_gif_ffmpeg_downloading: &'static str,
}

pub struct LocaleText {
    language: &'static str,
    pub auxiliary: AuxiliaryLocaleText,
    pub badge: BadgeLocaleText,
    pub global_settings: GlobalSettingsLocaleText,
    pub preset_basics: PresetBasicsLocaleText,
    pub realtime: RealtimeLocaleText,
    pub shell: ShellLocaleText,
    pub tool_runtime: super::tool_runtime_locale::ToolRuntimeLocaleText,
    pub tts_playground: TtsPlaygroundLocaleText,
}

struct SelectorLocaleText {
    title: &'static str,
    subtitle: &'static str,
    badge: &'static str,
    display_title: &'static str,
    display_message: &'static str,
    display_action: &'static str,
}

impl LocaleText {
    pub fn get(lang_code: &str) -> Self {
        match lang_code {
            "vi" => Self::new(
                "vi",
                SelectorLocaleText {
                    title: "Chọn Cửa Sổ để Quay",
                    subtitle: "Nhấn Escape hoặc click bên ngoài để hủy",
                    badge: "CHỈ MÀN HÌNH",
                    display_title: "Hãy dùng Quay màn hình",
                    display_message: "Không thể quay ổn định cửa sổ toàn màn hình hoặc trình chiếu này như một cửa sổ riêng. Hãy chọn Quay màn hình.",
                    display_action: "Quay lại danh sách cửa sổ",
                },
                "Đang suy nghĩ...",
                "Hủy",
                "Chọn Ứng Dụng",
            ),
            "ko" => Self::new(
                "ko",
                SelectorLocaleText {
                    title: "녹화할 창 선택",
                    subtitle: "Escape 또는 바깥 클릭으로 취소",
                    badge: "화면만",
                    display_title: "화면 캡처를 사용하세요",
                    display_message: "이 전체 화면 또는 프레젠테이션 창은 개별 창으로 안정적으로 녹화할 수 없습니다. 대신 화면 캡처를 선택하세요.",
                    display_action: "창 목록으로 돌아가기",
                },
                "생각 중...",
                "취소",
                "앱 선택",
            ),
            _ => Self::new(
                "en",
                SelectorLocaleText {
                    title: "Select a Window to Record",
                    subtitle: "Press Escape or click outside to cancel",
                    badge: "DISPLAY ONLY",
                    display_title: "Use Display Capture",
                    display_message: "This fullscreen or presentation window cannot be recorded reliably as an individual window. Choose Display capture instead.",
                    display_action: "Back to window list",
                },
                "Thinking...",
                "Cancel",
                "Select App to Capture",
            ),
        }
    }

    fn new(
        language: &'static str,
        selector: SelectorLocaleText,
        thinking: &'static str,
        cancel: &'static str,
        app_title: &'static str,
    ) -> Self {
        Self {
            language,
            auxiliary: AuxiliaryLocaleText {
                win_select_title: selector.title,
                win_select_subtitle: selector.subtitle,
                win_select_display_only_badge: selector.badge,
                win_select_display_only_title: selector.display_title,
                win_select_display_only_message: selector.display_message,
                win_select_display_only_action: selector.display_action,
                win_select_count: match language {
                    "vi" => "{} cửa sổ",
                    "ko" => "창 {}개",
                    _ => "{} windows",
                },
                managed_tools: ManagedToolsLocaleText {
                    tool_local_asr_worker: match language {
                        "vi" => "Bộ máy nhận dạng giọng nói cục bộ",
                        "ko" => "로컬 음성 인식 엔진",
                        _ => "Local speech recognition engine",
                    },
                    tool_onnx_runtime: match language {
                        "vi" => "Runtime ONNX + DirectML",
                        "ko" => "ONNX + DirectML 런타임",
                        _ => "ONNX + DirectML runtime",
                    },
                },
            },
            badge: badge_for(language),
            global_settings: GlobalSettingsLocaleText {
                model_thinking: thinking,
            },
            preset_basics: PresetBasicsLocaleText {
                cancel_label: cancel,
            },
            realtime: RealtimeLocaleText {
                app_select_title: app_title,
                app_select_count: match language {
                    "vi" => "{} ứng dụng",
                    "ko" => "앱 {}개",
                    _ => "{} apps",
                },
            },
            shell: ShellLocaleText {
                auto_copied_badge: match language {
                    "vi" => "Đã tự động copy",
                    "ko" => "자동으로 복사됨",
                    _ => "Auto-copied",
                },
            },
            tool_runtime: super::tool_runtime_locale::get(language),
            tts_playground: TtsPlaygroundLocaleText {
                screen_record_audio_ffmpeg_downloading: match language {
                    "vi" => "Đang tải FFmpeg để xuất âm thanh giữ nguyên cao độ",
                    "ko" => "피치 보존 오디오 내보내기를 위해 FFmpeg 다운로드 중",
                    _ => "Downloading FFmpeg for pitch-preserving audio export",
                },
                screen_record_gif_ffmpeg_downloading: match language {
                    "vi" => "Đang tải FFmpeg để xuất GIF",
                    "ko" => "GIF 내보내기를 위해 FFmpeg 다운로드 중",
                    _ => "Downloading FFmpeg for GIF export",
                },
            },
        }
    }

    pub fn hotkey_conflict_message(&self, conflict: &crate::config::HotkeyConflict) -> String {
        use crate::config::{GlobalHotkeyOwner, HotkeyConflict};
        match conflict {
            HotkeyConflict::Global { owner, hotkey_name } => {
                let owner = match (self.language, owner) {
                    ("vi", GlobalHotkeyOwner::ScreenRecord) => "Quay MH",
                    ("vi", GlobalHotkeyOwner::TranslationGummy) => "Bánh mỳ chuyển ngữ",
                    ("vi", GlobalHotkeyOwner::ComputerControl) => "Điều khiển máy tính",
                    ("ko", GlobalHotkeyOwner::ScreenRecord) => "화면 녹화",
                    ("ko", GlobalHotkeyOwner::TranslationGummy) => "통역 곤약",
                    ("ko", GlobalHotkeyOwner::ComputerControl) => "컴퓨터 제어",
                    (_, GlobalHotkeyOwner::ScreenRecord) => "Record Screen",
                    (_, GlobalHotkeyOwner::TranslationGummy) => "Translation Gummy",
                    (_, GlobalHotkeyOwner::ComputerControl) => "Computer Control",
                };
                match self.language {
                    "vi" => format!("Phím '{hotkey_name}' xung đột với phím tắt {owner}."),
                    "ko" => format!("'{hotkey_name}' 단축키가 {owner} 단축키와 충돌합니다."),
                    _ => format!("Hotkey '{hotkey_name}' conflicts with {owner}."),
                }
            }
            HotkeyConflict::Preset {
                hotkey_name,
                preset_name,
            } => match self.language {
                "vi" => format!("Phím '{hotkey_name}' xung đột với cấu hình '{preset_name}'."),
                "ko" => format!("'{hotkey_name}' 단축키가 '{preset_name}' 프리셋과 충돌합니다."),
                _ => format!("Hotkey '{hotkey_name}' conflicts with preset '{preset_name}'."),
            },
        }
    }
}

fn badge_for(language: &str) -> BadgeLocaleText {
    match language {
        "vi" => BadgeLocaleText {
            downloading_component_fmt: "Đang tải {name}",
            preparing_component_fmt: "Đang chuẩn bị gói {name} đã xác minh...",
            downloading_runtime_fmt: "Đang tải runtime {name}",
            fetching_runtime_manifest: "Đang lấy manifest runtime...",
            downloading_file_fmt: "Đang tải {name}...",
            preparing_runtime_fmt: "Đang chuẩn bị runtime {name}...",
            downloading_sherpa_runtime: "Đang tải runtime sherpa-onnx",
            required_for_offline_tts_fmt: "Cần cho TTS ngoại tuyến {name}",
            downloading_model_fmt: "Đang tải {name}",
            preparing_model_fmt: "Đang chuẩn bị {name}...",
            export_failed: "Xuất file thất bại",
            subtitles_saved: "Đã lưu phụ đề",
            feature_needs_webview2_fmt: "{name} cần WebView2 Runtime",
            feature_screen_record: "Trình ghi màn hình",
            install_webview2_hint: "Mở Công cụ đã tải, cài Microsoft Edge WebView2 Runtime rồi thử lại.",
        },
        "ko" => BadgeLocaleText {
            downloading_component_fmt: "{name} 다운로드 중",
            preparing_component_fmt: "검증된 {name} 패키지 준비 중...",
            downloading_runtime_fmt: "{name} 런타임 다운로드 중",
            fetching_runtime_manifest: "런타임 매니페스트 가져오는 중...",
            downloading_file_fmt: "{name} 다운로드 중...",
            preparing_runtime_fmt: "{name} 런타임 준비 중...",
            downloading_sherpa_runtime: "sherpa-onnx 런타임 다운로드 중",
            required_for_offline_tts_fmt: "{name} 오프라인 TTS에 필요",
            downloading_model_fmt: "{name} 다운로드 중",
            preparing_model_fmt: "{name} 준비 중...",
            export_failed: "내보내기 실패",
            subtitles_saved: "자막 저장 완료",
            feature_needs_webview2_fmt: "{name}에 WebView2 Runtime이 필요합니다",
            feature_screen_record: "화면 녹화",
            install_webview2_hint: "다운로드한 도구를 열어 Microsoft Edge WebView2 Runtime을 설치한 후 다시 시도하세요.",
        },
        _ => BadgeLocaleText {
            downloading_component_fmt: "Downloading {name}",
            preparing_component_fmt: "Preparing verified {name} package...",
            downloading_runtime_fmt: "Downloading {name} runtime",
            fetching_runtime_manifest: "Fetching runtime manifest...",
            downloading_file_fmt: "Downloading {name}...",
            preparing_runtime_fmt: "Preparing {name} runtime...",
            downloading_sherpa_runtime: "Downloading sherpa-onnx runtime",
            required_for_offline_tts_fmt: "Required for {name} offline TTS",
            downloading_model_fmt: "Downloading {name}",
            preparing_model_fmt: "Preparing {name}...",
            export_failed: "Export failed",
            subtitles_saved: "Subtitles saved",
            feature_needs_webview2_fmt: "{name} needs WebView2 Runtime",
            feature_screen_record: "Screen recorder",
            install_webview2_hint: "Open Downloaded Tools, install Microsoft Edge WebView2 Runtime, then try again.",
        },
    }
}
