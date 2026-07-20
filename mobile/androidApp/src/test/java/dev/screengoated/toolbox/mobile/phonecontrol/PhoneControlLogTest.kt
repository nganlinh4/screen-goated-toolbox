package dev.screengoated.toolbox.mobile.phonecontrol

import org.junit.Assert.assertEquals
import org.junit.Test

class PhoneControlLogTest {
    @Test
    fun `persistent diagnostic fields preserve Unicode and flatten control characters`() {
        assertEquals(
            "bắt đầu 한국어 next",
            PhoneControlLog.normalizeDiagnosticField(" bắt đầu\n한국어\tnext ", 80),
        )
    }

    @Test
    fun `persistent diagnostic fields are bounded`() {
        assertEquals(
            "12345",
            PhoneControlLog.normalizeDiagnosticField("123456789", 5),
        )
    }
}
