import { describe, expect, it } from 'vitest';
import { buildKeystrokeEvents } from '../../src/lib/keystrokeProcessor';

describe('keystroke labels', () => {
  it('preserves the native label in the shared preview/export events', () => {
    const [event] = buildKeystrokeEvents([{
      type: 'keyboard', timestamp: 0.1, vk: 107, key: 'Numpad Add',
      label: 'Ctrl + Win + Numpad Add', modifiers: { ctrl: true, win: true },
    }], 3);
    expect(event.label).toBe('Ctrl + Win + Numpad Add');
  });

  it('still reads input events saved without a complete label', () => {
    const [event] = buildKeystrokeEvents([{
      type: 'keyboard', timestamp: 0.1, vk: 65, key: 'A', modifiers: { ctrl: true },
    }], 3);
    expect(event.label).toBe('Ctrl + A');
  });
});
