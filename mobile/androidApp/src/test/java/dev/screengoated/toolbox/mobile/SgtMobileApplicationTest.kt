package dev.screengoated.toolbox.mobile

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SgtMobileApplicationTest {
    @Test
    fun `only the exact package process owns the full application container`() {
        val packageName = "example.product"

        assertTrue(isPrimaryApplicationProcess(packageName, packageName))
        assertFalse(isPrimaryApplicationProcess(packageName, "$packageName:service"))
        assertFalse(isPrimaryApplicationProcess(packageName, "example.worker"))
        assertFalse(isPrimaryApplicationProcess(packageName, ""))
    }
}
