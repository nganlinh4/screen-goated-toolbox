/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/
import { html, LitElement } from 'lit';
import { customElement, property, state, query } from 'lit/decorators.js';
import { styleMap } from 'lit/directives/style-map.js';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';

import { throttle } from '../utils/throttle';
import { ICONS } from '../utils/Icons';

import './PromptController';
import './PlayPauseMorphWrapper';
import './OnboardingPopup';
import type { PlaybackState, Prompt } from '../types';
import { MidiDispatcher } from '../utils/MidiDispatcher';
import { LOCALES, Lang } from '../utils/Locales';
import { promptDjMidiStyles } from './PromptDjMidi.styles';
import {
  buildPromptBackground,
  createPromptDjInitialState,
  savePromptDjState,
} from './PromptDjMidi.state';

/** The grid of prompt inputs. */
@customElement('prompt-dj-midi')
export class PromptDjMidi extends LitElement {
  static override styles = promptDjMidiStyles;

  private prompts: Map<string, Prompt>;
  private midiDispatcher: MidiDispatcher;

  @property({ type: Boolean }) private showMidi = false;
  @property({ type: String }) public playbackState: PlaybackState = 'stopped';
  @property({ type: String }) public lang: string = 'en';
  @property({ type: Boolean }) public apiKeySet = false;
  @property({ type: Number }) public audioLevel = 0;
  private lastUserAction: 'play' | 'pause' | null = null;

  @state() private isRecording = false;
  @state() private recordElapsed = 0;
  @state() private recordAudioElapsed = 0;
  private recordInterval: number | null = null;

  // Recording playback state
  @state() private recordingUrl: string | null = null;

  @state() private midiInputIds: string[] = [];
  @state() private activeMidiInputId: string | null = null;
  @state() private optimisticLoading: boolean = false;
  @state() private optimisticPlaying: boolean | null = null; // null = follow real state
  @state() private downloaded = false;
  private clickCooldownUntil: number = 0; // epoch ms; during this window, ignore extra toggles

  // Background drift control
  @state() private driftStrength: number = 0; // 0 = at base, 1 = full drift
  private driftTarget: number = 0;
  private driftRaf: number | null = null;
  private lastDriftTick = 0;

  // Left add-column activation state (4 slots)
  @state() private addSlotsActive: boolean[] = [false, false, false, false];
  // Track which base grid slots are removed (to render add buttons in-grid)
  @state() private removedSlots: Set<string> = new Set();

  @state() private volume: number = 1.0;

  @property({ type: Object })
  private filteredPrompts = new Set<string>();

  private basePrompts: Map<string, Prompt>;
  private baseOrder: string[] = [];
  private readonly STORAGE_KEY = 'pdj_midi_state_v1';

  constructor(
    initialPrompts: Map<string, Prompt>,
  ) {
    super();
    const initialState = createPromptDjInitialState(initialPrompts, this.STORAGE_KEY);
    this.basePrompts = initialState.basePrompts;
    this.baseOrder = initialState.baseOrder;
    this.prompts = initialState.prompts;
    this.addSlotsActive = initialState.addSlotsActive;
    this.removedSlots = initialState.removedSlots;
    this.midiDispatcher = new MidiDispatcher();
  }

  public showRecording(blob: Blob) {
    if (this.recordingUrl) {
      URL.revokeObjectURL(this.recordingUrl);
    }
    this.recordingUrl = URL.createObjectURL(blob);
    this.requestUpdate();
  }

  private closeModal() {
    if (this.recordingUrl) {
      URL.revokeObjectURL(this.recordingUrl);
      this.recordingUrl = null;
    }
  }

  private downloadRecording() {
    if (!this.recordingUrl) return;
    const a = document.createElement('a');
    a.href = this.recordingUrl;
    a.download = `PromptDJ_${new Date().toISOString().replace(/:/g, '-')}.wav`;
    a.click();
    this.downloaded = true;
    setTimeout(() => { this.downloaded = false; }, 3000);
  }

