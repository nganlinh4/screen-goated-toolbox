package dev.screengoated.toolbox.mobile.preset

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

internal object PresetAudioCodec {
    private const val SAMPLE_RATE_HZ = 16_000
    private const val CHANNEL_COUNT = 1
    private const val BITS_PER_SAMPLE = 16

    fun encodePcm16MonoWav(samples: ShortArray): ByteArray {
        val dataSize = samples.size * 2
        val byteRate = SAMPLE_RATE_HZ * CHANNEL_COUNT * (BITS_PER_SAMPLE / 8)
        val blockAlign = CHANNEL_COUNT * (BITS_PER_SAMPLE / 8)
        val output = ByteArrayOutputStream(44 + dataSize)

        fun writeAscii(text: String) {
            output.write(text.toByteArray(Charsets.US_ASCII))
        }

        fun writeIntLe(value: Int) {
            output.write(
                ByteBuffer.allocate(4)
                    .order(ByteOrder.LITTLE_ENDIAN)
                    .putInt(value)
                    .array(),
            )
        }

        fun writeShortLe(value: Int) {
            output.write(
                ByteBuffer.allocate(2)
                    .order(ByteOrder.LITTLE_ENDIAN)
                    .putShort(value.toShort())
                    .array(),
            )
        }

        writeAscii("RIFF")
        writeIntLe(36 + dataSize)
        writeAscii("WAVE")
        writeAscii("fmt ")
        writeIntLe(16)
        writeShortLe(1)
        writeShortLe(CHANNEL_COUNT)
        writeIntLe(SAMPLE_RATE_HZ)
        writeIntLe(byteRate)
        writeShortLe(blockAlign)
        writeShortLe(BITS_PER_SAMPLE)
        writeAscii("data")
        writeIntLe(dataSize)
        samples.forEach { sample -> writeShortLe(sample.toInt()) }
        return output.toByteArray()
    }

    fun decodePcm16MonoWav(wavBytes: ByteArray): ShortArray {
        require(wavBytes.size >= 44) { "WAV payload is too small." }
        require(
            wavBytes.copyOfRange(0, 4).contentEquals(byteArrayOf('R'.code.toByte(), 'I'.code.toByte(), 'F'.code.toByte(), 'F'.code.toByte())) &&
                wavBytes.copyOfRange(8, 12).contentEquals(byteArrayOf('W'.code.toByte(), 'A'.code.toByte(), 'V'.code.toByte(), 'E'.code.toByte())),
        ) {
            "Unsupported WAV container."
        }

        var offset = 12
        var dataOffset = -1
        var dataSize = -1
        while (offset + 8 <= wavBytes.size) {
            val chunkId = String(wavBytes, offset, 4, Charsets.US_ASCII)
            val chunkSize = ByteBuffer.wrap(wavBytes, offset + 4, 4)
                .order(ByteOrder.LITTLE_ENDIAN)
                .int
            val chunkDataStart = offset + 8
            if (chunkId == "data") {
                dataOffset = chunkDataStart
                dataSize = chunkSize
                break
            }
            offset = chunkDataStart + chunkSize + (chunkSize and 1)
        }

        require(dataOffset >= 0 && dataSize >= 0 && dataOffset + dataSize <= wavBytes.size) {
            "WAV payload is missing audio data."
        }

        val sampleCount = dataSize / 2
        val samples = ShortArray(sampleCount)
        val buffer = ByteBuffer.wrap(wavBytes, dataOffset, dataSize).order(ByteOrder.LITTLE_ENDIAN)
        for (index in 0 until sampleCount) {
            samples[index] = buffer.short
        }
        return samples
    }
}
