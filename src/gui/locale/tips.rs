const TIP_COUNT: usize = 25;

const EN: [&str; TIP_COUNT] = [
    "While choosing a screen region, press **Esc** or the same preset hotkey again to cancel.",
    "If text is already highlighted in another app, press a **Text-Select** preset hotkey to process it immediately.",
    "Preset hotkeys can use **Middle Click**, **Mouse Back**, or **Mouse Forward**. Assign one to any **MASTER** preset to open its matching preset wheel.",
    "Star presets for **Favorite Bubble**, then enable it from the system tray. Hold a non-MASTER Image or Text hotkey—or its bubble item—to enter Continuous Mode; press Esc or the same hotkey to exit.",
    "**Drop files onto SGT** or press Ctrl+V with a clipboard image or text. Images and text open a preset wheel; audio can use a preset or Record Screen, while video and subtitle files open Record Screen actions.",
    "On a result overlay, left, right, or middle click closes **one, its group, or all**; dragging with those buttons moves the same scope.",
    "Closed a result by mistake? Use the tray menu to **restore the last closed overlay batch** or a recent batch.",
    "On a result, use **Edit / Refine** for follow-up instructions, or **Toggle Markdown** to switch between plain text and rendered output.",
    "If result overlays feel slow, try **Graphics Mode → Minimal**.",
    "On the preset graph, scroll to zoom, drag empty space to pan, double-click to fit nodes, and right-click for **Add/Delete** menus.",
    "The output mode beside the eye offers **Normal, Stream, Markdown, and MD+Stream**.",
    "A model's magnifying-glass marker means its normal selection runs **web search by default**.",
    "Only one processing step can own **Auto-copy**. With Controller off, it reveals Auto-paste and Add newline; Auto-paste still needs a blinking caret in the target app.",
    "Choose **Restore** on a built-in preset to reset its settings without losing its hotkeys; custom preset names remain editable.",
    "Use **Profiles** to keep separate preset collections; a new profile copies the active one so you can customize it safely.",
    "Use **History** search to find earlier results. Lowering Max Items prunes the oldest entries, while Computer Control memory has its own limit.",
    "For non-realtime audio presets, enable **Auto-stop** to finish recording after silence.",
    "In **Model Priority**, put preferred models first and leave Auto to continue through smart fallbacks; unavailable providers or bad or missing keys are skipped.",
    "Use **Custom Models** to scan OpenRouter or Ollama. Use **Downloaded Tools** to install or remove local models, runtimes, Record Screen backgrounds, and pointer collections.",
    "Voice Settings selects the TTS method and per-language voices or accents for Presets, Live Translate, and Translation Gummy; Live Translate's **AUTO** speed catches up with the speaker.",
    "Use **TTS Playground** to test or clone voices, edit audio, and export WAV or MP3.",
    "In **Download Video**, Advanced Features controls metadata, SponsorBlock, subtitles, playlists, and browser cookies.",
    "The footer opens **Computer Control, Pointer Gallery, Translation Gummy, TTS Playground, Image to 3D, Image to SVG, Be a DJ, Download Video, and Record Screen** directly.",
    "In **Be a DJ**, enable MIDI and map MIDI CC controls to the prompt-weight knobs.",
    "If you are unsure which workflow to use, open **How to use** in Global Settings and ask in your own words.",
];

