package dev.screengoated.toolbox.mobile.service.tts

import android.media.MediaCodec
import android.media.MediaDataSource
import android.media.MediaExtractor
import android.media.MediaFormat
import dev.screengoated.toolbox.mobile.service.ShortSampleBuffer
import java.nio.ByteBuffer

internal class Mp3Decoder {
    fun decodeToMonoPcm24k(mp3Bytes: ByteArray): ByteArray {
        if (mp3Bytes.isEmpty()) {
            return ByteArray(0)
        }

        val extractor = MediaExtractor()
        val dataSource = ByteArrayMediaDataSource(mp3Bytes)
        var createdDecoder: MediaCodec? = null
        var sourceRate = 24_000
        var channelCount = 1

        try {
            extractor.setDataSource(dataSource)
            val audioTrackIndex = (0 until extractor.trackCount)
                .firstOrNull { index ->
                    extractor.getTrackFormat(index)
                        .getString(MediaFormat.KEY_MIME)
                        ?.startsWith("audio/") == true
                } ?: error("No audio track")

            extractor.selectTrack(audioTrackIndex)
            val format = extractor.getTrackFormat(audioTrackIndex)
            val mime = format.getString(MediaFormat.KEY_MIME) ?: error("Audio track has no MIME type")
            val codec = MediaCodec.createDecoderByType(mime)
            createdDecoder = codec
            codec.configure(format, null, null, 0)
            codec.start()
            sourceRate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
            channelCount = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        } catch (_: Throwable) {
            // Every setup failure, including a missing track, releases what was
            // created. A codec that failed to configure or start is a
            // hardware resource too; it must not leak.
            createdDecoder?.let { codec -> runCatching { codec.release() } }
            extractor.release()
            dataSource.close()
            return ByteArray(0)
        }

        val decoder = requireNotNull(createdDecoder)
        val bufferInfo = MediaCodec.BufferInfo()
        val samples = ShortSampleBuffer()
        var inputEnded = false
        var outputEnded = false

        try {
            while (!outputEnded) {
                if (!inputEnded) {
                    val inputIndex = decoder.dequeueInputBuffer(DEQUEUE_TIMEOUT_US)
                    if (inputIndex >= 0) {
                        val inputBuffer = decoder.getInputBuffer(inputIndex) ?: ByteBuffer.allocate(0)
                        val sampleSize = extractor.readSampleData(inputBuffer, 0)
                        if (sampleSize < 0) {
                            decoder.queueInputBuffer(
                                inputIndex,
                                0,
                                0,
                                0,
                                MediaCodec.BUFFER_FLAG_END_OF_STREAM,
                            )
                            inputEnded = true
                        } else {
                            decoder.queueInputBuffer(
                                inputIndex,
                                0,
                                sampleSize,
                                extractor.sampleTime,
                                0,
                            )
                            extractor.advance()
                        }
                    }
                }

                when (val outputIndex = decoder.dequeueOutputBuffer(bufferInfo, DEQUEUE_TIMEOUT_US)) {
                    MediaCodec.INFO_TRY_AGAIN_LATER -> Unit
                    MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        val format = decoder.outputFormat
                        sourceRate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                        channelCount = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                    }
                    else -> {
                        if (outputIndex >= 0) {
                            val outputBuffer = decoder.getOutputBuffer(outputIndex)
                            if (outputBuffer != null && bufferInfo.size > 0) {
                                outputBuffer.position(bufferInfo.offset)
                                outputBuffer.limit(bufferInfo.offset + bufferInfo.size)
                                val shortBuffer = outputBuffer.asShortBuffer()
                                val chunk = ShortArray(shortBuffer.remaining())
                                shortBuffer.get(chunk)
                                mixToMono(chunk, channelCount, samples)
                            }
                            decoder.releaseOutputBuffer(outputIndex, false)
                            if (bufferInfo.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                                outputEnded = true
                            }
                        }
                    }
                }
            }
        } finally {
            runCatching { decoder.stop() }
            runCatching { decoder.release() }
            extractor.release()
            dataSource.close()
        }

        if (samples.size == 0) {
            return ByteArray(0)
        }

        val monoSamples = samples.toShortArray()
        val outputSamples = if (sourceRate == 24_000) {
            monoSamples
        } else {
            resampleTo24k(monoSamples, sourceRate)
        }
        return shortsToLeBytes(outputSamples)
    }

    private fun mixToMono(
        chunk: ShortArray,
        channelCount: Int,
        sink: ShortSampleBuffer,
    ) {
        if (channelCount <= 1) {
            sink.addAll(chunk)
            return
        }
        var index = 0
        while (index < chunk.size) {
            var total = 0
            var actualChannels = 0
            repeat(channelCount) {
                if (index < chunk.size) {
                    total += chunk[index].toInt()
                    actualChannels += 1
                    index += 1
                }
            }
            sink.add((total / actualChannels.coerceAtLeast(1)).toShort())
        }
    }

    private fun resampleTo24k(
        input: ShortArray,
        sourceRate: Int,
    ): ShortArray {
        if (input.isEmpty() || sourceRate <= 0) {
            return input
        }
        val ratio = 24_000.0 / sourceRate.toDouble()
        val outputSize = (input.size * ratio).toInt().coerceAtLeast(1)
        val output = ShortArray(outputSize)
        for (index in 0 until outputSize) {
            val sourceIndex = index / ratio
            val floor = sourceIndex.toInt().coerceIn(0, input.lastIndex)
            val ceil = (floor + 1).coerceAtMost(input.lastIndex)
            val fraction = (sourceIndex - floor).toFloat()
            val interpolated = input[floor] + ((input[ceil] - input[floor]) * fraction).toInt()
            output[index] = interpolated.toShort()
        }
        return output
    }

    private fun shortsToLeBytes(samples: ShortArray): ByteArray {
        val bytes = ByteArray(samples.size * 2)
        samples.forEachIndexed { index, sample ->
            val byteIndex = index * 2
            bytes[byteIndex] = (sample.toInt() and 0xFF).toByte()
            bytes[byteIndex + 1] = ((sample.toInt() shr 8) and 0xFF).toByte()
        }
        return bytes
    }

    private class ByteArrayMediaDataSource(
        private val bytes: ByteArray,
    ) : MediaDataSource() {
        override fun readAt(
            position: Long,
            buffer: ByteArray,
            offset: Int,
            size: Int,
        ): Int {
            if (position >= bytes.size) {
                return -1
            }
            val available = (bytes.size - position.toInt()).coerceAtMost(size)
            bytes.copyInto(buffer, offset, position.toInt(), position.toInt() + available)
            return available
        }

        override fun getSize(): Long = bytes.size.toLong()

        override fun close() = Unit
    }

    private companion object {
        private const val DEQUEUE_TIMEOUT_US = 10_000L
    }
}
