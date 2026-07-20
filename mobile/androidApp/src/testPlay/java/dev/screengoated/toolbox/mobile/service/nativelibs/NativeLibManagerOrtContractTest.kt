package dev.screengoated.toolbox.mobile.service.nativelibs

import org.junit.Assert.assertEquals
import org.junit.Test

class NativeLibManagerOrtContractTest {
    @Test
    fun `play ORT delivery requires the proxy real runtime and shared native module`() {
        val engine = NativeLibManager.Engine.ORT

        assertEquals(
            setOf("libc++_shared.so", "libonnxruntime_real.so", "libonnxruntime.so"),
            engine.libs.toSet(),
        )
        assertEquals(
            setOf("feature_asr_ort", "feature_native_cpp"),
            requiredModulesForPlay(engine).toSet(),
        )
        assertEquals(
            listOf("libc++_shared.so", "libonnxruntime_real.so"),
            NativeLibraryLoadContract.orderedDependencies(engine.libs),
        )
    }
}
