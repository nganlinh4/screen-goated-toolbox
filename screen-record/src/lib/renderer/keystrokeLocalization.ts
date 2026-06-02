export function translateLabel(label: string, lang: string): string {
  if (lang === 'en') return label;

  // Arrow keys -> symbols universally for all non-English languages.
  // They appear both as standalone keys and as combo tokens ("Ctrl + Left" -> "Ctrl + <-").
  const ARROW_SYMBOLS: Record<string, string> = {
    'Left': '\u2190', 'Right': '\u2192', 'Up': '\u2191', 'Down': '\u2193',
  };

  // Language-specific overrides. Rules per locale:
  //   ko - transliterate to phonetics; modifier keys (Ctrl/Shift/Alt/Win) stay English
  //   vi - only mouse labels + Space get Vietnamese; all other keys stay English (research-backed)
  //   es - standard Spanish computing terms (Intro, Retroceso, Supr, Inicio/Fin, Re/Av Pag)
  //   ja - katakana phonetics for common keys; Home/End/PageUp/PageDown stay English
  //   zh - standard Chinese computing terms; Tab and Esc stay English
  const LOCALIZATION_MAPS: Record<string, Record<string, string>> = {
    ko: {
      'Left Click': '\uC88C\uD074\uB9AD', 'Right Click': '\uC6B0\uD074\uB9AD', 'Middle Click': '\uD720\uD074\uB9AD',
      '\u2191 Scroll': '\u2191 \uC2A4\uD06C\uB864', '\u2193 Scroll': '\u2193 \uC2A4\uD06C\uB864', 'Mouse Click': '\uD074\uB9AD',
      'Space': '\uC2A4\uD398\uC774\uC2A4', 'Enter': '\uC5D4\uD130', 'Backspace': '\uBC31\uC2A4\uD398\uC774\uC2A4',
      'Esc': 'ESC', 'Tab': '\uD0ED', 'Delete': '\uC0AD\uC81C', 'Insert': '\uC0BD\uC785',
      'Home': 'Home', 'End': 'End', 'PageUp': '\uD398\uC774\uC9C0\uC5C5', 'PageDown': '\uD398\uC774\uC9C0\uB2E4\uC6B4',
      'CapsLock': '\uD55C/\uC601',
    },
    vi: {
      // Mouse labels use Vietnamese; key names stay English per local convention
      'Left Click': 'Chu\u1ED9t Tr\u00E1i', 'Right Click': 'Chu\u1ED9t Ph\u1EA3i', 'Middle Click': 'Chu\u1ED9t Gi\u1EEFa',
      '\u2191 Scroll': '\u2191 Cu\u1ED9n', '\u2193 Scroll': '\u2193 Cu\u1ED9n', 'Mouse Click': 'Nh\u1EA5p Chu\u1ED9t',
      'Space': 'C\u00E1ch',
    },
    es: {
      'Left Click': 'Clic Izq', 'Right Click': 'Clic Der', 'Middle Click': 'Clic Central',
      '\u2191 Scroll': '\u2191 Desplazar', '\u2193 Scroll': '\u2193 Desplazar', 'Mouse Click': 'Clic',
      'Space': 'Espacio', 'Enter': 'Intro', 'Backspace': 'Retroceso',
      'Esc': 'Esc', 'Tab': 'Tab', 'Delete': 'Supr', 'Insert': 'Ins',
      'Home': 'Inicio', 'End': 'Fin', 'PageUp': 'Re P\u00E1g', 'PageDown': 'Av P\u00E1g',
    },
    ja: {
      'Left Click': '\u5DE6\u30AF\u30EA\u30C3\u30AF', 'Right Click': '\u53F3\u30AF\u30EA\u30C3\u30AF', 'Middle Click': '\u4E2D\u30AF\u30EA\u30C3\u30AF',
      '\u2191 Scroll': '\u2191 \u30B9\u30AF\u30ED\u30FC\u30EB', '\u2193 Scroll': '\u2193 \u30B9\u30AF\u30ED\u30FC\u30EB', 'Mouse Click': '\u30AF\u30EA\u30C3\u30AF',
      // Katakana phonetics; Enter!=\u78BA\u5B9A (that's IME confirm), Backspace!=Delete
      'Space': '\u30B9\u30DA\u30FC\u30B9', 'Enter': '\u30A8\u30F3\u30BF\u30FC', 'Backspace': '\u30D0\u30C3\u30AF\u30B9\u30DA\u30FC\u30B9', 'Delete': '\u30C7\u30EA\u30FC\u30C8',
      'Esc': 'ESC', 'Tab': '\u30BF\u30D6', 'Insert': '\u633F\u5165',
      // Home/End/PageUp/PageDown stay English in Japanese convention
    },
    zh: {
      'Left Click': '\u5DE6\u952E\u70B9\u51FB', 'Right Click': '\u53F3\u952E\u70B9\u51FB', 'Middle Click': '\u4E2D\u952E\u70B9\u51FB',
      '\u2191 Scroll': '\u2191 \u6EDA\u52A8', '\u2193 Scroll': '\u2193 \u6EDA\u52A8', 'Mouse Click': '\u70B9\u51FB',
      // Standard Chinese computing terms; Tab and Esc stay English
      'Space': '\u7A7A\u683C', 'Enter': '\u56DE\u8F66', 'Backspace': '\u9000\u683C', 'Delete': '\u5220\u9664',
      'Home': '\u884C\u9996', 'End': '\u884C\u5C3E', 'PageUp': '\u4E0A\u4E00\u9875', 'PageDown': '\u4E0B\u4E00\u9875',
      'CapsLock': '\u5927\u5199\u9501\u5B9A',
    },
  };

  const map = LOCALIZATION_MAPS[lang] || {};
  // Split modifier combos like "Ctrl + Left Click", translate each token independently
  const parts = label.split(' + ');
  const translatedParts = parts.map(part => map[part] ?? ARROW_SYMBOLS[part] ?? part);
  return translatedParts.join(' + ');
}