  private renderModal() {
    const labels = LOCALES[this.lang as Lang];
    if (!this.recordingUrl) return html``;
    return html`
      <div class="modal-overlay" @click=${this.closeModal}>
        <div class="modal-content" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <div style="flex: 1; display: flex; flex-direction: column; gap: 2px;">
              <span style="display: block;">${this.downloaded ? labels.saved : labels.recording_ready}</span>
              <div style="font-size: 0.85em; opacity: 0.8;">${this.downloaded ? '' : labels.silence_removed}</div>
            </div>
            <button class="close-modal" @click=${this.closeModal}>&times;</button>
          </div>
          <audio class="audio-player" src=${this.recordingUrl} controls></audio>
          <button class="download-btn" @click=${this.downloadRecording}>
            ${unsafeHTML(this.downloaded ? ICONS.check_circle : ICONS.download)}
            ${this.downloaded ? labels.downloaded_msg : labels.download_btn}
          </button>
        </div>
      </div>
    `;
  }

  private saveState() {
    savePromptDjState(
      this.STORAGE_KEY,
      this.prompts,
      this.addSlotsActive,
      this.removedSlots,
    );
  }

  private handlePromptChanged(e: CustomEvent<Prompt>) {
    const { promptId, text, weight, cc } = e.detail;
    const prompt = this.prompts.get(promptId);

    if (!prompt) {
      console.error('prompt not found', promptId);
      return;
    }

    prompt.text = text;
    prompt.weight = weight;
    prompt.cc = cc;

    const newPrompts = new Map(this.prompts);
    newPrompts.set(promptId, prompt);

    this.prompts = newPrompts;
    this.requestUpdate();
    this.saveState();

    this.dispatchEvent(
      new CustomEvent('prompts-changed', { detail: this.prompts }),
    );
  }

  /** Generates radial gradients for each prompt based on weight and color, with gentle drift while playing. */
  private readonly makeBackground = throttle(
    () => buildPromptBackground(this.prompts, this.driftStrength),
    30, // don't re-render more than once every XXms
  );

  public async setShowMidi(show: boolean) {
    this.showMidi = show;
    if (!this.showMidi) return;
    try {
      const inputIds = await this.midiDispatcher.getMidiAccess();
      this.midiInputIds = inputIds;
      this.activeMidiInputId = this.midiDispatcher.activeMidiInputId;
      // Notify listeners (iframe bridge) that inputs are available/updated
      this.dispatchEvent(new CustomEvent('midi-inputs-changed', { detail: { inputs: this.midiInputIds, activeId: this.activeMidiInputId } }));
    } catch (e) {
      this.showMidi = false;
      this.dispatchEvent(new CustomEvent('error', { detail: (e as any).message }));
    }
  }

  // Public API used by parent (main app) via postMessage bridge
  public getShowMidi(): boolean { return this.showMidi; }
  public async refreshMidiInputs(): Promise<void> {
    try {
      const inputIds = await this.midiDispatcher.getMidiAccess();
      this.midiInputIds = inputIds;
      this.activeMidiInputId = this.midiDispatcher.activeMidiInputId;
      this.dispatchEvent(new CustomEvent('midi-inputs-changed', { detail: { inputs: this.midiInputIds, activeId: this.activeMidiInputId } }));
    } catch (e) {
      this.dispatchEvent(new CustomEvent('error', { detail: (e as any).message }));
    }
  }
  public getMidiInputs(): string[] { return this.midiInputIds; }
  public getActiveMidiInputId(): string | null { return this.activeMidiInputId; }
  public setActiveMidiInputId(id: string) {
    if (!id) return;
    this.activeMidiInputId = id;
    this.midiDispatcher.activeMidiInputId = id;
    this.dispatchEvent(new CustomEvent('midi-inputs-changed', { detail: { inputs: this.midiInputIds, activeId: this.activeMidiInputId } }));
    this.requestUpdate();
  }

  // Localized placeholder text
  private trPlaceholder(): string {
    return LOCALES[this.lang as Lang].prompt_placeholder;
  }

