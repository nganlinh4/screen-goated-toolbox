package dev.screengoated.toolbox.mobile.creation

import kotlin.coroutines.coroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class CreationThumbnailOwnershipTest {
    @Test
    fun `cancelled decode disposes the produced resource before delivery`() = runBlocking {
        val disposed = mutableListOf<String>()
        val job = launch {
            decodeCreationResourceCancellationSafe(
                Dispatchers.Unconfined,
                disposed::add,
            ) {
                "decoded".also { coroutineContext.cancel() }
            }
        }

        job.join()

        assertEquals(listOf("decoded"), disposed)
    }
}
