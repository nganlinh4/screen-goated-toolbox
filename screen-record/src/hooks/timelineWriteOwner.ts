export interface TimelineWriteOwner {
  isCurrent: () => boolean;
  onInvalidated: (listener: () => void) => () => void;
  audioFence?: import('@/lib/audioImportWriteFence').AudioImportWriteFence;
}

export class AudioSavedNotPublishedError extends Error {
  constructor() { super('Audio was saved, but later edits are not saved. Retry saving the current edits.'); }
}

export class AudioSavedPreviewError extends Error {
  constructor() { super('Audio was saved, but its preview could not be opened.'); }
}
