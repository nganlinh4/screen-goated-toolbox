import type { Translations } from './en';

const vi: Translations = {
  // Header
  appTitle: 'Quay Màn Hình',
  rec: 'GHI',
  clickToRemove: 'Nhấn để xóa',
  addHotkey: 'Thêm Phím Tắt',
  toggleKeyviz: 'Bật/Tắt Keyviz',
  installKeyviz: 'Cài & Bật Keyviz',
  keystrokesOn: 'Phím Nhấn: BẬT',
  showKeystrokes: 'Hiện Phím Nhấn',
  export: 'Xuất',
  projects: 'Dự Án',
  minimize: 'Thu nhỏ',
  maximize: 'Phóng to',
  restore: 'Khôi phục',
  close: 'Đóng',

  // SidePanel tabs
  tabZoom: 'Phóng To',
  tabBackground: 'Nền',
  tabCursor: 'Con Trỏ',
  tabText: 'Chữ',

  // ZoomPanel
  zoomConfiguration: 'Cấu Hình Phóng To',
  zoomFactor: 'Hệ Số Phóng',
  horizontalPosition: 'Vị Trí Ngang',
  verticalPosition: 'Vị Trí Dọc',
  zoomHint: 'Cuộn hoặc kéo trong bản xem trước để thêm keyframe phóng to',

  // BackgroundPanel
  backgroundAndLayout: 'Nền & Bố Cục',
  canvasSize: 'Kích Thước Canvas',
  canvasAuto: 'Tự Động',
  canvasCustom: 'Tùy Chỉnh',
  videoSize: 'Kích Thước Video',
  roundness: 'Độ Bo Tròn',
  shadow: 'Đổ Bóng',
  volume: 'Âm Lượng',
  backgroundStyle: 'Kiểu Nền',

  // CursorPanel
  cursorSettings: 'Cài Đặt Con Trỏ',
  cursorSize: 'Kích Thước Con Trỏ',
  movementSmoothing: 'Độ Mượt Di Chuyển',
  pointerMovementDelay: 'Độ Trễ Di Chuyển Con Trỏ',
  pointerWiggleStrength: 'Độ Lắc Con Trỏ',
  pointerWiggleDamping: 'Giảm Chấn Lắc',
  pointerWiggleResponse: 'Độ Nhạy Lắc',

  // TextPanel
  textOverlay: 'Lớp Phủ Chữ',
  addText: 'Thêm Chữ',
  textContent: 'Nội Dung',
  dragTextHint: 'Kéo chữ trong bản xem trước để đặt lại vị trí',
  fontSize: 'Cỡ Chữ',
  color: 'Màu',
  textPanelHint: 'Thêm lớp phủ chữ hoặc chọn từ timeline',
  fontWeight: 'Độ Đậm',
  fontWidth: 'Độ Rộng',
  fontSlant: 'Nghiêng',
  fontRound: 'Độ Tròn',
  textAlignment: 'Căn Chỉnh',
  opacity: 'Độ Mờ',
  letterSpacing: 'Giãn Chữ',
  backgroundPill: 'Nền',
  pillColor: 'Màu Nền',
  pillOpacity: 'Độ Mờ',
  pillRadius: 'Độ Bo Tròn',
  deleteText: 'Xóa Chữ',

  // VideoPreview
  processingVideo: 'Đang Xử Lý Video',
  processingHint: 'Có thể mất vài giây...',
  recordingInProgress: 'Đang ghi hình',
  noVideoSelected: 'Chưa Chọn Video',
  startRecordingHint: "Nhấn 'Bắt Đầu Ghi' để bắt đầu",
  loadingVideo: 'Đang tải video:',
  applyCrop: 'Áp Dụng Cắt',
  cropVideo: 'Cắt Video',

  // Dialogs - Export
  exportingVideo: 'Đang xuất video...',
  processingVideoShort: 'Đang xử lý video...',
  timeRemaining: 'còn lại',
  preparingExport: 'Đang chuẩn bị...',
  exportOptions: 'Tùy Chọn Xuất',
  resolution: 'Độ Phân Giải',
  frameRate: 'Tốc Độ Khung Hình',
  speed: 'Tốc Độ',
  slower: 'Chậm hơn',
  faster: 'Nhanh hơn',
  saveLocation: 'Vị trí lưu',
  browse: 'Duyệt',
  browsing: 'Đang duyệt...',
  cancel: 'Hủy',
  exportVideo: 'Xuất Video',

  // Dialogs - Projects
  noProjectsYet: 'Chưa có dự án nào',
  max: 'Tối đa',

  // Dialogs - Monitor Select
  selectMonitor: 'Chọn Màn Hình',

  // Dialogs - Hotkey
  pressKeys: 'Nhấn Phím...',
  pressKeysHint: 'Nhấn tổ hợp phím bạn muốn sử dụng.',

  // Dialogs - FFmpeg Setup
  downloadingDeps: 'Đang Tải Phụ Thuộc',
  settingUp: 'Đang Cài Đặt...',
  installFailed: 'Cài Đặt Thất Bại',
  installCancelled: 'Cài Đặt Đã Hủy',
  preparingRecorder: 'Đang Chuẩn Bị Quay Màn Hình',
  ffmpegDesc: 'FFmpeg và ffprobe cần thiết để quay màn hình. Chúng tôi đang tải cho bạn.',
  extractingDesc: 'Sắp xong! Đang giải nén file vào hệ thống.',
  cancelledDesc: 'Quá trình cài đặt đã bị dừng.',
  systemCheckDesc: 'Vui lòng đợi trong khi kiểm tra hệ thống.',
  tryAgain: 'Thử Lại',
  cancelInstallation: 'Hủy Cài Đặt',
  closeApp: 'Đóng Ứng Dụng',
  ffmpegEssentials: 'FFmpeg Essentials',
  downloaded: 'đã tải',

  // Timeline
  trackZoom: 'Phóng To',
  trackText: 'Chữ',
  trackPointer: 'Con Trỏ',
  trackVideo: 'Video',

  // App
  preparingVideoOverlay: 'Đang chuẩn bị video...',
  autoZoom: 'Phóng To Tự Động',
  smartPointer: 'Con Trỏ Thông Minh',

  // Export presets
  presetBalanced: 'Cân Bằng (Khuyến Nghị)',
  presetOriginal: 'Chất Lượng Gốc',
  dimOriginal: 'Kích Thước Gốc',
  dimFullHD: 'Full HD (1080p)',
  dimHD: 'HD (720p)',
};

export default vi;