  private playPause(e: Event) {
    // Prevent the bubbling play-pause event from also reaching outer listeners
    e.stopPropagation();

    // Debounce rapid clicks to avoid double toggles
    const now = Date.now();
    if (now < this.clickCooldownUntil) return;
    this.clickCooldownUntil = now + 500;

    const morphEl = this.renderRoot?.querySelector('play-pause-morph') as HTMLElement | null;

    // If currently playing or loading: this click means STOP
    if (this.playbackState === 'playing' || this.playbackState === 'loading') {
      this.lastUserAction = 'pause';
      this.optimisticPlaying = false; // pause -> play morph immediately
      this.optimisticLoading = false; // ensure spinner is off
      morphEl?.removeAttribute('loading');
      morphEl?.setAttribute('playing', 'false');
      this.dispatchEvent(new CustomEvent('pause', { bubbles: true })); // explicit pause/stop
      return;
    }

    // If paused/stopped: this click means PLAY
    if (!this.apiKeySet) {
      this.dispatchEvent(new CustomEvent('error', { detail: 'Please set your Gemini API key in the main app first.' }));
      return;
    }
    this.lastUserAction = 'play';
    this.optimisticLoading = true; // show spinner immediately
    this.optimisticPlaying = null; // follow real state for icon
    morphEl?.setAttribute('loading', '');
    this.dispatchEvent(new CustomEvent('play', { bubbles: true }));
  }

  public addFilteredPrompt(prompt: string) {
    this.filteredPrompts = new Set([...this.filteredPrompts, prompt]);
  }

  public setPromptLabels(labels: string[]) {
    const updated = new Map<string, Prompt>();
    let i = 0;
    for (const [key, p] of this.prompts.entries()) {
      const newText = labels[i] ?? p.text;
      updated.set(key, { ...p, text: newText });
      i++;
    }
    this.prompts = updated;
    this.requestUpdate();
    this.dispatchEvent(new CustomEvent('prompts-changed', { detail: this.prompts }));
  }

  public getPrompts(): Map<string, Prompt> {
    return new Map(this.prompts);
  }

  private addExtraSlot(idx: number) {
    const promptId = `extra-${idx}`;
    if (this.prompts.has(promptId)) return;
    const color = ['#9900ff', '#2af6de', '#ff25f6', '#ffdd28'][idx % 4];
    const p: Prompt = { promptId, text: this.trPlaceholder(), weight: 0, cc: 100 + idx, color };
    const updated = new Map(this.prompts);
    updated.set(promptId, p);
    this.prompts = updated;
    const slots = [...this.addSlotsActive];
    slots[idx] = true;
    this.addSlotsActive = slots;
    this.requestUpdate();
    this.saveState();
    this.dispatchEvent(new CustomEvent('prompts-changed', { detail: this.prompts }));
  }

  private addBaseSlot(idx: number) {
    const id = this.baseOrder[idx];
    if (!id || this.prompts.has(id)) return;
    const base = this.basePrompts.get(id);
    if (!base) return;
    const updated = new Map(this.prompts);
    // Always use placeholder text when adding (user deleted the original intentionally)
    const text = this.trPlaceholder();
    updated.set(id, { ...base, text });
    this.prompts = updated;
    const rem = new Set(this.removedSlots);
    rem.delete(id);
    this.removedSlots = rem;
    this.requestUpdate();
    this.saveState();
    this.dispatchEvent(new CustomEvent('prompts-changed', { detail: this.prompts }));
  }

  private clearPrompt(promptId: string) {
    if (!this.prompts.has(promptId)) return;
    if (promptId.startsWith('extra-')) {
      // Remove extra prompt and deactivate slot
      const idx = Number(promptId.split('-')[1] || 0);
      const updated = new Map(this.prompts);
      updated.delete(promptId);
      this.prompts = updated;
      const slots = [...this.addSlotsActive];
      if (!Number.isNaN(idx)) slots[idx] = false;
      this.addSlotsActive = slots;
      this.requestUpdate();
      this.saveState();
      this.dispatchEvent(new CustomEvent('prompts-changed', { detail: this.prompts }));
      return;
    }
    // Remove built-in prompt and mark slot as removed to render add button in-grid
    const updated = new Map(this.prompts);
    updated.delete(promptId);
    this.prompts = updated;
    const rem = new Set(this.removedSlots);
    rem.add(promptId);
    this.removedSlots = rem;
    this.requestUpdate();
    this.saveState();
    this.dispatchEvent(new CustomEvent('prompts-changed', { detail: this.prompts }));
  }

  public resetAll() {
    // Reset to original base prompts
    // BUT maintain the "22 active, 2 empty" rule
    const newPrompts = new Map<string, Prompt>();
    let count = 0;
    for (const [k, p] of this.basePrompts.entries()) {
      if (count < 22) {
        newPrompts.set(k, { ...p });
      }
      count++;
    }
    this.prompts = newPrompts;
    this.addSlotsActive = [false, false, false, false];
    this.removedSlots = new Set();
    this.requestUpdate();
    this.saveState();
    this.dispatchEvent(new CustomEvent('prompts-changed', { detail: this.prompts }));
  }

