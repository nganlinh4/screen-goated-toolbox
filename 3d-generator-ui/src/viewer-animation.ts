import * as THREE from "three";

export type PlaybackState = {
  clips: string[];
  selected: number;
  playing: boolean;
  time: number;
  duration: number;
  speed: number;
};

/** Playback owns authored transforms; presentation transforms belong to its parent. */
export class ModelAnimation {
  private mixer: THREE.AnimationMixer;
  private action: THREE.AnimationAction | null = null;
  private selected = -1;
  private playing = false;
  private speed = 1;
  private root: THREE.Object3D;
  private clips: THREE.AnimationClip[];

  constructor(root: THREE.Object3D, clips: THREE.AnimationClip[]) {
    this.root = root;
    this.clips = clips;
    this.mixer = new THREE.AnimationMixer(root);
    if (clips.length) this.select(0);
  }

  state(): PlaybackState {
    return {
      clips: this.clips.map((clip, index) => clip.name || String(index + 1)),
      selected: this.selected,
      playing: this.playing,
      time: this.action?.time ?? 0,
      duration: this.clips[this.selected]?.duration ?? 0,
      speed: this.speed,
    };
  }

  select(index: number) {
    if (!Number.isInteger(index) || index < 0 || index >= this.clips.length) return;
    this.mixer.stopAllAction();
    this.selected = index;
    this.playing = false;
    this.action = this.mixer.clipAction(this.clips[index]).reset().play();
    this.action.paused = true;
    this.mixer.update(0);
  }

  setPlaying(playing: boolean) {
    this.playing = playing && this.action !== null && this.state().duration > 0;
    if (this.action) this.action.paused = !this.playing;
  }

  seek(seconds: number) {
    if (!this.action || !Number.isFinite(seconds)) return;
    const duration = this.state().duration;
    this.action.time = THREE.MathUtils.clamp(seconds, 0, Math.max(0, duration - 1e-7));
    this.mixer.update(0);
    this.root.updateMatrixWorld(true);
  }

  setSpeed(speed: number) {
    if (Number.isFinite(speed) && speed >= 0.25 && speed <= 2) this.speed = speed;
  }

  update(seconds: number) {
    if (!this.playing || !Number.isFinite(seconds) || seconds <= 0) return false;
    this.mixer.update(Math.min(seconds, 0.1) * this.speed);
    this.root.updateMatrixWorld(true);
    return true;
  }

  resetPose() {
    this.playing = false;
    this.mixer.stopAllAction();
    this.action = null;
    this.selected = -1;
    this.root.updateMatrixWorld(true);
  }

  dispose() {
    this.resetPose();
    this.mixer.uncacheRoot(this.root);
    this.clips = [];
  }
}
