export type Language = "en" | "ko" | "vi";

export interface Copy {
  title: string;
  queue: string;
  emptyQueue: string;
  newSession: string;
  newSessionHint: string;
  newImage: string;
  addImages: string;
  addReferences: string;
  dropImages: string;
  image: string;
  references: string;
  reference: string;
  referenceCount: (count: number) => string;
  referenceLimit: (count: number) => string;
  removeReference: string;
  noReferences: string;
  source: string;
  instruction: string;
  instructionHint: string;
  saveTo: string;
  change: string;
  generate: string;
  generateAgain: string;
  textOnlyTitle: string;
  textOnlyHint: string;
  before: string;
  after: string;
  comparison: string;
  queued: string;
  preparing: string;
  uploading: string;
  generating: string;
  finalizing: string;
  almostThere: string;
  lessMinute: string;
  aboutMinutes: (count: number) => string;
  takingLonger: string;
  ready: string;
  failed: string;
  cancelled: string;
  cancel: string;
  openFolder: string;
  rename: string;
  delete: string;
  deleteConfirm: string;
  renameTitle: string;
  dismiss: string;
  save: string;
  promptRequired: string;
  referenceReady: string;
  savedResult: string;
  minimize: string;
  close: string;
}

const copies: Record<Language, Copy> = {
  en: {
    title: "Create/edit image",
    queue: "Creations",
    emptyQueue: "Press + to start a new image.",
    newSession: "New image",
    newSessionHint: "Start with instructions, references, or both.",
    newImage: "New image",
    addImages: "Add images",
    addReferences: "Add references",
    dropImages: "Drop PNG, JPG, or WebP images",
    image: "References",
    references: "References",
    reference: "Reference",
    referenceCount: (count) => `${count} references`,
    referenceLimit: (count) => `A session supports up to ${count} references.`,
    removeReference: "Remove reference",
    noReferences: "No references — create from text",
    source: "Reference image",
    instruction: "Instructions",
    instructionHint: "Describe the image to create or what should change",
    saveTo: "Save to",
    change: "Change",
    generate: "Create image",
    generateAgain: "Create again",
    textOnlyTitle: "Create from your description",
    textOnlyHint: "References are optional.",
    before: "Before",
    after: "After",
    comparison: "Before and after",
    queued: "Queued",
    preparing: "Getting ready",
    uploading: "Adding reference image",
    generating: "Creating image",
    finalizing: "Finishing image",
    almostThere: "Almost there",
    lessMinute: "Less than a minute",
    aboutMinutes: (count) => `About ${count} min left`,
    takingLonger: "Taking a little longer",
    ready: "Image ready",
    failed: "Could not create image",
    cancelled: "Cancelled",
    cancel: "Cancel",
    openFolder: "Show in folder",
    rename: "Rename",
    delete: "Delete",
    deleteConfirm: "Delete this result?",
    renameTitle: "Rename result",
    dismiss: "Cancel",
    save: "Save",
    promptRequired: "Write the image instructions first.",
    referenceReady: "Ready to create",
    savedResult: "Saved result",
    minimize: "Minimize",
    close: "Close",
  },
  ko: {
    title: "이미지 생성/편집",
    queue: "작업",
    emptyQueue: "+를 눌러 새 이미지를 시작하세요.",
    newSession: "새 이미지",
    newSessionHint: "지시, 참조 이미지 또는 둘 다로 시작하세요.",
    newImage: "새 이미지",
    addImages: "이미지 추가",
    addReferences: "참조 추가",
    dropImages: "PNG, JPG 또는 WebP 이미지를 놓으세요",
    image: "참조",
    references: "참조",
    reference: "참조",
    referenceCount: (count) => `참조 ${count}개`,
    referenceLimit: (count) => `한 작업에는 참조를 최대 ${count}개 추가할 수 있습니다.`,
    removeReference: "참조 삭제",
    noReferences: "참조 없음 — 텍스트로 생성",
    source: "참조 이미지",
    instruction: "지시",
    instructionHint: "만들 이미지 또는 바꿀 내용을 설명하세요",
    saveTo: "저장 위치",
    change: "변경",
    generate: "이미지 만들기",
    generateAgain: "다시 만들기",
    textOnlyTitle: "설명으로 이미지 만들기",
    textOnlyHint: "참조 이미지는 선택 사항입니다.",
    before: "이전",
    after: "이후",
    comparison: "이전 및 이후",
    queued: "대기 중",
    preparing: "준비 중",
    uploading: "참조 이미지 추가 중",
    generating: "이미지 생성 중",
    finalizing: "이미지 마무리 중",
    almostThere: "거의 완료되었습니다",
    lessMinute: "1분 이내",
    aboutMinutes: (count) => `약 ${count}분 남음`,
    takingLonger: "예상보다 조금 더 걸리고 있습니다",
    ready: "이미지 준비 완료",
    failed: "이미지를 만들지 못했습니다",
    cancelled: "취소됨",
    cancel: "취소",
    openFolder: "폴더에서 보기",
    rename: "이름 바꾸기",
    delete: "삭제",
    deleteConfirm: "이 결과를 삭제할까요?",
    renameTitle: "결과 이름 바꾸기",
    dismiss: "취소",
    save: "저장",
    promptRequired: "먼저 이미지 지시를 작성하세요.",
    referenceReady: "생성 준비 완료",
    savedResult: "저장된 결과",
    minimize: "최소화",
    close: "닫기",
  },
  vi: {
    title: "Tạo/edit ảnh",
    queue: "Phiên tạo",
    emptyQueue: "Nhấn + để bắt đầu một ảnh mới.",
    newSession: "Ảnh mới",
    newSessionHint: "Bắt đầu bằng yêu cầu, ảnh tham chiếu hoặc cả hai.",
    newImage: "Ảnh mới",
    addImages: "Thêm ảnh",
    addReferences: "Thêm ảnh tham chiếu",
    dropImages: "Thả ảnh PNG, JPG hoặc WebP",
    image: "Ảnh tham chiếu",
    references: "Ảnh tham chiếu",
    reference: "Ảnh tham chiếu",
    referenceCount: (count) => `${count} ảnh tham chiếu`,
    referenceLimit: (count) => `Mỗi phiên hỗ trợ tối đa ${count} ảnh tham chiếu.`,
    removeReference: "Xóa ảnh tham chiếu",
    noReferences: "Không có ảnh tham chiếu — tạo từ mô tả",
    source: "Ảnh tham chiếu",
    instruction: "Yêu cầu",
    instructionHint: "Mô tả ảnh cần tạo hoặc nội dung cần thay đổi",
    saveTo: "Lưu vào",
    change: "Đổi",
    generate: "Tạo ảnh",
    generateAgain: "Tạo lại",
    textOnlyTitle: "Tạo ảnh từ mô tả",
    textOnlyHint: "Ảnh tham chiếu là tùy chọn.",
    before: "Trước",
    after: "Sau",
    comparison: "Trước và sau",
    queued: "Đang chờ",
    preparing: "Đang chuẩn bị",
    uploading: "Đang thêm ảnh tham chiếu",
    generating: "Đang tạo ảnh",
    finalizing: "Đang hoàn thiện ảnh",
    almostThere: "Sắp xong",
    lessMinute: "Còn dưới một phút",
    aboutMinutes: (count) => `Còn khoảng ${count} phút`,
    takingLonger: "Đang mất thêm một chút thời gian",
    ready: "Ảnh đã sẵn sàng",
    failed: "Không thể tạo ảnh",
    cancelled: "Đã hủy",
    cancel: "Hủy",
    openFolder: "Hiện trong thư mục",
    rename: "Đổi tên",
    delete: "Xóa",
    deleteConfirm: "Xóa kết quả này?",
    renameTitle: "Đổi tên kết quả",
    dismiss: "Hủy",
    save: "Lưu",
    promptRequired: "Hãy viết yêu cầu cho ảnh trước.",
    referenceReady: "Sẵn sàng tạo",
    savedResult: "Kết quả đã lưu",
    minimize: "Thu nhỏ",
    close: "Đóng",
  },
};

export function copyFor(value: string | undefined): Copy {
  const language: Language = value === "ko" || value === "vi" ? value : "en";
  return copies[language];
}