  private formatDuration(sec: number) {
    if (!sec) return "0:00";
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  private toggleRecording() {
    if (this.recordInterval) {
      clearInterval(this.recordInterval);
      this.recordInterval = null;
    }

    this.isRecording = !this.isRecording;
    if (this.isRecording) {
      this.recordElapsed = 0;
      this.recordAudioElapsed = 0;
      const startTime = Date.now();
      let lastTick = startTime;

      this.recordInterval = window.setInterval(() => {
        const now = Date.now();
        const dt = (now - lastTick) / 1000;
        lastTick = now;

        this.recordElapsed = (now - startTime) / 1000;
        // Sensitivity threshold bumped to 0.02
        if (this.audioLevel > 0.02) {
          this.recordAudioElapsed += dt;
        }
        this.requestUpdate();
      }, 100);

      this.dispatchEvent(new CustomEvent('start-recording'));
    } else {
      this.dispatchEvent(new CustomEvent('stop-recording'));
    }
  }

  private toggleMidiPanel() {
    // Reset MIDI state when toggling on to allow retry after denial
    if (!this.showMidi) {
      this.midiDispatcher.reset();
    }
    this.setShowMidi(!this.showMidi);
  }

  private handleVolumeChange(e: Event) {
    const val = parseFloat((e.target as HTMLInputElement).value);
    this.volume = val;

    // 1. IPC to native
    if ((window as any).ipc) {
      (window as any).ipc.postMessage('set_volume:' + val);
    }

    // 2. Global variable for hooks
    (window as any)._currentVolume = val;

    // 3. Audio tags
    document.querySelectorAll('audio, video').forEach((el) => {
      (el as HTMLMediaElement).volume = val;
    });

    // 4. Captured AudioContext Gains (from mod.rs hook)
    const gains = (window as any)._activeMasterGains;
    if (Array.isArray(gains)) {
      gains.forEach((g: any) => {
        try {
          if (g && g.gain) {
            // smooth transition
            g.gain.setTargetAtTime(val, g.context.currentTime, 0.1);
          }
        } catch {
          if (g && g.gain) g.gain.value = val;
        }
      });
    }
  }


  protected updated(changedProps: Map<string, any>) {
    if (changedProps.has('playbackState')) {
      const state = this.playbackState;

      // Set drift target based on state and ensure the animation loop is running
      this.driftTarget = (state === 'playing' || state === 'loading') ? 1 : 0;
      this.ensureDriftLoop();

      if (this.lastUserAction === 'play') {
        if (state === 'playing') {
          this.optimisticLoading = false;
          this.optimisticPlaying = null;
          this.lastUserAction = null;
        } else if (state === 'loading') {
          this.optimisticLoading = true;
        } else if (state === 'paused' || state === 'stopped') {
          this.optimisticLoading = true;
        }
      } else if (this.lastUserAction === 'pause') {
        this.optimisticLoading = false;
        this.optimisticPlaying = false;
        if (state === 'paused' || state === 'stopped') {
          this.lastUserAction = null;
        }
      } else {
        this.optimisticLoading = (state === 'loading');
        this.optimisticPlaying = null;
      }
    }
  }

  private ensureDriftLoop() {
    if (this.driftRaf != null) return;
    this.lastDriftTick = performance.now();
    const tick = () => {
      const now = performance.now();
      const dt = Math.max(0, now - this.lastDriftTick) / 1000; // seconds
      this.lastDriftTick = now;

      // Approach driftTarget smoothly (exponential smoothing)
      const speed = 3.0; // higher = faster return/engage
      const diff = this.driftTarget - this.driftStrength;
      const step = 1 - Math.exp(-speed * dt);
      this.driftStrength = this.driftStrength + diff * step;

      // Force a re-render so gradients animate (uses performance.now in makeBackground)
      this.requestUpdate();

      // If we're returning to base and very close, stop the loop; otherwise keep running
      if (this.driftTarget === 0 && Math.abs(this.driftStrength) < 0.001) {
        this.driftStrength = 0;
        this.driftRaf = null;
        return;
      }
      this.driftRaf = requestAnimationFrame(tick);
    };
    this.driftRaf = requestAnimationFrame(tick);
  }

  override render() {
    const bg = styleMap({
      backgroundImage: this.makeBackground(),
    });
    const playingProp = this.optimisticPlaying !== null
      ? this.optimisticPlaying
      : (this.playbackState === 'playing');
    const loadingProp = this.optimisticLoading || this.playbackState === 'loading';

    return html`<div id="background" style=${bg}></div>
      <onboarding-popup lang=${this.lang}></onboarding-popup>
      ${this.renderModal()}
      <div id="content">
        <div id="gridWrap">
          <div id="grid">${this.renderPrompts()}</div>
        </div>
        <div id="sideControls">
          <!-- Volume at the top -->
          <div class="volume-container" title="Master Volume">
            <span class="volume-icon">
              ${unsafeHTML(this.volume <= 0.001 ? ICONS.volume_off : this.volume < 0.5 ? ICONS.volume_down : ICONS.volume_up)}
            </span>
            <input type="range" class="volume-slider" min="0" max="1" step="0.01"
              style=${styleMap({
      background: `linear-gradient(to right, #ffffff 0%, #ffffff ${this.volume * 100}%, rgba(255,255,255,0.3) ${this.volume * 100}%, rgba(255,255,255,0.3) 100%)`
    })}
              .value=${this.volume.toString()}
              @input=${this.handleVolumeChange}>
          </div>

