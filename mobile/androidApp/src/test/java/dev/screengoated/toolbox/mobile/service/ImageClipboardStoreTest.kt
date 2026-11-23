package dev.screengoated.toolbox.mobile.service

import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ImageClipboardStoreTest {
    @get:Rule val temp = TemporaryFolder()

    @Test
    fun copiesAreImmutableAndUncertainPublicationIsRetained() {
        val dir = temp.newFolder()
        var published: File? = null
        fun copy(bytes: ByteArray) = ImageClipboardStore.publish(dir, bytes) { file ->
            { published = file }
        }
        assertTrue(copy(byteArrayOf(1)))
        val first = requireNotNull(published)
        assertTrue(copy(byteArrayOf(2)))
        val second = requireNotNull(published)
        assertNotEquals(first, second)
        assertArrayEquals(byteArrayOf(1), first.readBytes())
        assertArrayEquals(byteArrayOf(2), second.readBytes())

        var uncertain: File? = null
        assertFalse(ImageClipboardStore.publish(dir, byteArrayOf(3)) { file ->
            uncertain = file
            { throw IllegalStateException("clipboard publication failed") }
        })
        assertEquals(second, published)
        assertArrayEquals(byteArrayOf(2), second.readBytes())
        assertArrayEquals(byteArrayOf(3), requireNotNull(uncertain).readBytes())
    }

    @Test
    fun preparationFailureIsCleanedUp() {
        val dir = temp.newFolder()
        assertFalse(ImageClipboardStore.publish(dir, byteArrayOf(1)) { throw IllegalStateException("prepare") })
        assertTrue(requireNotNull(dir.listFiles()).isEmpty())
    }

    @Test
    fun fullStoreEvictsTheOldestCopyInsteadOfBlockingNewCopies() {
        val dir = temp.newFolder()
        val published = mutableListOf<File>()
        repeat(64) { index ->
            assertTrue(ImageClipboardStore.publish(dir, byteArrayOf(index.toByte())) { file -> { published += file } })
        }
        var attempted = false
        assertTrue(ImageClipboardStore.publish(dir, byteArrayOf(64)) { file -> { attempted = true; published += file } })
        assertTrue(attempted)
        val remaining = requireNotNull(dir.listFiles()).toSet()
        assertEquals(64, remaining.size)
        assertFalse("the oldest copy is evicted", published.first() in remaining)
        assertTrue("recent copies keep their bytes", published.takeLast(8).all { it in remaining })
        assertArrayEquals(byteArrayOf(63), published[63].readBytes())
    }

    @Test
    fun newestCopiesAreNeverEvictedEvenWhenTheyFillTheByteBudget() {
        val dir = temp.newFolder()
        val chunk = ByteArray(16 * 1024 * 1024)
        repeat(8) { assertTrue(ImageClipboardStore.publish(dir, chunk) { { } }) }
        val before = requireNotNull(dir.listFiles()).toSet()
        assertFalse(ImageClipboardStore.publish(dir, byteArrayOf(1)) { { } })
        assertEquals(before, requireNotNull(dir.listFiles()).toSet())
    }
}
