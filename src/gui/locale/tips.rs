// Curation policy and cross-platform contract: .claude/parity/usage-tips.md

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UsageTipCategory {
    #[default]
    CaptureShortcuts,
    PresetsAutomation,
    ResultsRecovery,
    ModelsSearch,
    CreativeTools,
}

impl UsageTipCategory {
    pub const ALL: [Self; 5] = [
        Self::CaptureShortcuts,
        Self::PresetsAutomation,
        Self::ResultsRecovery,
        Self::ModelsSearch,
        Self::CreativeTools,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::CaptureShortcuts => "capture_shortcuts",
            Self::PresetsAutomation => "presets_automation",
            Self::ResultsRecovery => "results_recovery",
            Self::ModelsSearch => "models_search",
            Self::CreativeTools => "creative_tools",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageTip {
    pub id: &'static str,
    pub text: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageTipSection {
    pub id: UsageTipCategory,
    pub title: &'static str,
    pub description: &'static str,
    pub tips: &'static [UsageTip],
}

macro_rules! tip {
    ($id:literal, $text:literal) => {
        UsageTip {
            id: $id,
            text: $text,
        }
    };
}

const EN_CAPTURE: &[UsageTip] = &[
    tip!(
        "selection_cancel",
        "Press **Esc** to cancel a region selection. If capture began with a keyboard hotkey, pressing that hotkey again also cancels."
    ),
    tip!(
        "selection_hidden_gestures",
        "Before dragging an Image selection, use the **mouse wheel to zoom** and right-drag to pan. A click without a region copies that pixel as #RRGGBB."
    ),
    tip!(
        "selected_text_hotkey",
        "If text is already highlighted in another app, press a **Text-Select preset hotkey** to process it immediately."
    ),
    tip!(
        "mouse_button_hotkeys",
        "Preset hotkeys can be **Middle Click, Mouse Back, or Mouse Forward**, so capture actions do not need a keyboard chord."
    ),
    tip!(
        "continuous_mode_entry",
        "In **Favorite Bubble**, click to run once; hold a non-MASTER Image/Text item for Continuous Mode. Press Esc to exit."
    ),
    tip!(
        "continuous_mode_image_gestures",
        "In Image Continuous Mode, **right-drag** to capture a region. A plain right-click copies the pixel's HEX color; wheel while holding right zooms from 1× to 4×."
    ),
    tip!(
        "smart_input_routing",
        "Paste image/text into SGT or **drop a file** to open its routing workflow; video and subtitle drops open SGT Record. Explorer's Process with SGT feeds registered files through the same router."
    ),
    tip!(
        "input_history_navigation",
        "In Refine, use **Up/Down** to recall submissions. In a multiline typed prompt, use Up at the start or Down at the end; moving past newest restores your draft."
    ),
    tip!(
        "audio_auto_stop_semantics",
        "Audio **Auto-stop** waits for detected voice or sound activity, then stops after the following silence; quiet setup time alone will not stop recording."
    ),
];

const EN_PRESETS: &[UsageTip] = &[
    tip!(
        "graph_navigation",
        "On the preset graph, scroll to zoom, drag empty space to pan, double-click to fit nodes, and right-click for **Add/Delete** menus."
    ),
    tip!(
        "auto_copy_ownership",
        "Only one processing node can own **Auto-copy**. For normal hotkey/Bubble runs, Auto-paste targets the editor focused at launch; an open prompt or Refine editor receives it instead."
    ),
    tip!(
        "profile_clone_active",
        "A new **Profile** clones the active preset collection—including favorites and hotkeys—then switches to the copy, leaving the original profile unchanged."
    ),
    tip!(
        "continuous_input_replaces_group",
        "For a typed Text preset, **Continuous Input** keeps the prompt open; each submission replaces the previous result group instead of accumulating windows."
    ),
];

const EN_RESULTS: &[UsageTip] = &[
    tip!(
        "restore_closed_batches",
        "The tray can restore the **last closed overlay batch**, or several recently closed batches together."
    ),
    tip!(
        "refine_revision_history",
        "Each **Refine** creates a revision you can Undo or Redo; image follow-ups also retain the original image context."
    ),
    tip!(
        "result_geometry_memory",
        "Move or resize the first Text/Audio result and SGT **remembers that geometry for the preset**. Image presets ignore saved geometry and use the current input placement."
    ),
    tip!(
        "history_pruning",
        "Lowering **History → Max Items** immediately prunes the oldest entries and deletes their stored media."
    ),
    tip!(
        "tray_close_keeps_running",
        "Closing the main window **hides SGT to the tray**, so preset hotkeys keep working. Choose Quit from the tray to exit the app."
    ),
];

const EN_MODELS: &[UsageTip] = &[
    tip!(
        "search_marker_default",
        "A model's **magnifying-glass marker** means its web-search tool is enabled during normal use."
    ),
    tip!(
        "fallback_cooldown_search",
        "In preset Image→Text/Text→Text retries, a **rate-limited model** is skipped for five minutes. Fallback preserves model type, and search-capable models retry only through search-capable models."
    ),
];

const EN_CREATIVE: &[UsageTip] = &[tip!(
    "promptdj_midi_learn",
    "In **Be a DJ**, enable MIDI, click a prompt's CC:n badge to enter Learn, then move a hardware control to bind it to that prompt's weight."
)];

const EN: &[UsageTipSection] = &[
    UsageTipSection {
        id: UsageTipCategory::CaptureShortcuts,
        title: "Capture & shortcuts",
        description: "Hidden gestures available before processing starts.",
        tips: EN_CAPTURE,
    },
    UsageTipSection {
        id: UsageTipCategory::PresetsAutomation,
        title: "Presets & automation",
        description: "Quiet rules SGT applies behind the preset graph.",
        tips: EN_PRESETS,
    },
    UsageTipSection {
        id: UsageTipCategory::ResultsRecovery,
        title: "Results & recovery",
        description: "Recovery, revision history, and retained output behavior.",
        tips: EN_RESULTS,
    },
    UsageTipSection {
        id: UsageTipCategory::ModelsSearch,
        title: "Models & search",
        description: "Routing behavior behind model choices.",
        tips: EN_MODELS,
    },
    UsageTipSection {
        id: UsageTipCategory::CreativeTools,
        title: "Creative tools",
        description: "Hardware behavior beyond the main controls.",
        tips: EN_CREATIVE,
    },
];

const KO_CAPTURE: &[UsageTip] = &[
    tip!(
        "selection_cancel",
        "**Esc**를 누르면 영역 선택이 취소됩니다. 키보드 단축키로 캡처를 시작했다면 같은 단축키를 다시 눌러도 취소됩니다."
    ),
    tip!(
        "selection_hidden_gestures",
        "이미지 영역을 드래그하기 전에 **마우스 휠로 확대/축소**하고 오른쪽 버튼 드래그로 화면을 이동할 수 있습니다. 영역 없이 클릭하면 해당 픽셀의 #RRGGBB 색상이 복사됩니다."
    ),
    tip!(
        "selected_text_hotkey",
        "다른 앱에서 텍스트를 미리 선택해 두었다면 **단축키 후 텍스트 선택** 방식의 프리셋 단축키를 눌러 바로 처리할 수 있습니다."
    ),
    tip!(
        "mouse_button_hotkeys",
        "프리셋 단축키에는 **Middle Click, Mouse Back, Mouse Forward**도 지정할 수 있어 키보드 조합 없이 캡처 작업을 실행할 수 있습니다."
    ),
    tip!(
        "continuous_mode_entry",
        "**즐겨찾기 버블**에서 클릭하면 한 번 실행하고, 마스터가 아닌 이미지/텍스트 항목을 길게 누르면 연속 모드로 들어갑니다. Esc를 눌러 종료하세요."
    ),
    tip!(
        "continuous_mode_image_gestures",
        "이미지 연속 모드에서는 **오른쪽 버튼으로 드래그**해 영역을 캡처합니다. 짧게 오른쪽 클릭하면 픽셀의 HEX 색상이 복사되고, 오른쪽 버튼을 누른 채 휠을 돌리면 1배에서 4배까지 확대됩니다."
    ),
    tip!(
        "smart_input_routing",
        "이미지/텍스트를 SGT에 붙여넣거나 **파일을 드롭**하면 라우팅 작업이 열립니다. 비디오와 자막 드롭은 SGT Record를 열고, 탐색기의 Process with SGT는 등록된 파일을 같은 라우터로 전달합니다."
    ),
    tip!(
        "input_history_navigation",
        "다듬기에서는 **위/아래 화살표**로 이전에 제출한 내용을 다시 불러올 수 있습니다. 여러 줄 입력 프롬프트에서는 커서가 맨 앞에 있을 때 위쪽 화살표를, 맨 끝에 있을 때 아래쪽 화살표를 누르세요. 최신 항목을 지나면 작성 중이던 초안이 복원됩니다."
    ),
    tip!(
        "audio_auto_stop_semantics",
        "오디오 **자동 중지**는 먼저 음성이나 소리를 감지할 때까지 기다린 뒤, 그다음 침묵이 이어지면 녹음을 끝냅니다. 따라서 준비 중에 조용한 시간만 흘러서는 녹음이 멈추지 않습니다."
    ),
];

const KO_PRESETS: &[UsageTip] = &[
    tip!(
        "graph_navigation",
        "프리셋 그래프에서 스크롤로 확대/축소하고, 빈 공간을 드래그해 이동하고, 더블 클릭해 노드를 화면에 맞추고, 오른쪽 클릭으로 **추가/삭제** 메뉴를 여세요."
    ),
    tip!(
        "auto_copy_ownership",
        "처리 노드 하나만 **자동 복사**를 사용할 수 있습니다. 일반 단축키/버블 실행에서는 시작할 때 포커스된 편집기로 자동 붙여넣기하며, 열린 프롬프트 또는 다듬기 편집기가 있으면 그곳이 우선합니다."
    ),
    tip!(
        "profile_clone_active",
        "새 **프로필**은 즐겨찾기와 단축키를 포함한 현재 프리셋 모음을 복제한 뒤 그 복사본으로 전환하므로 원본 프로필은 바뀌지 않습니다."
    ),
    tip!(
        "continuous_input_replaces_group",
        "텍스트 프리셋의 작동 방식을 '단축키 후 입력'으로 설정하고 **연속 입력**을 켜면 입력창이 계속 열려 있고, 제출할 때마다 새 창을 쌓는 대신 이전 결과 그룹을 대체합니다."
    ),
];

const KO_RESULTS: &[UsageTip] = &[
    tip!(
        "restore_closed_batches",
        "트레이에서 **마지막으로 닫은 오버레이 묶음** 또는 최근에 닫은 여러 묶음을 함께 복원할 수 있습니다."
    ),
    tip!(
        "refine_revision_history",
        "**다듬기**를 실행할 때마다 실행 취소/다시 실행이 가능한 새 리비전이 생기며, 이미지 후속 요청은 원본 이미지 문맥도 유지합니다."
    ),
    tip!(
        "result_geometry_memory",
        "첫 텍스트/오디오 결과를 이동하거나 크기를 바꾸면 SGT가 **해당 프리셋의 위치와 크기**를 기억합니다. 이미지 프리셋은 저장된 위치와 크기를 사용하지 않고 현재 입력 위치에 맞춰 배치됩니다."
    ),
    tip!(
        "history_pruning",
        "**히스토리 → 저장 한도**를 낮추면 가장 오래된 항목이 즉시 정리되고 저장된 미디어 파일도 삭제됩니다."
    ),
    tip!(
        "tray_close_keeps_running",
        "메인 창을 닫아도 SGT는 **트레이로 숨겨지므로** 프리셋 단축키가 계속 작동합니다. 앱을 종료하려면 트레이에서 종료를 선택하세요."
    ),
];

const KO_MODELS: &[UsageTip] = &[
    tip!(
        "search_marker_default",
        "모델의 **돋보기 표시**는 일반 사용 시 해당 모델의 웹 검색 도구가 활성화된다는 뜻입니다."
    ),
    tip!(
        "fallback_cooldown_search",
        "프리셋 이미지→텍스트/텍스트→텍스트 재시도에서 **요청 한도에 걸린 모델**은 5분 동안 건너뜁니다. 폴백은 모델 유형을 유지하며, 검색 지원 모델은 검색 지원 모델만 거쳐 재시도합니다."
    ),
];

const KO_CREATIVE: &[UsageTip] = &[tip!(
    "promptdj_midi_learn",
    "**DJ 되기**에서 MIDI를 켜고 프롬프트의 CC:n 배지를 클릭해 학습 모드로 들어간 다음, 하드웨어 컨트롤을 움직여 프롬프트 가중치에 연결하세요."
)];

const KO: &[UsageTipSection] = &[
    UsageTipSection {
        id: UsageTipCategory::CaptureShortcuts,
        title: "캡처 및 단축키",
        description: "처리를 시작하기 전에 쓸 수 있는 숨은 동작입니다.",
        tips: KO_CAPTURE,
    },
    UsageTipSection {
        id: UsageTipCategory::PresetsAutomation,
        title: "프리셋 및 자동화",
        description: "프리셋 그래프 뒤에서 SGT가 적용하는 규칙입니다.",
        tips: KO_PRESETS,
    },
    UsageTipSection {
        id: UsageTipCategory::ResultsRecovery,
        title: "결과 및 복구",
        description: "복구, 리비전 기록, 결과 보관 방식입니다.",
        tips: KO_RESULTS,
    },
    UsageTipSection {
        id: UsageTipCategory::ModelsSearch,
        title: "모델 및 검색",
        description: "모델 선택 뒤에서 작동하는 라우팅 규칙입니다.",
        tips: KO_MODELS,
    },
    UsageTipSection {
        id: UsageTipCategory::CreativeTools,
        title: "창작 도구",
        description: "기본 컨트롤 너머의 하드웨어 동작입니다.",
        tips: KO_CREATIVE,
    },
];

const VI_CAPTURE: &[UsageTip] = &[
    tip!(
        "selection_cancel",
        "Nhấn **Esc** để hủy chọn vùng. Nếu bắt đầu chụp bằng phím tắt bàn phím, nhấn lại phím tắt đó cũng sẽ hủy."
    ),
    tip!(
        "selection_hidden_gestures",
        "Trước khi kéo để chọn vùng cho preset Ảnh, dùng **con lăn chuột để thu phóng** và kéo chuột phải để di chuyển khung nhìn. Nhấp mà không chọn vùng sẽ sao chép màu #RRGGBB của pixel đó."
    ),
    tip!(
        "selected_text_hotkey",
        "Nếu văn bản đã được bôi đen trong ứng dụng khác, nhấn phím tắt của preset Văn bản ở chế độ **Hotkey rồi bôi text** để xử lý ngay."
    ),
    tip!(
        "mouse_button_hotkeys",
        "Phím tắt preset có thể là **Middle Click, Mouse Back hoặc Mouse Forward**, nên bạn có thể chạy thao tác chụp mà không cần tổ hợp phím trên bàn phím."
    ),
    tip!(
        "continuous_mode_entry",
        "Trong **Bong bóng yêu thích**, nhấp để chạy một lần; nhấn giữ mục Ảnh/Văn bản không phải MASTER để vào Chế độ liên tục. Nhấn Esc để thoát."
    ),
    tip!(
        "continuous_mode_image_gestures",
        "Trong Chế độ liên tục của Ảnh, **kéo chuột phải** để chụp một vùng. Nhấp chuột phải sẽ copy màu HEX của pixel; giữ chuột phải và cuộn để zoom từ 1× đến 4×."
    ),
    tip!(
        "smart_input_routing",
        "Dán ảnh/văn bản vào SGT hoặc **thả tệp** để mở quy trình định tuyến; tệp video và phụ đề sẽ mở SGT Record. Process with SGT trong Explorer đưa tệp đã đăng ký qua cùng bộ định tuyến."
    ),
    tip!(
        "input_history_navigation",
        "Trong Viết lại, dùng **Lên/Xuống** để gọi lại nội dung đã gửi. Với prompt nhiều dòng, dùng Lên ở đầu hoặc Xuống ở cuối; đi qua mục mới nhất sẽ khôi phục bản nháp."
    ),
    tip!(
        "audio_auto_stop_semantics",
        "**Tự động dừng** chờ đến khi phát hiện giọng nói hoặc âm thanh, rồi dừng ghi âm sau khoảng lặng tiếp theo; nếu chỉ yên tĩnh trong lúc chuẩn bị thì ghi âm sẽ không dừng."
    ),
];

const VI_PRESETS: &[UsageTip] = &[
    tip!(
        "graph_navigation",
        "Trên sơ đồ preset, cuộn để zoom, kéo vùng trống để pan, nhấp đúp để căn vừa các node, và nhấp phải để mở menu **Thêm/Xóa**."
    ),
    tip!(
        "auto_copy_ownership",
        "Chỉ có thể bật **Tự động copy** trên một node xử lý. Với lần chạy thông thường bằng phím tắt hoặc Bong bóng yêu thích, Tự động dán sẽ gửi kết quả đến ô soạn thảo đang có tiêu điểm khi bắt đầu; nếu prompt hoặc ô Viết lại đang mở thì kết quả được dán vào đó."
    ),
    tip!(
        "profile_clone_active",
        "**Hồ sơ** mới sao chép bộ preset đang dùng, gồm cả mục yêu thích và phím tắt, rồi chuyển sang bản sao nên hồ sơ gốc không thay đổi."
    ),
    tip!(
        "continuous_input_replaces_group",
        "Với preset Văn bản ở chế độ “Hotkey rồi gõ”, **Nhập liên tục** giữ ô prompt mở; mỗi lần gửi sẽ thay thế nhóm kết quả trước đó thay vì mở thêm cửa sổ."
    ),
];

const VI_RESULTS: &[UsageTip] = &[
    tip!(
        "restore_closed_batches",
        "Từ khay hệ thống, bạn có thể **khôi phục nhóm overlay vừa đóng** hoặc nhiều nhóm đã đóng gần đây cùng lúc."
    ),
    tip!(
        "refine_revision_history",
        "Mỗi lần **Viết lại** tạo một phiên bản có thể Hoàn tác/Làm lại; yêu cầu tiếp theo cho ảnh cũng giữ ngữ cảnh của ảnh gốc."
    ),
    tip!(
        "result_geometry_memory",
        "Khi bạn di chuyển hoặc đổi kích thước kết quả Văn bản/Âm thanh đầu tiên, SGT sẽ **nhớ vị trí và kích thước cho preset đó**. Preset Ảnh bỏ qua vị trí và kích thước đã lưu, rồi dùng vị trí của đầu vào hiện tại."
    ),
    tip!(
        "history_pruning",
        "Giảm **Lịch sử → Giới hạn lưu** sẽ xóa ngay các mục cũ nhất và file media đã lưu của chúng."
    ),
    tip!(
        "tray_close_keeps_running",
        "Đóng cửa sổ chính chỉ **ẩn SGT vào khay hệ thống**, nên phím tắt preset vẫn hoạt động. Chọn Thoát trong khay để tắt ứng dụng."
    ),
];

const VI_MODELS: &[UsageTip] = &[
    tip!(
        "search_marker_default",
        "Dấu **kính lúp trên một mô hình** cho biết công cụ tìm kiếm web của mô hình đó được bật khi sử dụng bình thường."
    ),
    tip!(
        "fallback_cooldown_search",
        "Khi preset Ảnh → Text/Text → Text thử lại, **mô hình gặp giới hạn tần suất** sẽ bị bỏ qua trong 5 phút. Fallback giữ nguyên loại mô hình; mô hình hỗ trợ tìm kiếm chỉ thử lại qua các mô hình cũng hỗ trợ tìm kiếm."
    ),
];

const VI_CREATIVE: &[UsageTip] = &[tip!(
    "promptdj_midi_learn",
    "Trong **Làm DJ**, bật MIDI, nhấp huy hiệu CC:n của prompt để vào chế độ Học, rồi di chuyển bộ điều khiển phần cứng để gán cho trọng số prompt."
)];

const VI: &[UsageTipSection] = &[
    UsageTipSection {
        id: UsageTipCategory::CaptureShortcuts,
        title: "Chụp & phím tắt",
        description: "Các thao tác ẩn trước khi bắt đầu xử lý.",
        tips: VI_CAPTURE,
    },
    UsageTipSection {
        id: UsageTipCategory::PresetsAutomation,
        title: "Preset & tự động hóa",
        description: "Các quy tắc SGT áp dụng phía sau sơ đồ preset.",
        tips: VI_PRESETS,
    },
    UsageTipSection {
        id: UsageTipCategory::ResultsRecovery,
        title: "Kết quả & khôi phục",
        description: "Khôi phục, lịch sử phiên bản và cách lưu giữ kết quả.",
        tips: VI_RESULTS,
    },
    UsageTipSection {
        id: UsageTipCategory::ModelsSearch,
        title: "Mô hình & tìm kiếm",
        description: "Cách SGT định tuyến phía sau lựa chọn model.",
        tips: VI_MODELS,
    },
    UsageTipSection {
        id: UsageTipCategory::CreativeTools,
        title: "Công cụ sáng tạo",
        description: "Hành vi phần cứng ngoài các nút chính.",
        tips: VI_CREATIVE,
    },
];

pub(super) const fn en() -> &'static [UsageTipSection] {
    EN
}

pub(super) const fn ko() -> &'static [UsageTipSection] {
    KO
}

pub(super) const fn vi() -> &'static [UsageTipSection] {
    VI
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{EN, KO, UsageTipCategory, UsageTipSection, VI};

