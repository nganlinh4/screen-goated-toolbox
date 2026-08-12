package dev.screengoated.toolbox.mobile.service.nativelibs

import org.junit.Assert.assertEquals
import org.junit.Test

class NativeLibManagerOrtContractTest {
    @Test
    fun `full ORT install contract requires the proxy and real runtime`() {
        val needed = NativeLibManager.Engine.ORT.libs.toSet()

        assertEquals(
            setOf("libonnxruntime_real.so", "libonnxruntime.so"),
            needed,
        )
        assertEquals(
            listOf("libonnxruntime_real.so"),
            NativeLibraryLoadContract.orderedDependencies(NativeLibManager.Engine.ORT.libs),
        )
    }
}
