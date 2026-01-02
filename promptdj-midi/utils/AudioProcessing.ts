/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

// Helper to write string to DataView
function writeString(view: DataView, offset: number, string: string) {
    for (let i = 0; i < string.length; i++) {
        view.setUint8(offset + i, string.charCodeAt(i));
    }
}

// Convert AudioBuffer to WAV Blob
export function audioBufferToWav(buffer: AudioBuffer): Blob {
    const numOfChan = buffer.numberOfChannels;
    const length = buffer.length * numOfChan * 2 + 44;
    const bufferArr = new ArrayBuffer(length);
    const view = new DataView(bufferArr);
    const channels = [];
    let i;
    let sample;
    let offset = 0;
    let pos = 0;

    // write WAVE header
    writeString(view, 0, 'RIFF');
    view.setUint32(4, 36 + buffer.length * numOfChan * 2, true);
    writeString(view, 8, 'WAVE');
    writeString(view, 12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, numOfChan, true);
    view.setUint32(24, buffer.sampleRate, true);
    view.setUint32(28, buffer.sampleRate * 2 * numOfChan, true);
    view.setUint16(32, numOfChan * 2, true);
    view.setUint16(34, 16, true);
    writeString(view, 36, 'data');
    view.setUint32(40, buffer.length * numOfChan * 2, true);

    // write interleaved data
    for (i = 0; i < buffer.numberOfChannels; i++) {
        channels.push(buffer.getChannelData(i));
    }

    offset = 44;
    while (pos < buffer.length) {
        for (i = 0; i < numOfChan; i++) {
            // clamp
            sample = Math.max(-1, Math.min(1, channels[i][pos]));
            // scale to 16-bit
            sample = (0.5 + sample < 0 ? sample * 32768 : sample * 32767) | 0;
            view.setInt16(offset, sample, true);
            offset += 2;
        }
        pos++;
    }

    return new Blob([view], { type: 'audio/wav' });
}

// Trim silence from start and end
export function trimSilence(buffer: AudioBuffer, threshold = 0.02): AudioBuffer {
    const numChannels = buffer.numberOfChannels;
    let start = 0;
    let end = buffer.length;

    // Find start
    let foundStart = false;
    for (let i = 0; i < buffer.length; i++) {
        let max = 0;
        for (let c = 0; c < numChannels; c++) {
            const v = Math.abs(buffer.getChannelData(c)[i]);
            if (v > max) max = v;
        }
        if (max > threshold) {
            start = i;
            foundStart = true;
            break;
        }
    }

    if (!foundStart) return null;

    // Find end
    let foundEnd = false;
    for (let i = buffer.length - 1; i >= start; i--) {
        let max = 0;
        for (let c = 0; c < numChannels; c++) {
            const v = Math.abs(buffer.getChannelData(c)[i]);
            if (v > max) max = v;
        }
        if (max > threshold) {
            end = i + 1;
            foundEnd = true;
            break;
        }
    }

    if (!foundEnd) return null;

    const length = end - start;
    if (length <= 0) {
        // Return null if completely silent
        return null;
    }

    const newBuffer = new AudioContext().createBuffer(
        numChannels,
        length,
        buffer.sampleRate
    );

    for (let c = 0; c < numChannels; c++) {
        const chanData = buffer.getChannelData(c);
        const newChanData = newBuffer.getChannelData(c);
        // copy slice
        for (let i = 0; i < length; i++) {
            newChanData[i] = chanData[start + i];
        }
    }

    return newBuffer;
}

export async function processAudioBlob(blob: Blob, context: AudioContext): Promise<Blob> {
    const arrayBuffer = await blob.arrayBuffer();
    const audioBuffer = await context.decodeAudioData(arrayBuffer);
    const trimmed = trimSilence(audioBuffer);
    if (!trimmed) {
        throw new Error('No audio recorded (silence).');
    }
    return audioBufferToWav(trimmed);
}
