package dev.screengoated.toolbox.mobile.service.nativelibs

import org.junit.Assert.assertEquals
import org.junit.Test

class NativeLibManagerOrtContractTest {
    @Test
    fun `play ORT delivery requires only the proxy and real runtime feature`() {
        val engine = NativeLibManager.Engine.ORT

        assertEquals(
            setOf("libonnxruntime_real.so", "libonnxruntime.so"),
            engine.libs.toSet(),
        )
        assertEquals(
            setOf("feature_asr_ort"),
            requiredModulesForPlay(engine).toSet(),
        )
        assertEquals(
            listOf("libonnxruntime_real.so"),
            NativeLibraryLoadContract.orderedDependencies(engine.libs),
        )
    }
}
