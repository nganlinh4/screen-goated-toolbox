const en = {
  // Header
  appTitle: 'Screen Record',
  rec: 'REC',
  clickToRemove: 'Click to remove',
  addHotkey: 'Add Hotkey',
  toggleKeyviz: 'Toggle Keyviz',
  installKeyviz: 'Install & Enable Keyviz',
  keystrokesOn: 'Keystrokes: ON',
  showKeystrokes: 'Show Keystrokes',
  export: 'Export',
  projects: 'Projects',
  minimize: 'Minimize',
  maximize: 'Maximize',
  restore: 'Restore',
  close: 'Close',

  // SidePanel tabs
  tabZoom: 'Zoom',
  tabBackground: 'Background',
  tabCursor: 'Cursor',
  tabText: 'Text',

  // ZoomPanel
  zoomConfiguration: 'Zoom Configuration',
  zoomFactor: 'Zoom Factor',
  horizontalPosition: 'Horizontal Position',
  verticalPosition: 'Vertical Position',
  zoomHint: 'Scroll or drag in the preview to add a zoom keyframe',

  // BackgroundPanel
  backgroundAndLayout: 'Background & Layout',
  videoSize: 'Video Size',
  roundness: 'Roundness',
  shadow: 'Shadow',
  volume: 'Volume',
  backgroundStyle: 'Background Style',

  // CursorPanel
  cursorSettings: 'Cursor Settings',
  cursorSize: 'Cursor Size',
  movementSmoothing: 'Movement Smoothing',

  // TextPanel
  textOverlay: 'Text Overlay',
  addText: 'Add Text',
  textContent: 'Text Content',
  dragTextHint: 'Drag text in preview to reposition',
  fontSize: 'Font Size',
  color: 'Color',
  textPanelHint: 'Add a text overlay or select one from the timeline',
  fontWeight: 'Weight',
  fontWidth: 'Width',
  fontSlant: 'Slant',
  fontRound: 'Roundness',
  textAlignment: 'Alignment',
  opacity: 'Opacity',
  letterSpacing: 'Letter Spacing',
  backgroundPill: 'Background',
  pillColor: 'Background Color',
  pillRadius: 'Roundness',
  deleteText: 'Delete Text',

  // VideoPreview
  processingVideo: 'Processing Video',
  processingHint: 'This may take a few moments...',
  recordingInProgress: 'Recording in progress',
  noVideoSelected: 'No Video Selected',
  startRecordingHint: "Click 'Start Recording' to begin",
  loadingVideo: 'Loading video:',
  applyCrop: 'Apply Crop',
  cropVideo: 'Crop Video',

  // Dialogs - Export
  exportingVideo: 'Exporting video...',
  processingVideoShort: 'Processing video...',
  exportOptions: 'Export Options',
  quality: 'Quality',
  dimensions: 'Dimensions',
  speed: 'Speed',
  slower: 'Slower',
  faster: 'Faster',
  cancel: 'Cancel',
  exportVideo: 'Export Video',

  // Dialogs - Projects
  noProjectsYet: 'No projects yet',
  max: 'Max',

  // Dialogs - Monitor Select
  selectMonitor: 'Select Monitor',

  // Dialogs - Hotkey
  pressKeys: 'Press Keys...',
  pressKeysHint: 'Press the combination of keys you want to use.',

  // Dialogs - FFmpeg Setup
  downloadingDeps: 'Downloading Dependencies',
  settingUp: 'Setting Up...',
  installFailed: 'Installation Failed',
  installCancelled: 'Installation Cancelled',
  preparingRecorder: 'Preparing Screen Recorder',
  ffmpegDesc: 'FFmpeg and ffprobe are required for screen recording. We are downloading them for you.',
  extractingDesc: 'Almost ready! Extracting binaries to your system.',
  cancelledDesc: 'The installation was stopped.',
  systemCheckDesc: 'Please wait while we check your system.',
  tryAgain: 'Try Again',
  cancelInstallation: 'Cancel Installation',
  closeApp: 'Close App',
  ffmpegEssentials: 'FFmpeg Essentials',
  downloaded: 'downloaded',

  // Timeline
  trackZoom: 'Zoom',
  trackText: 'Text',
  trackVideo: 'Video',

  // App
  preparingVideoOverlay: 'Preparing video...',
  autoZoom: 'Auto Zoom',

  // Export presets
  presetBalanced: 'Balanced (Recommended)',
  presetOriginal: 'Original Quality',
  dimOriginal: 'Original Size',
  dimFullHD: 'Full HD (1080p)',
  dimHD: 'HD (720p)',
};

export type Translations = { [K in keyof typeof en]: string };
export default en as Translations;
