package dev.screengoated.toolbox.mobile.phonecontrol

import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlAdbParityContractTest {
    @Test
    fun `debug deadline and adb terminal truth match the shared authority fixture`() {
        val root = PhoneControlAuthorityFixture.load().root
        val debugProbe = root.getValue("debugProbeHarness").jsonObject

        assertTrue(debugProbe.getValue("hostSuppliesExecutionDeadline").jsonPrimitive.boolean)
        assertEquals(8_000L, debugProbe.long("deviceDefaultExecutionDeadlineMs"))
        assertEquals(250L, debugProbe.long("minimumExecutionDeadlineMs"))
        assertEquals(118_000L, debugProbe.long("maximumExecutionDeadlineMs"))
        assertEquals(1_500L, debugProbe.long("hostReceiptMarginMs"))

        val adbCommand = root.getValue("firstPartyAdbCommand").jsonObject
        assertEquals(
            "exact_per_operation_status_marker_line",
            adbCommand.getValue("terminalSignal").jsonPrimitive.content,
        )
        assertFalse(adbCommand.getValue("transportEofRequired").jsonPrimitive.boolean)
        assertTrue(adbCommand.getValue("markerLineTerminatorRequired").jsonPrimitive.boolean)

        val receipt = adbCommand.getValue("authorityReceipt").jsonObject
        assertTrue(receipt.getValue("ok").jsonPrimitive.boolean)
        assertEquals("process_exited", receipt.getValue("code").jsonPrimitive.content)
        assertFalse(receipt.getValue("timedOut").jsonPrimitive.boolean)
        assertFalse(receipt.getValue("cancelled").jsonPrimitive.boolean)
        assertEquals(2_000L, receipt.long("authorityUid"))
        assertEquals(2_000L, receipt.long("outputUid"))
    }

    private fun kotlinx.serialization.json.JsonObject.long(name: String): Long =
        getValue(name).jsonPrimitive.long
}
