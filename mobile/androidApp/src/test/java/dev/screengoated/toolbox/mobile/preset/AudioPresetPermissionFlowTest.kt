package dev.screengoated.toolbox.mobile.preset

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.w3c.dom.Element

class AudioPresetPermissionFlowTest {
    @Test
    fun `permission outcomes follow the shared recovery contract`() {
        val fixture = Json.parseToJsonElement(repoFile("parity-fixtures/preset-system/audio-permission-recovery.json").readText()).jsonObject
        val request = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "owner")
        fixture.getValue("cases").jsonArray.map { it.jsonObject }.forEach { case ->
            val entry = case.getValue("entry").jsonPrimitive.content
            if (entry == "external_launcher_extra") return@forEach
            if (entry == "owner_stopped" || entry == "stop_before_delivery" || entry == "stop_requested_before_destroy") {
                val store = AudioPresetLaunchStore()
                var calls = 0
                store.registerOwner("owner") { calls++ }
                store.set(request)
                assertEquals(AudioPermissionDecision.RESUME_CAPTURE, audioPermissionDecision(request.requestId, store.peek(), true))
                store.releaseOwner("owner")
                store.resumeIfOwned(request.requestId)
                assertEquals(0, calls)
                assertEquals(AudioPermissionDecision.IGNORE, audioPermissionDecision(request.requestId, store.peek(), true))
                return@forEach
            }
            if (entry.startsWith("input_window_")) {
                val dictation = request.copy(kind = AudioPresetLaunchKind.INPUT_DICTATION, inputWindowId = "input")
                val target = if (entry == "input_window_unchanged") "input" else "replacement"
                val expected = if (case.getValue("effect").jsonPrimitive.content == "resume_dictation") {
                    AudioPresetLaunchKind.INPUT_DICTATION
                } else null
                assertEquals(expected, audioResumeKind(dictation, "owner", target))
                return@forEach
            }
            val pending = if (case.getValue("pending").jsonPrimitive.content == "matching_capture") request else null
            val decision = audioPermissionDecision(request.requestId, pending, entry == "internal_permission_granted")
            val expected = when (case.getValue("effect").jsonPrimitive.content) {
                "resume_capture" -> AudioPermissionDecision.RESUME_CAPTURE
                "clear_capture" -> AudioPermissionDecision.CLEAR_CAPTURE
                else -> AudioPermissionDecision.IGNORE
            }
            assertEquals(expected, decision)
        }
    }

    @Test
    fun `stop request revokes owner before asynchronous service destruction`() {
        val store = AudioPresetLaunchStore()
        val request = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "owner")
        var resumed = false
        var stopRequested = false
        store.registerOwner("owner") { resumed = true }
        assertEquals(true, store.set(request))
        store.stopCurrentOwnerThen {
            stopRequested = true
            // The service has not been destroyed yet; a late permission callback arrives now.
            store.resumeIfOwned(request.requestId)
        }
        assertEquals(true, stopRequested)
        assertEquals(false, resumed)
        assertNull(store.peek())
    }

    @Test
    fun `replaced request cannot be consumed or cleared by an older handoff`() {
        val store = AudioPresetLaunchStore()
        store.registerOwner("owner") {}
        val old = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "owner")
        val replacement = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "owner")
        assertNotEquals(old.requestId, replacement.requestId)
        store.set(old)
        store.set(replacement)
        assertEquals(AudioPermissionDecision.IGNORE, audioPermissionDecision(old.requestId, store.peek(), true))
        assertNull(store.takeIfId(old.requestId, "owner"))
        store.clearIfId(old.requestId)
        assertNull(store.takeIfId(replacement.requestId, "different-owner"))
        assertEquals(replacement, store.takeIfId(replacement.requestId, "owner"))
        assertNull(store.takeIfId(replacement.requestId, "owner"))
    }

    @Test
    fun `grant refreshes permission state before the capture can resume`() {
        val store = AudioPresetLaunchStore()
        store.registerOwner("owner") {}
        val request = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "owner")
        store.set(request)
        var refreshed = false
        val decision = refreshedAudioPermissionDecision(request.requestId, store, true) { refreshed = true }
        assertEquals(AudioPermissionDecision.RESUME_CAPTURE, decision)
        assertEquals(true, refreshed)
    }

    @Test
    fun `closing a window or stopping its owner invalidates only their pending recovery`() {
        val store = AudioPresetLaunchStore()
        store.registerOwner("owner") {}
        val request = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.INPUT_DICTATION, "owner", "input")
        store.set(request)
        store.clearInputWindow("owner", "other")
        assertEquals(request, store.peek())
        store.clearInputWindow("owner", "input")
        assertNull(store.takeIfId(request.requestId, "owner"))
        store.set(request)
        store.clearOwner("owner")
        assertNull(store.takeIfId(request.requestId, "owner"))
        assertEquals(AudioPermissionDecision.IGNORE, audioPermissionDecision(request.requestId, store.peek(), true))
    }

    @Test
    fun `late callback cannot reactivate a stopped or replaced owner`() {
        val store = AudioPresetLaunchStore()
        val old = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "old")
        val replacement = AudioPresetLaunchRequest("preset", AudioPresetLaunchKind.CAPTURE, "new")
        val calls = mutableListOf<String>()
        store.registerOwner("old") { calls.add("old:$it") }
        assertEquals(true, store.set(old))
        // Permission result has observed the request, but stop occurs before delivery.
        assertEquals(AudioPermissionDecision.RESUME_CAPTURE, audioPermissionDecision(old.requestId, store.peek(), true))
        store.releaseOwner("old")
        store.registerOwner("new") { calls.add("new:$it") }
        assertEquals(true, store.set(replacement))
        store.resumeIfOwned(old.requestId)
        assertEquals(emptyList<String>(), calls)
        assertEquals(false, store.set(old))
        store.releaseOwner("old")
        store.resumeIfOwned(replacement.requestId)
        assertEquals(listOf("new:${replacement.requestId}"), calls)
    }

    @Test
    fun `external launcher cannot access the private recovery activity`() {
        val manifest = DocumentBuilderFactory.newInstance().newDocumentBuilder()
            .parse(repoFile("mobile/androidApp/src/main/AndroidManifest.xml"))
        val activities = manifest.getElementsByTagName("activity")
        val all = (0 until activities.length).map { activities.item(it) as Element }
        val launcher = all.single { it.getAttribute("android:name") == ".MainActivity" }
        val recovery = all.single { it.getAttribute("android:name") == ".AudioPresetPermissionActivity" }
        assertEquals("true", launcher.getAttribute("android:exported"))
        assertEquals("false", recovery.getAttribute("android:exported"))
        val mainSource = repoFile("mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainActivity.kt").readText()
        assertFalse(mainSource.contains("AUTO_START"))
        assertFalse(mainSource.contains("RESUME_PENDING_AUDIO_PRESET"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}
