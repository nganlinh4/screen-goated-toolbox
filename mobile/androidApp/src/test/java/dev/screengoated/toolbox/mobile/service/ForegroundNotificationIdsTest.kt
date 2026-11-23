package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.service.dj.DjPlaybackService
import dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyNotificationFactory
import java.lang.reflect.Modifier
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ForegroundNotificationIdsTest {
    @Test
    fun allServiceIdsAreUnique() {
        val ids = ForegroundNotificationIds::class.java.declaredFields
            // Skip compiler-generated statics such as the Compose `$stable` marker.
            .filter { Modifier.isStatic(it.modifiers) && it.type == Int::class.javaPrimitiveType }
            .filterNot { it.isSynthetic || it.name.startsWith("$") }
            .map { it.getInt(null) }
        assertTrue("registry must list the service ids", ids.size >= 6)
        assertEquals(ids.size, ids.toSet().size)
    }

    @Test
    fun servicesTakeTheirIdsFromTheRegistry() {
        assertEquals(ForegroundNotificationIds.BUBBLE, BubbleService.NOTIFICATION_ID)
        assertEquals(ForegroundNotificationIds.DJ_PLAYBACK, DjPlaybackService.NOTIFICATION_ID)
        assertEquals(ForegroundNotificationIds.LIVE_TRANSLATE, ServiceNotificationFactory.NOTIFICATION_ID)
        assertEquals(
            ForegroundNotificationIds.TRANSLATION_GUMMY,
            TranslationGummyNotificationFactory.NOTIFICATION_ID,
        )
        assertTrue(BubbleService.NOTIFICATION_ID != DjPlaybackService.NOTIFICATION_ID)
    }
}