    #[test]
    fn localized_tip_catalogs_have_matching_semantic_structure() {
        let expected_categories = UsageTipCategory::ALL;
        let expected_ids = tip_ids(EN);

        for (language, sections) in [("en", EN), ("ko", KO), ("vi", VI)] {
            assert_eq!(sections.len(), expected_categories.len());
            assert_eq!(
                sections
                    .iter()
                    .map(|section| section.id)
                    .collect::<Vec<_>>(),
                expected_categories,
                "{language} category order drifted"
            );
            assert_eq!(
                tip_ids(sections),
                expected_ids,
                "{language} tip ID/order drifted"
            );
        }
    }

    #[test]
    fn localized_tip_catalogs_are_unique_and_well_formed() {
        for (language, sections) in [("en", EN), ("ko", KO), ("vi", VI)] {
            let mut ids = HashSet::new();
            let mut text = HashSet::new();

            for section in sections {
                assert!(!section.title.trim().is_empty(), "{language} empty title");
                assert!(
                    !section.description.trim().is_empty(),
                    "{language} empty description"
                );
                assert!(
                    !section.tips.is_empty(),
                    "{language} has an empty category: {}",
                    section.id.stable_id()
                );
                for tip in section.tips {
                    assert!(ids.insert(tip.id), "{language} duplicate ID: {}", tip.id);
                    assert!(
                        text.insert(tip.text),
                        "{language} contains duplicate tip text"
                    );
                    assert!(!tip.text.trim().is_empty(), "{language} empty tip");
                    assert_eq!(
                        tip.text.matches("**").count() % 2,
                        0,
                        "{language} contains unbalanced bold markers: {}",
                        tip.text
                    );
                }
            }
        }
    }

