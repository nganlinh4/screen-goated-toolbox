/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/

/** The MediaRecorder surface a recording session drives. */
export interface RecorderLike {
  readonly mimeType: string;
  ondataavailable: ((event: BlobEvent) => void) | null;
  onstop: ((event: Event) => void) | null;
  start(timeslice?: number): void;
}

interface CaptureDestination {
  readonly stream: MediaStream;
}

interface RecordingSessionOptions<R extends RecorderLike, D extends CaptureDestination> {
  /** The node that fans playback out to the analyser and recorders. */
  tee: { connect(destination: NoInfer<D>): unknown; disconnect(destination: NoInfer<D>): void };
  context: { createMediaStreamDestination(): D };
  createRecorder: (stream: MediaStream) => R;
  onStopped: (blob: Blob, chunkCount: number) => Promise<void>;
}

/**
 * Taps the playback graph into a new MediaRecorder. The capture destination is
 * disconnected once the recording has been handed off (or failed to start);
 * a connected destination keeps pulling audio for the life of the context.
 */
export function startRecordingSession<R extends RecorderLike, D extends CaptureDestination>(
  options: RecordingSessionOptions<R, D>,
): R {
  const { tee, context, createRecorder, onStopped } = options;
  const destination = context.createMediaStreamDestination();
  tee.connect(destination);
  let connected = true;
  const release = () => {
    if (!connected) return;
    connected = false;
    try {
      tee.disconnect(destination);
    } catch {
      // A closed context has already torn the graph down.
    }
  };
  try {
    const recorder = createRecorder(destination.stream);
    const chunks: BlobPart[] = [];
    recorder.ondataavailable = (event) => {
      if (event.data && event.data.size > 0) chunks.push(event.data);
    };
    recorder.onstop = () => {
      const blob = new Blob(chunks, { type: recorder.mimeType || 'audio/webm' });
      onStopped(blob, chunks.length)
        .catch((error: unknown) => console.error('[PDJ] Recording hand-off failed:', error))
        .finally(release);
    };
    recorder.start(250);
    return recorder;
  } catch (error) {
    release();
    throw error;
  }
}
