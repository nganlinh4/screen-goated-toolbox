export type Language = "en" | "ko" | "vi";

const messages = {
  en: {
    title: "Image to SVG", ready: "Ready", preparing: "Preparing", oneWorker: "1 worker ready",
    queue: "Queue", addImages: "Add images", emptyQueue: "Add images to begin", source: "Source image",
    model: "Model", simple: "Simple", simpleHint: "Clean shapes, smaller file", detail: "Detail",
    detailHint: "More paths and fine edges", saveTo: "Save to", generate: "Create SVG", cancel: "Cancel",
    canvasEmpty: "Your vector will appear here", canvasHint: "Choose one or more images to start",
    queued: "Queued", creating: "Drawing vector paths", done: "Vector ready", failed: "Could not create vector",
    cancelled: "Cancelled", openFolder: "Show in folder", paths: "paths", minimize: "Minimize", close: "Close",
    selectJob: "Select a result to inspect it", selected: "selected",
  },
  ko: {
    title: "SVG 변환", ready: "준비됨", preparing: "준비 중", oneWorker: "작업자 1개 준비됨",
    queue: "대기열", addImages: "이미지 추가", emptyQueue: "이미지를 추가해 시작하세요", source: "원본 이미지",
    model: "모델", simple: "간단", simpleHint: "깔끔한 형태, 작은 파일", detail: "상세",
    detailHint: "더 많은 경로와 세밀한 가장자리", saveTo: "저장 위치", generate: "SVG 만들기", cancel: "취소",
    canvasEmpty: "벡터 결과가 여기에 표시됩니다", canvasHint: "이미지를 하나 이상 선택하세요",
    queued: "대기 중", creating: "벡터 경로 그리는 중", done: "벡터 준비됨", failed: "벡터를 만들지 못했습니다",
    cancelled: "취소됨", openFolder: "폴더에서 보기", paths: "개 경로", minimize: "최소화", close: "닫기",
    selectJob: "결과를 선택해 확인하세요", selected: "개 선택됨",
  },
  vi: {
    title: "Ảnh sang SVG", ready: "Sẵn sàng", preparing: "Đang chuẩn bị", oneWorker: "Sẵn sàng 1 luồng",
    queue: "Hàng đợi", addImages: "Thêm ảnh", emptyQueue: "Thêm ảnh để bắt đầu", source: "Ảnh nguồn",
    model: "Mô hình", simple: "Đơn giản", simpleHint: "Hình gọn, tệp nhẹ", detail: "Chi tiết",
    detailHint: "Nhiều đường nét và cạnh mịn hơn", saveTo: "Lưu vào", generate: "Tạo SVG", cancel: "Hủy",
    canvasEmpty: "Ảnh vector sẽ hiện ở đây", canvasHint: "Chọn một hoặc nhiều ảnh để bắt đầu",
    queued: "Đang chờ", creating: "Đang vẽ đường vector", done: "Vector đã sẵn sàng", failed: "Không thể tạo vector",
    cancelled: "Đã hủy", openFolder: "Hiện trong thư mục", paths: "đường", minimize: "Thu nhỏ", close: "Đóng",
    selectJob: "Chọn một kết quả để xem", selected: "đã chọn",
  },
} as const;

let language: Language = "en";
export function setLanguage(value?: string) {
  language = value?.toLowerCase().startsWith("ko") ? "ko" : value?.toLowerCase().startsWith("vi") ? "vi" : "en";
}
export function t(key: keyof typeof messages.en): string { return messages[language][key]; }
