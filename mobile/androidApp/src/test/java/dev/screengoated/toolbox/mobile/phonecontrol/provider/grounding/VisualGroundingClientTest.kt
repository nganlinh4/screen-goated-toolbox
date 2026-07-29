package dev.screengoated.toolbox.mobile.phonecontrol.provider.grounding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualGroundingClientTest {
    @Test
    fun namedRecordsRequireEveryExactIdentifier() {
        val records = parseNamedRecords(
            "M|from|100|200|source\nM|to|800|700|destination",
            setOf("from", "to"),
            "model",
        )
        assertEquals(listOf("from", "to"), records?.map(GroundingCoordinate::id))
        assertNull(
            parseNamedRecords(
                "M|from|100|200|source\nN|to",
                setOf("from", "to"),
                "model",
            ),
        )
        assertTrue(
            reportsNotVisible(
                "M|from|100|200|source\nN|to",
                setOf("from", "to"),
            ),
        )
        assertTrue(
            reportsNotVisible(
                "N|from\nN|to",
                setOf("from", "to"),
            ),
        )
        assertFalse(
            reportsNotVisible(
                "M|target|100|200|source\nN|to",
                setOf("target"),
            ),
        )
        assertFalse(
            reportsNotVisible(
                "N|from\nN|from",
                setOf("from", "to"),
            ),
        )
    }

    @Test
    fun openRecordsRejectProseDuplicatesAndOverflow() {
        assertNull(parseOpenRecords("Here are the points", "model"))
        assertNull(
            parseOpenRecords(
                "M|first|100|200\nM|second|104|204",
                "model",
            ),
        )
        val overflow = (0..30).joinToString("\n") { index ->
            "M|target $index|${index * 30}|${index * 30}"
        }
        assertNull(parseOpenRecords(overflow, "model"))
    }

    @Test
    fun openRecordsAcceptEmptyAndReadingOrder() {
        assertTrue(parseOpenRecords("N", "model").orEmpty().isEmpty())
        val records = parseOpenRecords(
            "M|upper|250|100\nM|lower|250|800|",
            "model",
        )
        assertEquals(listOf("upper", "lower"), records?.map(GroundingCoordinate::label))
    }

    @Test
    fun labelsCountUnicodeCodePointsRatherThanUtf16Units() {
        assertTrue(
            parseOpenRecords("M|${"😀".repeat(160)}|100|200", "model")?.size == 1,
        )
        assertNull(parseOpenRecords("M|${"😀".repeat(161)}|100|200", "model"))
    }

    @Test
    fun verificationIsStrictAndConfidenceGated() {
        assertTrue(
            parseVerification(
                """{"matches":true,"confidence":92,"what":"target"}""",
                "model",
            ) is GroundingClientResult.Success,
        )
        assertTrue(
            parseVerification(
                """{"matches":true,"confidence":20,"what":"target"}""",
                "model",
            ) is GroundingClientResult.Failure,
        )
        assertTrue(
            parseVerification("""{"matches":"yes","confidence":92}""", "model") is
                GroundingClientResult.Failure,
        )
        assertTrue(
            parseVerification(
                """prose {"matches":true,"confidence":92,"what":"target"}""",
                "model",
            ) is GroundingClientResult.Failure,
        )
        assertTrue(
            parseVerification(
                """{"matches":true,"confidence":92,"what":"target"}{}""",
                "model",
            ) is GroundingClientResult.Failure,
        )
        assertTrue(
            parseVerification(
                """{"matches":true,"confidence":101,"what":"target"}""",
                "model",
            ) is GroundingClientResult.Failure,
        )
        assertTrue(
            parseVerification(
                """{"matches":true,"confidence":92,"what":"target","extra":1}""",
                "model",
            ) is GroundingClientResult.Failure,
        )
    }
}
