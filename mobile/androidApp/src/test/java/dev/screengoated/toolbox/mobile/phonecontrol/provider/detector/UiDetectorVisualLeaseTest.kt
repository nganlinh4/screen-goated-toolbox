package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UiDetectorVisualLeaseTest {
    @Test
    fun ambientNoiseKeepsTheSameTargetLease() {
        val baseline = signature { index -> index % 251 }
        val minorNoise = signature { index -> (index % 251) + if (index % 17 == 0) 8 else 0 }

        assertTrue(baseline.matches(minorNoise))
    }

    @Test
    fun changedTargetInvalidatesTheLease() {
        val baseline = signature { 0 }
        val changed = signature { index -> if (index < SIGNATURE_BYTES / 4) 255 else 0 }

        assertFalse(baseline.matches(changed))
    }
}

private fun signature(value: (Int) -> Int): UiDetectorVisualSignature =
    UiDetectorVisualSignature(
        ByteArray(SIGNATURE_BYTES) { index -> value(index).coerceIn(0, 255).toByte() },
    )

private const val SIGNATURE_BYTES = 16 * 16 * 3
