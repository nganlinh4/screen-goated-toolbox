package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityServiceInfo
import android.view.accessibility.AccessibilityEvent
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityEventDemandTest {
    @Test
    fun idleProfileListensOnlyForSelectionAndFocus() {
        val idle = accessibilityEventProfile(phoneControlActive = false, sdkInt = 34)
        assertEquals(
            AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED or AccessibilityEvent.TYPE_VIEW_FOCUSED,
            idle.eventTypes,
        )
        assertEquals(100L, idle.notificationTimeoutMs)
        assertEquals(0, idle.flags and AccessibilityServiceInfo.FLAG_INPUT_METHOD_EDITOR)
    }

    @Test
    fun declaredServiceConfigStartsWithTheIdleProfile() {
        val config = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .map { File(it, "mobile/androidApp/src/main/res/xml/accessibility_service_config.xml") }
            .first(File::isFile)
        val root = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(config).documentElement
        fun attribute(name: String) = root.getAttributeNS("http://schemas.android.com/apk/res/android", name)
        fun mask(name: String, values: Map<String, Int>) =
            attribute(name).split('|').fold(0) { bits, value -> bits or values.getValue(value) }
        val idle = accessibilityEventProfile(phoneControlActive = false, sdkInt = 34)
        assertEquals(
            idle.eventTypes,
            mask(
                "accessibilityEventTypes",
                mapOf(
                    "typeViewTextSelectionChanged" to AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED,
                    "typeViewFocused" to AccessibilityEvent.TYPE_VIEW_FOCUSED,
                    "typeAllMask" to AccessibilityEvent.TYPES_ALL_MASK,
                ),
            ),
        )
        assertEquals(idle.notificationTimeoutMs, attribute("notificationTimeout").toLong())
        assertEquals(
            idle.flags,
            mask(
                "accessibilityFlags",
                mapOf(
                    "flagReportViewIds" to AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS,
                    "flagIncludeNotImportantViews" to AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS,
                    "flagRetrieveInteractiveWindows" to AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS,
                ),
            ),
        )
    }

    @Test
    fun phoneControlProfileStreamsEveryEventAndAddsTheImeFlagOnlyOnApi33() {
        val active = accessibilityEventProfile(phoneControlActive = true, sdkInt = 33)
        assertEquals(AccessibilityEvent.TYPES_ALL_MASK, active.eventTypes)
        assertEquals(50L, active.notificationTimeoutMs)
        assertTrue(active.flags and AccessibilityServiceInfo.FLAG_INPUT_METHOD_EDITOR != 0)
        val older = accessibilityEventProfile(phoneControlActive = true, sdkInt = 32)
        assertEquals(0, older.flags and AccessibilityServiceInfo.FLAG_INPUT_METHOD_EDITOR)
    }

    @Test
    fun profileFollowsOwnersAndIsReappliedToANewService() {
        val demand = AccessibilityEventDemand()
        val applied = mutableListOf<Pair<String, Boolean>>()
        val owner = Any()

        demand.attach("first") { applied += "first" to it }
        demand.acquire(owner)
        demand.acquire(owner)
        demand.release(owner)
        demand.release(owner)
        assertEquals(listOf("first" to false, "first" to true, "first" to false), applied)

        applied.clear()
        demand.acquire(owner)
        demand.detach("first")
        demand.release(owner)
        demand.acquire(owner)
        demand.attach("second") { applied += "second" to it }
        assertEquals(listOf("first" to true, "second" to true), applied)
    }

    @Test
    fun aStaleServiceCannotDetachItsReplacement() {
        val demand = AccessibilityEventDemand()
        val applied = mutableListOf<Boolean>()
        demand.attach("second") { applied += it }
        demand.detach("first")
        demand.acquire(Any())
        assertEquals(listOf(false, true), applied)
    }
}
