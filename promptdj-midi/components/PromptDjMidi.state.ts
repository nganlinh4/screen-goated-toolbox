import type { Prompt } from '../types';

export interface PromptDjInitialState {
  basePrompts: Map<string, Prompt>;
  baseOrder: string[];
  prompts: Map<string, Prompt>;
  addSlotsActive: boolean[];
  removedSlots: Set<string>;
}

interface SavedPrompt {
  promptId?: unknown;
  text?: unknown;
  weight?: unknown;
  cc?: unknown;
  color?: unknown;
}

export function createPromptDjInitialState(
  initialPrompts: Map<string, Prompt>,
  storageKey: string,
): PromptDjInitialState {
  const basePrompts = new Map<string, Prompt>();
  const baseOrder: string[] = [];
  for (const [key, prompt] of initialPrompts.entries()) {
    basePrompts.set(key, { ...prompt });
    baseOrder.push(key);
  }

  const prompts = new Map<string, Prompt>();
  let count = 0;
  for (const [key, prompt] of basePrompts.entries()) {
    if (count < 22) prompts.set(key, { ...prompt });
    count++;
  }

  let addSlotsActive = [false, false, false, false];
  let removedSlots = new Set<string>();

  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return { basePrompts, baseOrder, prompts, addSlotsActive, removedSlots };

    const parsed = JSON.parse(raw) as {
      prompts?: SavedPrompt[];
      addSlotsActive?: unknown[];
      removedSlots?: unknown[];
    };

    const savedPrompts = Array.isArray(parsed?.prompts) ? parsed.prompts : [];
    savedPrompts.forEach((saved) => {
      if (!saved || typeof saved.promptId !== 'string') return;
      if (prompts.has(saved.promptId)) {
        const existing = prompts.get(saved.promptId)!;
        prompts.set(saved.promptId, {
          ...existing,
          text: typeof saved.text === 'string' ? saved.text : existing.text,
          weight: typeof saved.weight === 'number' ? saved.weight : existing.weight,
          color: typeof saved.color === 'string' ? saved.color : existing.color,
          cc: typeof saved.cc === 'number' ? saved.cc : existing.cc,
        });
        return;
      }
      if (saved.promptId.startsWith('extra-')) {
        prompts.set(saved.promptId, {
          promptId: saved.promptId,
          text: typeof saved.text === 'string' ? saved.text : '',
          weight: typeof saved.weight === 'number' ? saved.weight : 0,
          cc: typeof saved.cc === 'number' ? saved.cc : 0,
          color: typeof saved.color === 'string' ? saved.color : '#ffffff',
        });
      }
    });

    if (Array.isArray(parsed?.addSlotsActive) && parsed.addSlotsActive.length === 4) {
      addSlotsActive = parsed.addSlotsActive.map(Boolean);
    }
    if (Array.isArray(parsed?.removedSlots)) {
      const validRemovals = parsed.removedSlots.filter(
        (value): value is string => typeof value === 'string' && basePrompts.has(value),
      );
      removedSlots = new Set<string>(validRemovals);
    }
  } catch {
    // Ignore corrupt localStorage and keep default prompt layout.
  }

  return { basePrompts, baseOrder, prompts, addSlotsActive, removedSlots };
}

export function savePromptDjState(
  storageKey: string,
  prompts: Map<string, Prompt>,
  addSlotsActive: boolean[],
  removedSlots: Set<string>,
) {
  try {
    const payload = {
      prompts: [...prompts.values()].map((prompt) => ({
        promptId: prompt.promptId,
        text: prompt.text,
        weight: prompt.weight,
        cc: prompt.cc,
        color: prompt.color,
      })),
      addSlotsActive,
      removedSlots: [...removedSlots],
    };
    localStorage.setItem(storageKey, JSON.stringify(payload));
  } catch {
    // Best-effort persistence only.
  }
}

export function buildPromptBackground(
  prompts: Map<string, Prompt>,
  driftStrength: number,
): string {
  const clamp01 = (value: number) => Math.min(Math.max(value, 0), 1);
  const maxWeight = 0.5;
  const maxAlpha = 0.6;
  const now = performance.now() * 0.0006;
  const backgrounds: string[] = [];

  [...prompts.values()].forEach((prompt, index) => {
    const alphaPct = clamp01(prompt.weight / maxWeight) * maxAlpha;
    const alpha = Math.round(alphaPct * 0xff).toString(16).padStart(2, '0');
    const stop = prompt.weight / 2;
    const gridX = (index % 6) / 5;
    const gridY = Math.floor(index / 6) / 3;
    const phase = index * 1.37;
    const driftAmp = 4 * (driftStrength || 0);
    const xPct = gridX * 100 + Math.sin(now + phase) * driftAmp;
    const yPct = gridY * 100 + Math.cos(now * 0.9 + phase) * driftAmp;
    const stopPct = Math.max(0, Math.min(100, stop * 100));
    backgrounds.push(
      `radial-gradient(circle at ${xPct}% ${yPct}%, ${prompt.color}${alpha} 0px, ${prompt.color}00 ${stopPct}%)`,
    );
  });

  return backgrounds.join(', ');
}