    #[test]
    fn windows_catalog_matches_the_shared_parity_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/mobile-shell/usage-tips.json"
        ))
        .expect("valid usage-tips fixture");
        let fixture_categories = fixture["catalog_contract"]["categories"]
            .as_array()
            .expect("fixture category array");

        assert_eq!(fixture_categories.len(), EN.len());
        for (index, fixture_category) in fixture_categories.iter().enumerate() {
            let expected_id = fixture_category["id"].as_str().expect("category ID");
            assert_eq!(EN[index].id.stable_id(), expected_id);
            for (language, sections) in [("en", EN), ("ko", KO), ("vi", VI)] {
                assert_eq!(
                    sections[index].title,
                    fixture_category["labels"][language]
                        .as_str()
                        .expect("localized category label"),
                    "{language} fixture label drifted"
                );
            }
        }

        let windows_case = fixture["cases"]
            .as_array()
            .expect("fixture cases")
            .iter()
            .find(|case| case["name"] == "windows_static_entry_contract")
            .expect("Windows tips fixture case");
        let expected_tip_ids = windows_case["required_tip_ids"]
            .as_array()
            .expect("Windows required tip IDs")
            .iter()
            .map(|id| id.as_str().expect("tip ID"))
            .collect::<Vec<_>>();
        assert_eq!(tip_ids(EN), expected_tip_ids);
    }

    fn tip_ids(sections: &[UsageTipSection]) -> Vec<&str> {
        sections
            .iter()
            .flat_map(|section| section.tips.iter().map(|tip| tip.id))
            .collect()
    }
}