const KO: [&str; TIP_COUNT] = [
    "어두워진 화면에서 영역을 선택하는 중에는 **Esc** 또는 같은 프리셋 단축키를 다시 눌러 취소할 수 있습니다.",
    "다른 앱에서 텍스트가 이미 선택되어 있다면 **텍스트 선택** 프리셋 단축키를 눌러 바로 처리할 수 있습니다.",
    "프리셋 단축키에는 **가운데 클릭, 마우스 뒤로, 마우스 앞으로**도 지정할 수 있습니다. **마스터 프리셋**에 지정하면 해당 프리셋 선택 휠이 열립니다.",
    "자주 쓰는 프리셋을 **즐겨찾기 버블**에 추가하고 트레이에서 버블을 켜세요. 마스터가 아닌 이미지 또는 텍스트 단축키나 버블 항목을 길게 누르면 연속 모드로 들어가며, Esc 또는 같은 단축키로 종료합니다.",
    "SGT에 **파일을 드롭**하거나 클립보드 이미지 또는 텍스트를 Ctrl+V로 붙여넣으세요. 이미지와 텍스트는 프리셋 휠을 열고, 오디오는 프리셋 또는 화면 녹화로, 비디오와 자막은 화면 녹화 작업으로 연결됩니다.",
    "결과 오버레이를 왼쪽, 오른쪽, 가운데 버튼으로 클릭하면 각각 **하나, 그룹, 전체**를 닫고, 같은 버튼으로 드래그하면 같은 범위를 이동합니다.",
    "결과를 실수로 닫았다면 트레이 메뉴에서 **마지막으로 닫은 오버레이 묶음**이나 최근 묶음을 복원하세요.",
    "결과에서 **편집 / 다듬기**로 후속 지시를 입력하거나 **마크다운 토글**로 일반 텍스트와 렌더링된 결과를 전환하세요.",
    "결과 오버레이가 느리다면 **그래픽 모드 → 최소**로 바꿔 보세요.",
    "프리셋 그래프에서 스크롤로 확대/축소하고, 빈 공간을 드래그해 이동하고, 더블 클릭해 노드를 화면에 맞추고, 오른쪽 클릭으로 **추가/삭제** 메뉴를 여세요.",
    "눈 아이콘 옆의 출력 모드에는 **일반, 스트림, 마크다운, 마크다운+스트림**이 있습니다.",
    "모델의 돋보기 표시는 일반 선택 시 **웹 검색이 기본으로 실행됨**을 뜻합니다.",
    "처리 단계 하나만 **자동 복사**를 사용할 수 있습니다. 컨트롤러가 꺼져 있으면 자동 붙여넣기와 줄바꿈 추가가 나타나며, 자동 붙여넣기는 대상 앱의 깜빡이는 텍스트 커서가 필요합니다.",
    "기본 제공 프리셋에서 **복원**을 누르면 단축키를 유지한 채 설정을 초기화합니다. 사용자 프리셋 이름은 계속 편집할 수 있습니다.",
    "**프로필**로 작업별 프리셋 모음을 분리하세요. 새 프로필은 현재 프로필을 복사하므로 안전하게 바꿀 수 있습니다.",
    "**기록** 검색으로 이전 결과를 찾을 수 있습니다. 최대 항목 수를 줄이면 오래된 항목부터 정리되며, 컴퓨터 제어 메모리는 별도 한도를 사용합니다.",
    "비실시간 오디오 프리셋에서 **자동 중지**를 켜면 침묵을 감지한 뒤 녹음을 마칩니다.",
    "**모델 우선순위**에서 선호 모델을 먼저 두고 자동을 남겨 스마트 폴백을 계속하세요. 비활성 공급자와 누락되거나 잘못된 키는 건너뜁니다.",
    "**사용자 모델**에서 OpenRouter나 Ollama를 검색하세요. **다운로드된 도구**에서는 로컬 모델, 런타임, 화면 녹화 배경, 포인터 컬렉션을 설치하거나 삭제할 수 있습니다.",
    "음성 설정에서는 프리셋, 실시간 음성 번역, 통역 곤약에 사용할 TTS 방식과 언어별 음성 또는 억양을 고를 수 있습니다. 실시간 음성 번역의 **자동** 속도는 화자의 속도를 따라갑니다.",
    "**TTS 플레이그라운드**에서 음성을 시험하거나 클론하고, 오디오를 편집한 뒤 WAV 또는 MP3로 내보낼 수 있습니다.",
    "**비디오 다운로드**의 고급 기능에서 메타데이터, SponsorBlock, 자막, 재생 목록, 브라우저 쿠키를 설정할 수 있습니다.",
    "푸터에서 **컴퓨터 제어, 포인터 갤러리, 통역 곤약, TTS 플레이그라운드, 이미지를 3D로, SVG 변환, DJ 되기, 비디오 다운로드, 화면 녹화**를 바로 열 수 있습니다.",
    "**DJ 되기**에서 MIDI를 켜고 MIDI CC 컨트롤을 프롬프트 가중치 노브에 연결할 수 있습니다.",
    "어떤 작업 흐름을 써야 할지 모르겠다면 전역 설정에서 **사용법 문의**를 열고 원하는 작업을 자연스럽게 물어보세요.",
];