          <play-pause-morph
            ?playing=${playingProp}
            ?loading=${loadingProp}
            @play-pause=${this.playPause}
          ></play-pause-morph>

          <div class="mini-controls">
             <!-- MIDI Toggle -->
             <button class="mini-btn ${this.showMidi ? 'toggled' : ''}" @click=${this.toggleMidiPanel} title=${LOCALES[this.lang as Lang].midi_tooltip}>
               ${unsafeHTML(ICONS.piano)}
             </button>

             <!-- Record Toggle -->
             <button class="mini-btn ${this.isRecording ? 'active' : ''}" @click=${this.toggleRecording} title="${this.isRecording ? LOCALES[this.lang as Lang].stop_tooltip : LOCALES[this.lang as Lang].record_tooltip}">
               ${unsafeHTML(this.isRecording ? ICONS.stop : ICONS.radio_button_checked)}
             </button>

             <!-- Reset -->
             <button class="mini-btn" @click=${() => this.resetAll()} title=${LOCALES[this.lang as Lang].reset_tooltip}>
               ${unsafeHTML(ICONS.restart_alt)}
             </button>
          </div>

          <div class="rec-timer-container">
            ${this.isRecording ? html`
               <div class="rec-timer-elapsed">${this.formatDuration(this.recordElapsed)}</div>
               <div class="rec-timer-audio">Audio: ${this.formatDuration(this.recordAudioElapsed)}</div>
            ` : html``}
          </div>
        </div>
      </div>`;
  }

  private renderPromptWithClear(promptId: string) {
    const p = this.prompts.get(promptId);
    if (!p) return html``;
    return html`<div class="pc-wrap">
      <button class="pc-clear" title=${LOCALES[this.lang as Lang].clear_tooltip} @click=${() => this.clearPrompt(promptId)}>
        ${unsafeHTML(ICONS.close)}
      </button>
      <prompt-controller
        promptId=${p.promptId}
        ?filtered=${this.filteredPrompts.has(p.text)}
        cc=${p.cc}
        text=${p.text}
        weight=${p.weight}
        color=${p.color}
        lang=${this.lang}
        .midiDispatcher=${this.midiDispatcher}
        .showCC=${this.showMidi}
        audioLevel=${this.audioLevel}
        @prompt-changed=${this.handlePromptChanged}
      ></prompt-controller>
    </div>`;
  }

  private renderPrompts() {
    const nodes: any[] = [];
    // Render in base grid order, allowing removed slots to show an add button
    this.baseOrder.forEach((id, idx) => {
      const p = this.prompts.get(id);
      if (!p || this.removedSlots.has(id)) {
        nodes.push(html`<button class="add-slot" @click=${() => this.addBaseSlot(idx)} title=${LOCALES[this.lang as Lang].add_tooltip}>
          <span class="add-icon">${unsafeHTML(ICONS.add)}</span>
        </button>`);
      } else {
        nodes.push(this.renderPromptWithClear(id));
      }
    });
    return nodes;
  }
}
