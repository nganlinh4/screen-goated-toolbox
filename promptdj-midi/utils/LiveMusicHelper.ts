/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/
import type { PlaybackState, Prompt } from '../types';
import type { AudioChunk, GoogleGenAI, LiveMusicFilteredPrompt, LiveMusicServerMessage, LiveMusicSession } from '@google/genai';
import { decode, decodeAudioData } from './audio';
import { PlaybackVolume } from './PlaybackVolume';

const PROMPT_SYNC_INTERVAL_MS = 200;
const OUTPUT_FADE_SECONDS = 0.1;

export class LiveMusicHelper extends EventTarget {

  private ai: GoogleGenAI;
  private model: string;

  private session: LiveMusicSession | null = null;
  private sessionPromise: Promise<LiveMusicSession> | null = null;

  private connectionError = true;

  private filteredPrompts = new Set<string>();
  private nextStartTime = 0;
  private bufferTime = 2;

  public readonly audioContext: AudioContext;
  public extraDestination: AudioNode | null = null;

  private outputNode: GainNode;
  private readonly playbackVolume: PlaybackVolume;
  private disposed = false;
  private playbackState: PlaybackState = 'stopped';
  private loadingTimer: number | null = null;
  private bufferTimer: number | null = null;
  private retryTimer: number | null = null;
  private retryCount = 0;
  private readonly maxRetries = 3;
  private promptSyncTimer: number | null = null;
  private lastPromptSyncAt = -Infinity;

  private prompts: Map<string, Prompt>;
  private sessionSeq: number = 0; // increments to invalidate stale callbacks

  constructor(ai: GoogleGenAI, model: string, volume = 1) {
    super();
    this.ai = ai;
    this.model = model;
    this.prompts = new Map();
    this.audioContext = new AudioContext({ sampleRate: 48000 });
    this.playbackVolume = new PlaybackVolume(this.audioContext, volume);
    this.outputNode = this.audioContext.createGain();
  }

  public setVolume(volume: number) {
    this.playbackVolume.setVolume(volume);
  }

  public get isDisposed() { return this.disposed; }