const VI: [&str; TIP_COUNT] = [
    "Khi đang chọn vùng trên màn hình tối, nhấn **Esc** hoặc nhấn lại phím tắt preset để hủy.",
    "Nếu văn bản đã được bôi đen trong ứng dụng khác, nhấn phím tắt của preset **chọn văn bản** để xử lý ngay.",
    "Phím tắt preset có thể dùng **chuột giữa, nút Quay lại hoặc nút Tiến**. Gán một nút cho **preset MASTER** để mở vòng chọn preset tương ứng.",
    "Đánh dấu preset cho **bong bóng yêu thích** rồi bật bong bóng từ khay hệ thống. Nhấn giữ phím tắt Ảnh hoặc Văn bản không phải MASTER, hoặc mục trong bong bóng, để vào Chế độ liên tục; nhấn Esc hoặc phím đó để thoát.",
    "**Thả tệp vào SGT** hoặc nhấn Ctrl+V với ảnh hay văn bản trong clipboard. Ảnh và văn bản mở vòng chọn preset; âm thanh có thể vào preset hoặc Quay màn hình, còn video và phụ đề mở tác vụ Quay màn hình.",
    "Nhấp nút trái, phải hoặc giữa trên cửa sổ kết quả để đóng **một cửa sổ, cả nhóm hoặc tất cả**; kéo bằng các nút đó để di chuyển cùng phạm vi.",
    "Lỡ đóng kết quả? Dùng menu khay hệ thống để **khôi phục nhóm overlay vừa đóng** hoặc một nhóm gần đây.",
    "Trong cửa sổ kết quả, dùng **Chỉnh sửa / Viết lại** để nhập yêu cầu tiếp theo hoặc **Bật/Tắt Markdown** để đổi giữa văn bản thường và bản trình bày có định dạng.",
    "Nếu cửa sổ kết quả chạy chậm, hãy thử **Chế độ đồ họa → Tối giản**.",
    "Trên sơ đồ preset, cuộn để zoom, kéo vùng trống để pan, nhấp đúp để căn vừa các node, và nhấp phải để mở menu **Thêm/Xóa**.",
    "Nút chế độ cạnh biểu tượng con mắt có **Thường, Stream, Đẹp và Đẹp+Str**.",
    "Dấu kính lúp cạnh mô hình nghĩa là khi chọn bình thường, mô hình sẽ **tìm kiếm web theo mặc định**.",
    "Chỉ một bước xử lý có thể dùng **Tự động copy**. Khi Bộ điều khiển tắt, tùy chọn này hiện Tự động dán và Thêm dòng mới; Tự động dán vẫn cần con trỏ văn bản đang nhấp nháy ở ứng dụng đích.",
    "Chọn **Khôi phục** trên preset tích hợp để đặt lại cài đặt mà vẫn giữ phím tắt. Tên preset tùy chỉnh vẫn có thể chỉnh sửa.",
    "Dùng **Hồ sơ** để tách các bộ preset theo công việc. Hồ sơ mới sao chép hồ sơ hiện tại để bạn tùy chỉnh an toàn.",
    "Dùng tìm kiếm trong **Lịch sử** để tìm kết quả cũ. Giảm Giới hạn mục sẽ dọn các mục lâu nhất; bộ nhớ Điều khiển máy tính có giới hạn riêng.",
    "Với preset âm thanh không chạy thời gian thực, bật **Tự động dừng** để kết thúc ghi âm sau khi phát hiện im lặng.",
    "Trong **Ưu tiên mô hình**, đặt mô hình muốn dùng lên đầu và giữ Tự động để tiếp tục fallback thông minh; nhà cung cấp tắt hoặc khóa thiếu hay sai sẽ bị bỏ qua.",
    "Dùng **Tùy chỉnh mô hình** để quét OpenRouter hoặc Ollama. Trong **Công cụ đã tải**, bạn có thể cài hoặc xóa mô hình cục bộ, runtime, nền Quay màn hình và bộ con trỏ.",
    "Cài đặt giọng đọc chọn phương thức TTS cùng giọng hoặc giọng vùng theo từng ngôn ngữ cho preset, Dịch cabin và Bánh mỳ chuyển ngữ; tốc độ **Tự động** của Dịch cabin sẽ bám theo nhịp nói.",
    "Dùng **Sân chơi TTS** để thử hoặc clone giọng, chỉnh audio và xuất WAV hoặc MP3.",
    "**Tải video** có các tùy chọn nâng cao cho metadata, SponsorBlock, phụ đề, playlist và cookie trình duyệt.",
    "Thanh dưới mở nhanh **Điều khiển máy tính, Kho trỏ chuột, Bánh mỳ chuyển ngữ, Sân chơi TTS, Ảnh sang 3D, Ảnh sang SVG, Làm DJ, Tải video và Quay màn hình**.",
    "Trong **Làm DJ**, bật MIDI rồi gán các điều khiển MIDI CC cho núm trọng số prompt.",
    "Nếu chưa biết nên dùng quy trình nào, mở **Hỏi cách dùng** trong Cài đặt chung và mô tả điều bạn muốn làm.",
];

pub(super) fn en() -> Vec<&'static str> {
    EN.to_vec()
}

pub(super) fn ko() -> Vec<&'static str> {
    KO.to_vec()
}

pub(super) fn vi() -> Vec<&'static str> {
    VI.to_vec()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{EN, KO, VI};

    #[test]
    fn localized_tip_catalogs_are_complete_and_well_formed() {
        for (language, tips) in [("en", &EN), ("ko", &KO), ("vi", &VI)] {
            assert!(
                tips.iter().all(|tip| !tip.trim().is_empty()),
                "{language} contains an empty tip"
            );
            assert_eq!(
                tips.iter().copied().collect::<HashSet<_>>().len(),
                tips.len(),
                "{language} contains duplicate tips"
            );
            for tip in tips {
                assert_eq!(
                    tip.matches("**").count() % 2,
                    0,
                    "{language} contains unbalanced bold markers: {tip}"
                );
            }
        }
    }
}
