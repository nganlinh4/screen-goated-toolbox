import type { Translations } from './en';

const ko: Translations = {
  // Header
  appTitle: '화면 녹화',
  rec: '녹화',
  clickToRemove: '클릭하여 제거',
  addHotkey: '단축키 추가',
  toggleKeyviz: 'Keyviz 전환',
  installKeyviz: 'Keyviz 설치 및 활성화',
  keystrokesOn: '키 표시: 켜짐',
  showKeystrokes: '키 표시',
  export: '내보내기',
  projects: '프로젝트',
  minimize: '최소화',
  maximize: '최대화',
  restore: '복원',
  close: '닫기',

  // SidePanel tabs
  tabZoom: '확대',
  tabBackground: '배경',
  tabCursor: '커서',
  tabText: '텍스트',

  // ZoomPanel
  zoomConfiguration: '확대 설정',
  zoomFactor: '확대 배율',
  horizontalPosition: '가로 위치',
  verticalPosition: '세로 위치',
  zoomHint: '미리보기에서 스크롤하거나 드래그하여 확대 키프레임 추가',

  // BackgroundPanel
  backgroundAndLayout: '배경 및 레이아웃',
  videoSize: '비디오 크기',
  roundness: '둥글기',
  shadow: '그림자',
  volume: '볼륨',
  backgroundStyle: '배경 스타일',

  // CursorPanel
  cursorSettings: '커서 설정',
  cursorSize: '커서 크기',
  movementSmoothing: '움직임 부드러움',

  // TextPanel
  textOverlay: '텍스트 오버레이',
  addText: '텍스트 추가',
  textContent: '텍스트 내용',
  dragTextHint: '미리보기에서 텍스트를 드래그하여 위치 변경',
  fontSize: '글꼴 크기',
  color: '색상',
  textPanelHint: '텍스트 오버레이를 추가하거나 타임라인에서 선택하세요',
  fontWeight: '두께',
  fontWidth: '너비',
  fontSlant: '기울기',
  fontRound: '둥글기',
  textAlignment: '정렬',
  opacity: '투명도',
  letterSpacing: '자간',
  backgroundPill: '배경',
  pillColor: '배경 색상',
  pillRadius: '둥글기',
  deleteText: '텍스트 삭제',

  // VideoPreview
  processingVideo: '비디오 처리 중',
  processingHint: '잠시만 기다려주세요...',
  recordingInProgress: '녹화 진행 중',
  noVideoSelected: '비디오 없음',
  startRecordingHint: "'녹화 시작'을 클릭하여 시작",
  loadingVideo: '비디오 로딩:',
  applyCrop: '자르기 적용',
  cropVideo: '비디오 자르기',

  // Dialogs - Export
  exportingVideo: '비디오 내보내는 중...',
  processingVideoShort: '비디오 처리 중...',
  exportOptions: '내보내기 옵션',
  quality: '품질',
  dimensions: '해상도',
  speed: '속도',
  slower: '느리게',
  faster: '빠르게',
  cancel: '취소',
  exportVideo: '비디오 내보내기',

  // Dialogs - Projects
  noProjectsYet: '프로젝트가 없습니다',
  max: '최대',

  // Dialogs - Monitor Select
  selectMonitor: '모니터 선택',

  // Dialogs - Hotkey
  pressKeys: '키를 누르세요...',
  pressKeysHint: '사용할 키 조합을 누르세요.',

  // Dialogs - FFmpeg Setup
  downloadingDeps: '종속성 다운로드 중',
  settingUp: '설정 중...',
  installFailed: '설치 실패',
  installCancelled: '설치 취소됨',
  preparingRecorder: '화면 녹화 준비 중',
  ffmpegDesc: '화면 녹화에 FFmpeg와 ffprobe가 필요합니다. 다운로드 중입니다.',
  extractingDesc: '거의 완료! 바이너리를 시스템에 추출 중입니다.',
  cancelledDesc: '설치가 중단되었습니다.',
  systemCheckDesc: '시스템을 확인하는 동안 기다려주세요.',
  tryAgain: '다시 시도',
  cancelInstallation: '설치 취소',
  closeApp: '앱 닫기',
  ffmpegEssentials: 'FFmpeg Essentials',
  downloaded: '다운로드됨',

  // Timeline
  trackZoom: '확대',
  trackText: '텍스트',
  trackVideo: '비디오',

  // App
  preparingVideoOverlay: '비디오 준비 중...',
  autoZoom: '자동 확대',

  // Export presets
  presetBalanced: '균형 (권장)',
  presetOriginal: '원본 품질',
  dimOriginal: '원본 크기',
  dimFullHD: 'Full HD (1080p)',
  dimHD: 'HD (720p)',
};

export default ko;
