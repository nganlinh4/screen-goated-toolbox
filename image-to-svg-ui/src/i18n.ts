export type Language = "en" | "ko" | "vi";

const messages = {
  en: {
    title: "Image to SVG", ready: "Ready", preparing: "Preparing", oneWorker: "1 worker ready",
    queue: "Recent", addImages: "Add images", emptyQueue: "Add images to begin", source: "Source image",
    model: "Model", simple: "Simple", simpleHint: "Clean shapes, smaller file", detail: "Detail",
    detailHint: "More paths and fine edges", saveTo: "Save to", generate: "Create SVG", cancel: "Cancel",
    canvasEmpty: "Your vector will appear here", canvasHint: "Choose one or more images to start",
    queued: "Queued", creating: "Drawing vector paths", done: "Vector ready", failed: "Could not create vector",
    cancelled: "Cancelled", openFolder: "Show in folder", paths: "paths", minimize: "Minimize", close: "Close",
    selectJob: "Select a result to inspect it", selected: "selected", dropImages: "Drop images to add them",
    preparingWorkspace: "Preparing a vector workspace", confirmingWorkspace: "Confirming the workspace",
    workspaceReady: "Vector workspace ready", openingWorkspace: "Opening the vector workspace",
    imageReady: "Image ready", creatingPaths: "Creating vector paths", finishingVector: "Finishing the SVG",
    waitingWorkspace: "Waiting for a ready workspace", readingDepth: "Separating image depth", failedHint: "Try this image again",
    almostThere: "Almost there", lessMinute: "Less than a minute", aboutMinutes: "About {count} min left", takingLonger: "Taking a little longer",
    zoomIn: "Zoom in", zoomOut: "Zoom out", fitView: "Fit to canvas", canvasBackground: "Change canvas background", showOutlines: "Show path outlines",
    edit: "Edit", noSelection: "No shape selected", shapeSelected: "Shape {count}", fill: "Fill", stroke: "Stroke",
    removeFill: "Remove fill", removeStroke: "Remove stroke", undo: "Undo", redo: "Redo", deleteShape: "Delete shape",
    saveChanges: "Save changes", unsaved: "Unsaved", saveFailed: "Save failed", savedResult: "Saved result",
    renameResult: "Rename result", deleteResult: "Delete result", deleteResultConfirm: "Delete this result file from disk?",
    renameFailed: "Could not rename the result", deleteFailed: "Could not delete the result",
  },
  ko: {
    title: "SVG 변환", ready: "준비됨", preparing: "준비 중", oneWorker: "작업자 1개 준비됨",
    queue: "최근 항목", addImages: "이미지 추가", emptyQueue: "이미지를 추가해 시작하세요", source: "원본 이미지",
    model: "모델", simple: "간단", simpleHint: "깔끔한 형태, 작은 파일", detail: "상세",
    detailHint: "더 많은 경로와 세밀한 가장자리", saveTo: "저장 위치", generate: "SVG 만들기", cancel: "취소",
    canvasEmpty: "벡터 결과가 여기에 표시됩니다", canvasHint: "이미지를 하나 이상 선택하세요",
    queued: "대기 중", creating: "벡터 경로 그리는 중", done: "벡터 준비됨", failed: "벡터를 만들지 못했습니다",
    cancelled: "취소됨", openFolder: "폴더에서 보기", paths: "개 경로", minimize: "최소화", close: "닫기",
    selectJob: "결과를 선택해 확인하세요", selected: "개 선택됨", dropImages: "이미지를 놓아 추가하세요",
    preparingWorkspace: "벡터 작업 공간 준비 중", confirmingWorkspace: "작업 공간 확인 중",
    workspaceReady: "벡터 작업 공간 준비됨", openingWorkspace: "벡터 작업 공간 여는 중",
    imageReady: "이미지 준비됨", creatingPaths: "벡터 경로 생성 중", finishingVector: "SVG 마무리 중",
    waitingWorkspace: "준비된 작업 공간을 기다리는 중", readingDepth: "이미지 깊이 분리 중", failedHint: "이 이미지를 다시 시도하세요",
    almostThere: "거의 완료되었습니다", lessMinute: "1분 이내", aboutMinutes: "약 {count}분 남음", takingLonger: "예상보다 조금 더 걸리고 있습니다",
    zoomIn: "확대", zoomOut: "축소", fitView: "캔버스에 맞추기", canvasBackground: "캔버스 배경 변경", showOutlines: "패스 윤곽선 표시",
    edit: "편집", noSelection: "선택한 도형 없음", shapeSelected: "도형 {count}", fill: "채우기", stroke: "선",
    removeFill: "채우기 제거", removeStroke: "선 제거", undo: "실행 취소", redo: "다시 실행", deleteShape: "도형 삭제",
    saveChanges: "변경 사항 저장", unsaved: "저장되지 않음", saveFailed: "저장 실패", savedResult: "저장된 결과",
    renameResult: "결과 이름 변경", deleteResult: "결과 삭제", deleteResultConfirm: "이 결과 파일을 디스크에서 삭제할까요?",
    renameFailed: "결과 이름을 변경하지 못했습니다", deleteFailed: "결과를 삭제하지 못했습니다",
  },
  vi: {
    title: "Ảnh sang SVG", ready: "Sẵn sàng", preparing: "Đang chuẩn bị", oneWorker: "Sẵn sàng 1 luồng",
    queue: "Gần đây", addImages: "Thêm ảnh", emptyQueue: "Thêm ảnh để bắt đầu", source: "Ảnh nguồn",
    model: "Mô hình", simple: "Đơn giản", simpleHint: "Hình gọn, tệp nhẹ", detail: "Chi tiết",
    detailHint: "Nhiều đường nét và cạnh mịn hơn", saveTo: "Lưu vào", generate: "Tạo SVG", cancel: "Hủy",
    canvasEmpty: "Ảnh vector sẽ hiện ở đây", canvasHint: "Chọn một hoặc nhiều ảnh để bắt đầu",
    queued: "Đang chờ", creating: "Đang vẽ đường vector", done: "Vector đã sẵn sàng", failed: "Không thể tạo vector",
    cancelled: "Đã hủy", openFolder: "Hiện trong thư mục", paths: "đường", minimize: "Thu nhỏ", close: "Đóng",
    selectJob: "Chọn một kết quả để xem", selected: "đã chọn", dropImages: "Thả ảnh để thêm vào",
    preparingWorkspace: "Đang chuẩn bị không gian vector", confirmingWorkspace: "Đang xác nhận không gian làm việc",
    workspaceReady: "Không gian vector đã sẵn sàng", openingWorkspace: "Đang mở không gian vector",
    imageReady: "Ảnh đã sẵn sàng", creatingPaths: "Đang tạo các đường vector", finishingVector: "Đang hoàn thiện SVG",
    waitingWorkspace: "Đang chờ không gian sẵn sàng", readingDepth: "Đang tách ảnh theo chiều sâu", failedHint: "Hãy thử lại ảnh này",
    almostThere: "Sắp xong", lessMinute: "Còn dưới một phút", aboutMinutes: "Còn khoảng {count} phút", takingLonger: "Đang mất thêm một chút thời gian",
    zoomIn: "Phóng to", zoomOut: "Thu nhỏ", fitView: "Vừa khung vẽ", canvasBackground: "Đổi nền khung vẽ", showOutlines: "Hiện đường viền path",
    edit: "Chỉnh sửa", noSelection: "Chưa chọn hình", shapeSelected: "Hình {count}", fill: "Màu tô", stroke: "Nét viền",
    removeFill: "Bỏ màu tô", removeStroke: "Bỏ nét viền", undo: "Hoàn tác", redo: "Làm lại", deleteShape: "Xóa hình",
    saveChanges: "Lưu thay đổi", unsaved: "Chưa lưu", saveFailed: "Lưu thất bại", savedResult: "Kết quả đã lưu",
    renameResult: "Đổi tên kết quả", deleteResult: "Xóa kết quả", deleteResultConfirm: "Xóa tệp kết quả này khỏi ổ đĩa?",
    renameFailed: "Không thể đổi tên kết quả", deleteFailed: "Không thể xóa kết quả",
  },
} as const;

let language: Language = "en";
export function setLanguage(value?: string) {
  language = value?.toLowerCase().startsWith("ko") ? "ko" : value?.toLowerCase().startsWith("vi") ? "vi" : "en";
}
export function t(key: keyof typeof messages.en, replacements: Record<string, string | number> = {}): string {
  let value: string = messages[language][key];
  for (const [name, replacement] of Object.entries(replacements)) {
    value = value.split(`{${name}}`).join(String(replacement));
  }
  return value;
}
export type MessageKey = keyof typeof messages.en;
