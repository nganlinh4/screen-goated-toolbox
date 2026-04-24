export const DEFAULT_GEMINI_SUBTITLE_PROMPT = `Transcribe all spoken content in this media.
Return the original spoken language. Do not translate.
Create concise, readable subtitle segments.
Keep punctuation natural.
Do not add speaker labels, summaries, notes, markdown, or commentary.`;

export const GEMINI_SUBTITLE_PROMPT_PRESETS = [
  {
    id: 'general',
    labelKey: 'subtitleGeminiPresetGeneral',
    prompt: DEFAULT_GEMINI_SUBTITLE_PROMPT,
  },
  {
    id: 'extract-text',
    labelKey: 'subtitleGeminiPresetExtractText',
    prompt: `Extract only visible text and hardcoded subtitles from this media.
Ignore spoken audio.
Create concise, readable timed text segments.
Keep the visible text exact when possible.`,
  },
  {
    id: 'focus-lyrics',
    labelKey: 'subtitleGeminiPresetLyrics',
    prompt: `Extract only sung lyrics from this media.
Ignore spoken dialogue, narration, and instrumental music.
Create concise, readable timed lyric segments.
Keep the lyrics verbatim.`,
  },
  {
    id: 'describe-video',
    labelKey: 'subtitleGeminiPresetDescribeVideo',
    prompt: `Describe significant visual events and scene changes in this media.
Ignore spoken audio unless it explains the visible event.
Create concise, readable timed description segments.
Focus on meaningful visual moments.`,
  },
  {
    id: 'translate-directly',
    labelKey: 'subtitleGeminiPresetTranslate',
    prompt: `Transcribe all spoken content in this media.
Translate each timed segment directly into Vietnamese.
Return only the Vietnamese translation in each segment text.
Do not include the original language unless it is needed for names or terms.`,
  },
  {
    id: 'chaptering',
    labelKey: 'subtitleGeminiPresetChaptering',
    prompt: `Analyze this media and identify distinct chapters.
Base chapters on major topic shifts, scene changes, or activity changes.
Create a small number of meaningful timed chapter segments.
Format each segment as "Chapter Title :: Brief description".`,
  },
  {
    id: 'diarize-speakers',
    labelKey: 'subtitleGeminiPresetSpeakers',
    prompt: `Transcribe all spoken content in this media.
Identify different speakers.
Label speakers consistently as "Speaker 1", "Speaker 2", "Speaker 3", etc.
Every segment must start with "Speaker X: " and the spoken text.`,
  },
] as const;

export const GEMINI_SUBTITLE_OUTPUT_CONTRACT_PREVIEW = `Hidden output contract:
- Generate subtitle segments for this media clip.
- Return JSON only and match the provided schema exactly.
- Use integer millisecond timestamps relative to the start of the clip.
- Segments must be sorted by start_ms, non-overlapping, and strictly increasing.
- Each text field should contain the user-requested media-derived content for that time range.
- No markdown, prose wrapper, or commentary outside the JSON schema.`;
