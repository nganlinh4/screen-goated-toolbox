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
            """{"points":[{"id":"from","x":100,"y":200,"label":"source"},{"id":"to","x":800,"y":700,"label":"destination"}],"missing":[]}""",
            setOf("from", "to"),
            "model",
        )
        assertEquals(listOf("from", "to"), records?.map(GroundingCoordinate::id))
        assertNull(
            parseNamedRecords(
                """{"points":[{"id":"from","x":100,"y":200,"label":"source"}],"missing":["to"]}""",
                setOf("from", "to"),
                "model",
            ),
        )
        assertTrue(
            reportsNotVisible(
                """{"points":[{"id":"from","x":100,"y":200,"label":"source"}],"missing":["to"]}""",
                setOf("from", "to"),
            ),
        )
        assertTrue(
            reportsNotVisible(
                """{"points":[],"missing":["from","to"]}""",
                setOf("from", "to"),
            ),
        )
        assertFalse(
            reportsNotVisible(
                """{"points":[{"id":"target","x":100,"y":200,"label":"source"}],"missing":["to"]}""",
                setOf("target"),
            ),
        )
        assertFalse(
            reportsNotVisible(
                """{"points":[],"missing":["from","from"]}""",
                setOf("from", "to"),
            ),
        )
    }

    @Test
    fun openRecordsRejectProseDuplicatesAndOverflow() {
        assertNull(parseOpenRecords("Here are the points", "model"))
        assertNull(
            parseOpenRecords(
                """{"points":[{"x":100,"y":200,"label":"first"},{"x":104,"y":204,"label":"second"}]}""",
                "model",
            ),
        )
        val overflow = (0..30).joinToString(",", prefix = "{\"points\":[", postfix = "]}") { index ->
            "{\"x\":${index * 30},\"y\":${index * 30},\"label\":\"target $index\"}"
        }
        assertNull(parseOpenRecords(overflow, "model"))
    }

    @Test
    fun openRecordsAcceptEmptyAndReadingOrder() {
        assertTrue(parseOpenRecords("""{"points":[]}""", "model").orEmpty().isEmpty())
        val records = parseOpenRecords(
            """{"points":[{"x":250,"y":100,"label":"upper"},{"x":250,"y":800,"label":"lower"}]}""",
            "model",
        )
        assertEquals(listOf("upper", "lower"), records?.map(GroundingCoordinate::label))
    }

    @Test
    fun labelsCountUnicodeCodePointsRatherThanUtf16Units() {
        assertTrue(
            parseOpenRecords("""{"points":[{"x":100,"y":200,"label":"${"😀".repeat(160)}"}]}""", "model")?.size == 1,
        )
        assertNull(parseOpenRecords("""{"points":[{"x":100,"y":200,"label":"${"😀".repeat(161)}"}]}""", "model"))
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
