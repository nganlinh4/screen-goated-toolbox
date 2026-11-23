/** Prompt DJ's speaker gain; the optional recording tap stays before this gain. */
export class PlaybackVolume {
  private readonly gain: GainNode;
  private readonly context: AudioContext;

  constructor(context: AudioContext, initialVolume: number) {
    this.context = context;
    this.gain = context.createGain();
    this.gain.gain.value = initialVolume;
    this.gain.connect(context.destination);
  }

  setVolume(value: number) {
    this.gain.gain.setTargetAtTime(value, this.context.currentTime, 0.1);
  }

  connect(source: AudioNode, recordingTap: AudioNode | null) {
    source.connect(this.gain);
    if (recordingTap) source.connect(recordingTap);
  }

  disconnect() {
    this.gain.disconnect();
  }
}