  public dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.stop();
    this.playbackVolume.disconnect();
  }

  // DEBUG
  private debug(...args: any[]) { try { console.log('[PDJ][Helper]', ...args); } catch {} }

  private getSession(): Promise<LiveMusicSession> {
    if (!this.sessionPromise) this.sessionPromise = this.connect();
    return this.sessionPromise;
  }

  private async connect(): Promise<LiveMusicSession> {
    // Bump sequence and capture for this connection to ignore stale callbacks on stop()
    const mySeq = ++this.sessionSeq;
    this.sessionPromise = this.ai.live.music.connect({
      model: this.model,
      callbacks: {
        onmessage: async (e: LiveMusicServerMessage) => {
          if (mySeq !== this.sessionSeq) { this.debug('onmessage ignored (stale seq)'); return; }
          this.debug('onmessage', {
            setupComplete: !!e.setupComplete,
            filteredPrompt: !!e.filteredPrompt,
            chunks: e.serverContent?.audioChunks?.length || 0,
          });
          if (e.setupComplete) {
            this.debug('setupComplete received');
            this.connectionError = false;
            this.retryCount = 0;
            if (this.loadingTimer) { clearTimeout(this.loadingTimer); this.loadingTimer = null; }
          }
          if (e.filteredPrompt) {
            this.debug('filteredPrompt received', e.filteredPrompt);
            this.filteredPrompts = new Set([...this.filteredPrompts, e.filteredPrompt.text!])
            this.dispatchEvent(new CustomEvent<LiveMusicFilteredPrompt>('filtered-prompt', { detail: e.filteredPrompt }));
          }
          if (e.serverContent?.audioChunks) {
            if (mySeq !== this.sessionSeq) { this.debug('audioChunks ignored (stale seq)'); return; }
            this.debug('audioChunks received', e.serverContent.audioChunks.length);
            if (this.loadingTimer) { clearTimeout(this.loadingTimer); this.loadingTimer = null; }
            await this.processAudioChunks(e.serverContent.audioChunks);
          }
        },
        onerror: () => {
          if (mySeq !== this.sessionSeq) { this.debug('onerror ignored (stale seq)'); return; }
          this.debug('onerror');
          this.connectionError = true;
          if (this.loadingTimer) { clearTimeout(this.loadingTimer); this.loadingTimer = null; }
          this.stop();
          this.dispatchEvent(new CustomEvent('error', { detail: 'Connection error, please restart audio.' }));
        },
        onclose: () => {
          if (mySeq !== this.sessionSeq) { this.debug('onclose ignored (stale seq)'); return; }
          this.debug('onclose');
          this.connectionError = true;
          
          if (this.retryCount < this.maxRetries) {
            this.retryCount++;
            this.debug(`onclose: retrying connection (${this.retryCount}/${this.maxRetries}) in 1s...`);
            this.stop();
            this.retryTimer = (setTimeout(() => {
              this.retryTimer = null;
              this.play(false);
            }, 1000) as unknown) as number;
            return;
          }

          if (this.loadingTimer) { clearTimeout(this.loadingTimer); this.loadingTimer = null; }
          this.stop();
          this.dispatchEvent(new CustomEvent('error', { detail: 'Connection error, please restart audio.' }));
        },
      },
    });
    return this.sessionPromise;
  }

  private setPlaybackState(state: PlaybackState) {
    this.debug('setPlaybackState ->', state);
    this.playbackState = state;
    this.dispatchEvent(new CustomEvent('playback-state-changed', { detail: state }));
  }

  private isCurrent(generation: number) {
    return !this.disposed && generation === this.sessionSeq;
  }

  private isReceivingAudio() {
    return this.playbackState === 'playing' || this.playbackState === 'loading';
  }

  private clearBufferTimer() {
    if (this.bufferTimer) { clearTimeout(this.bufferTimer); this.bufferTimer = null; }
  }

  private async processAudioChunks(audioChunks: AudioChunk[]) {
    // Only schedule when we're in playing or loading states; ignore if paused or stopped
    if (!this.isReceivingAudio()) {
      this.debug('processAudioChunks: early return due to state', this.playbackState);
      return;
    }

    const generation = this.sessionSeq;
    // Decode the whole message before scheduling so its chunks stay contiguous
    // even when another message is processed concurrently.
    const buffers: AudioBuffer[] = [];
    for (const chunk of audioChunks) {
      if (!chunk.data) { this.debug('processAudioChunks: skipping chunk without data'); continue; }
      buffers.push(await decodeAudioData(decode(chunk.data), this.audioContext, 48000, 2));
    }
    if (!this.isCurrent(generation) || !this.isReceivingAudio()) return;
    for (const buffer of buffers) this.scheduleBuffer(buffer, generation);
  }

  private scheduleBuffer(audioBuffer: AudioBuffer, generation: number) {
    const source = this.audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(this.outputNode);

    const now = this.audioContext.currentTime;
    this.debug('scheduleBuffer: now=', now.toFixed(3), 'nextStartTime=', this.nextStartTime.toFixed(3), 'bufDur=', audioBuffer.duration.toFixed(3));

    if (this.nextStartTime === 0) {
      this.nextStartTime = now + this.bufferTime;
      this.debug('scheduleBuffer: scheduling first start at', this.nextStartTime.toFixed(3), 'bufferTime=', this.bufferTime);
      this.clearBufferTimer();
      this.bufferTimer = (setTimeout(() => {
        this.bufferTimer = null;
        if (!this.isCurrent(generation) || this.playbackState !== 'loading') return;
        this.debug('scheduleBuffer: set playing after buffer');
        this.setPlaybackState('playing');
      }, this.bufferTime * 1000) as unknown) as number;
    }
    if (this.nextStartTime < now) {
      this.debug('scheduleBuffer: fell behind, resetting to loading and rescheduling');
      this.setPlaybackState('loading');
      this.nextStartTime = 0;
      return;
    }
    try {
      source.start(this.nextStartTime);
      this.debug('scheduleBuffer: scheduled source at', this.nextStartTime.toFixed(3));
    } catch (e) {
      this.debug('scheduleBuffer: start() error', e);
    }
    this.nextStartTime += audioBuffer.duration;
  }

  public get activePrompts() {
    return Array.from(this.prompts.values())
      .filter((p) => {
        return !this.filteredPrompts.has(p.text) && p.weight !== 0;
      })
  }

  /**
   * Records the latest prompts at once and sends them at most once per
   * PROMPT_SYNC_INTERVAL_MS. A burst (knob drag, MIDI CC stream) ends with a
   * trailing send, so the session always converges on the last state.
   */
  public setWeightedPrompts(prompts: Map<string, Prompt>): Promise<void> {
    this.prompts = prompts;
    const wait = this.lastPromptSyncAt + PROMPT_SYNC_INTERVAL_MS - Date.now();
    if (wait <= 0) return this.syncPrompts();
    if (this.promptSyncTimer === null) {
      this.promptSyncTimer = (setTimeout(() => {
        this.promptSyncTimer = null;
        void this.syncPrompts();
      }, wait) as unknown) as number;
    }
    return Promise.resolve();
  }

  private clearPromptSyncTimer() {
    if (this.promptSyncTimer !== null) { clearTimeout(this.promptSyncTimer); this.promptSyncTimer = null; }
  }

  private async syncPrompts() {
    this.clearPromptSyncTimer();
    this.lastPromptSyncAt = Date.now();

    if (this.activePrompts.length === 0) {
      this.dispatchEvent(new CustomEvent('error', { detail: 'There needs to be one active prompt to play.' }));
      this.pause();
      return;
    }

    // store the prompts to set later if we haven't connected yet
    // there should be a user interaction before calling setWeightedPrompts
    if (!this.session) return;

    try {
      await this.session.setWeightedPrompts({
        weightedPrompts: this.activePrompts.map(p => ({ text: p.text, weight: p.weight })),
      });
    } catch (e: any) {
      this.dispatchEvent(new CustomEvent('error', { detail: e.message }));
      this.pause();
    }
  }

  public async play(resetRetries = true) {
    if (this.disposed) return;
    this.debug('play() called');
    if (resetRetries) {
      this.retryCount = 0;
    }
    this.setPlaybackState('loading');
    // Start a safety timer: if no audio or setupComplete within 12s, abort and show error
    if (this.loadingTimer) { clearTimeout(this.loadingTimer); this.loadingTimer = null; }
    this.loadingTimer = (setTimeout(() => {
      this.loadingTimer = null;
      this.debug('play() timeout hit (no audio/setupComplete)');
      this.dispatchEvent(new CustomEvent('error', { detail: 'Starting audio timed out. Please check API key/network and try again.' }));
      this.pause();
    }, 12000) as unknown) as number;

    this.debug('play(): awaiting getSession()');
    const pending = this.getSession();
    const generation = this.sessionSeq;
    let session: LiveMusicSession;
    try {
      session = await pending;
    } catch (error) {
      if (!this.isCurrent(generation)) return;
      throw error;
    }
    if (!this.isCurrent(generation)) {
      // Stop (or a newer connection) superseded this one while it was connecting.
      try { session.stop(); } catch {}
      return;
    }
    this.session = session;
    this.debug('play(): got session');
    // Every await below can race with stop()/pause(); only continue while this
    // connection is current and playback is still being requested.
    const stillRequested = () => this.isCurrent(generation) && this.playbackState === 'loading';
    if (!stillRequested()) return;
    this.debug('play(): setWeightedPrompts()');
    // A new session starts without prompts, so send them now, unthrottled.
    await this.syncPrompts();
    if (!stillRequested()) return;
    this.debug('play(): audioContext.resume()');
    await this.audioContext.resume();
    if (!stillRequested()) return;
    this.debug('play(): session.play()');
    session.play();
    this.debug('play(): connect destinations');
    this.playbackVolume.connect(this.outputNode, this.extraDestination);
    this.outputNode.gain.setValueAtTime(0, this.audioContext.currentTime);
    this.outputNode.gain.linearRampToValueAtTime(1, this.audioContext.currentTime + 0.1);
  }

  public pause() {
    this.clearBufferTimer();
    if (this.session) this.session.pause();
    this.setPlaybackState('paused');
    this.fadeOutAndRelease(this.outputNode);
    this.nextStartTime = 0;
    this.outputNode = this.audioContext.createGain();
  }

  /**
   * Fades a replaced output to silence, then detaches it so each pause does not
   * leave a dead node (and the audio already scheduled on it) in the graph.
   */
  private fadeOutAndRelease(output: GainNode) {
    const now = this.audioContext.currentTime;
    output.gain.setValueAtTime(1, now);
    output.gain.linearRampToValueAtTime(0, now + OUTPUT_FADE_SECONDS);
    setTimeout(() => {
      try { output.disconnect(); } catch {}
    }, OUTPUT_FADE_SECONDS * 1000 + 50);
  }

  public stop() {
    // Invalidate any in-flight callbacks from previous connection
    this.sessionSeq++;
    if (this.retryTimer) { clearTimeout(this.retryTimer); this.retryTimer = null; }
    // play() resends the latest prompts to the next session.
    this.clearPromptSyncTimer();
    this.clearBufferTimer();
    try { this.session?.stop(); } catch {}
    // Hard mute and disconnect
    try { this.outputNode.disconnect(); } catch {}
    this.outputNode = this.audioContext.createGain();
    this.nextStartTime = 0;
    this.setPlaybackState('stopped');
    this.session = null;
    this.sessionPromise = null;
    if (this.loadingTimer) { clearTimeout(this.loadingTimer); this.loadingTimer = null; }
    // Suspend AudioContext to fully release audio focus.
    // This allows Bluetooth multipoint devices to switch audio to other devices.
    // play() calls audioContext.resume() before starting, so this is safe.
    try { this.audioContext.suspend(); } catch {}
  }

  public async playPause() {
    switch (this.playbackState) {
      case 'playing':
        return this.pause();
      case 'paused':
      case 'stopped':
        return this.play();
      case 'loading':
        return this.stop();
    }
  }

}
