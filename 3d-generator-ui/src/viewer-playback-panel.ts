import type { ModelAnimation } from "./viewer-animation";
import "./viewer-playback.css";

export type PlaybackLabels = {
  animationClip: string; playAnimation: string; pauseAnimation: string;
  animationTime: string; animationSpeed: string; resetPose: string;
};

const DEFAULT_LABELS: PlaybackLabels = {
  animationClip: "Animation", playAnimation: "Play", pauseAnimation: "Pause",
  animationTime: "Animation time", animationSpeed: "Playback speed", resetPose: "Rest pose",
};

export class PlaybackPanel {
  private element = document.createElement("div");
  private clips = document.createElement("select");
  private play = document.createElement("button");
  private seek = document.createElement("input");
  private speed = document.createElement("select");
  private reset = document.createElement("button");
  private time = document.createElement("output");
  private playback: ModelAnimation | null = null;
  private labels = DEFAULT_LABELS;
  private render: () => void;

  constructor(container: HTMLElement, render: () => void) {
    this.render = render;
    this.element.className = "viewer-playback";
    this.element.hidden = true;
    this.clips.className = "viewer-playback-clips";
    this.play.className = "viewer-playback-play";
    this.seek.className = "viewer-playback-seek";
    this.speed.className = "viewer-playback-speed";
    this.reset.className = "viewer-playback-reset";
    this.time.className = "viewer-playback-time";
    this.time.setAttribute("aria-live", "off");
    this.play.type = this.reset.type = "button";
    this.seek.type = "range";
    this.seek.min = "0";
    this.seek.step = "0.001";
    for (const value of [0.25, 0.5, 1, 1.5, 2]) {
      this.speed.add(new Option(`${value}×`, String(value)));
    }
    this.speed.value = "1";
    this.element.append(this.clips, this.play, this.seek, this.time, this.speed, this.reset);
    container.append(this.element);
    this.clips.addEventListener("change", () => this.change(() => {
      if (this.clips.value === "-1") this.playback?.resetPose();
      else this.playback?.select(Number(this.clips.value));
    }));
    this.play.addEventListener("click", () => this.change(() => {
      if (this.playback?.state().selected === -1) this.playback.select(0);
      this.playback?.setPlaying(!this.playback.state().playing);
    }));
    this.seek.addEventListener("input", () => this.change(() => {
      this.playback?.setPlaying(false);
      this.playback?.seek(Number(this.seek.value));
    }));
    this.speed.addEventListener("change", () => this.change(() => {
      this.playback?.setSpeed(Number(this.speed.value));
    }));
    this.reset.addEventListener("click", () => this.change(() => this.playback?.resetPose()));
    this.setLabels(DEFAULT_LABELS);
  }

  setLabels(labels: PlaybackLabels) {
    this.labels = labels;
    this.clips.setAttribute("aria-label", labels.animationClip);
    this.seek.setAttribute("aria-label", labels.animationTime);
    this.speed.setAttribute("aria-label", labels.animationSpeed);
    this.reset.textContent = labels.resetPose;
    if (this.clips.options.length) this.clips.options[0].text = labels.resetPose;
    this.update();
  }

  bind(playback: ModelAnimation | null) {
    this.playback = playback;
    this.clips.replaceChildren();
    this.clips.add(new Option(this.labels.resetPose, "-1"));
    playback?.state().clips.forEach((name, index) => {
      this.clips.add(new Option(name, String(index)));
    });
    this.element.hidden = !playback?.state().clips.length;
    this.update();
  }

  update() {
    const state = this.playback?.state();
    if (!state) return;
    this.play.textContent = state.playing ? this.labels.pauseAnimation : this.labels.playAnimation;
    this.play.setAttribute("aria-pressed", String(state.playing));
    this.clips.value = String(state.selected);
    this.seek.max = String(state.duration);
    this.seek.value = String(state.time);
    this.seek.disabled = state.selected < 0;
    this.time.value = `${state.time.toFixed(1)} / ${state.duration.toFixed(1)}s`;
    this.speed.value = String(state.speed);
  }

  dispose() { this.bind(null); this.element.remove(); }

  private change(action: () => void) { action(); this.update(); this.render(); }
}
