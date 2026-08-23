package dev.screengoated.toolbox.mobile.preset

import org.junit.Assert.assertEquals
import org.junit.Test

class VisionRepetitionGuardTest {
    @Test
    fun salvagesObservedBrokenRestatements() {
        val cases = listOf(
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png\n" +
                "Screensho\not 2026-08-19 21\n13759.png\nScreensh\nhot 2026-08-19 2\n213630.png" to
                "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png",
            "Điều khiển máy tính\nĐiều khi\nển máy tính\nĐiều khiển má\ny tính" to
                "Điều khiển máy tính",
            "DJI_0872.JPG\nDJI\n_087\n2.JPG" to "DJI_0872.JPG",
        )
        cases.forEach { (corrupted, expected) ->
            assertEquals(expected, salvageVisionRestatement(corrupted))
        }
    }

    @Test
    fun keepsLegitimateRepetition() {
        val cases = listOf(
            "DJI_0872.JPG\nDJI_0872.JPG",
            "Configuration\nConfig\nConfiguration\nConfig",
            "Total: $5.00\nTotal: $5.00\nTotal: $5.00\nTotal: $5.00",
            "STOP STOP STOP",
        )
        cases.forEach { text -> assertEquals(text, salvageVisionRestatement(text)) }
    }
}
