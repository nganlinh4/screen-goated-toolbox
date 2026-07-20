package dev.screengoated.toolbox.mobile.service.nativelibs

import org.junit.Assert.assertEquals
import org.junit.Test

class NativeLibraryLoadContractTest {
    @Test
    fun `future runtime libraries remain loadable in declared order`() {
        assertEquals(
            listOf("libc++_shared.so", "libfuture-alpha.so", "libfuture-beta.so"),
            NativeLibraryLoadContract.orderedDependencies(
                listOf(
                    "libfuture-alpha.so",
                    "libonnxruntime.so",
                    "libc++_shared.so",
                    "libfuture-beta.so",
                ),
            ),
        )
    }
}
