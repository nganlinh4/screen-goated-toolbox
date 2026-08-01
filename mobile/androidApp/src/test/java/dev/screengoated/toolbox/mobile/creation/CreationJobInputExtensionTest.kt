package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Test

class CreationJobInputExtensionTest {
    @Test
    fun `accepted image mime types use matching snapshot extensions`() {
        assertEquals("png", creationImageExtension("image/png"))
        assertEquals("jpg", creationImageExtension("image/jpeg"))
        assertEquals("webp", creationImageExtension("image/webp"))
    }
}
