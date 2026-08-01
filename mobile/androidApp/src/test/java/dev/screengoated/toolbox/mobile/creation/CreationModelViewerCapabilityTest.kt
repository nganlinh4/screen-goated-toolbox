package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationModelViewerCapabilityTest {
    @Test
    fun acceptsMatchingNativeAbi() {
        assertTrue(supportsNative3dPreview(listOf("arm64-v8a"), "/data/app/example/lib/arm64"))
        assertTrue(supportsNative3dPreview(listOf("x86_64", "arm64-v8a"), "/data/app/example/lib/x86_64"))
    }

    @Test
    fun rejectsTranslatedNativeAbi() {
        assertFalse(
            supportsNative3dPreview(
                listOf("x86_64", "arm64-v8a"),
                "/data/app/example/lib/arm64",
            ),
        )
    }
}
