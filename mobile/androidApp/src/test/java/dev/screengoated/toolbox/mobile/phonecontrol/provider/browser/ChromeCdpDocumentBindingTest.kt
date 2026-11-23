package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChromeCdpDocumentBindingTest {
    private val root: File by lazy {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        directory
    }

    @Test
    fun boundCallKeepsWindowsGlobalScopeEvalSemantics() {
        val windows = File(root, "src/overlay/computer_control/browser/document_binding.rs").readText()
        assertTrue(windows.contains("(0, eval)(code)"))
        assertTrue(CDP_DOCUMENT_BOUND_CALL.contains("(0, eval)(code)"))
        assertTrue(CDP_DOCUMENT_BOUND_CALL.startsWith("async function(code)"))
    }

    @Test
    fun evaluationIsDispatchedOnTheDocumentObjectCheckedAfterResolution() {
        val binding = source("ChromeCdpDocumentEvaluation.kt")
            .substringAfter("internal suspend fun ChromeCdpController.evaluateOnBoundDocument(")
            .substringBefore("internal fun ChromeCdpController.cdpEvalResult(")
        val resolved = binding.indexOf("resolveDocumentObject(current)")
        val checked = binding.indexOf("ensureCurrentDocument(current)")
        val dispatched = binding.indexOf("\"Runtime.callFunctionOn\"")
        assertTrue(resolved in 0 until checked)
        assertTrue(checked < dispatched)
        assertTrue(binding.contains("put(\"objectId\", objectId)"))
        assertTrue(binding.contains("\"Runtime.releaseObject\""))
        assertFalse(binding.contains("\"Runtime.evaluate\""))
    }

    @Test
    fun evalWaitAndUploadLeaveNoGapBetweenCheckAndDispatch() {
        val tools = source("ChromeCdpPageTools.kt")
        val eval = tools.substringAfter("internal suspend fun ChromeCdpController.eval(")
            .substringBefore("internal suspend fun ChromeCdpController.upload(")
        assertTrue(eval.contains("evaluateOnBoundDocument(current, code, true)"))
        assertFalse(eval.contains("evaluate(current, code"))

        val waitFor = tools.substringBefore("internal suspend fun ChromeCdpController.eval(")
            .substringAfterLast("document.querySelector(")
        assertTrue(waitFor.contains("evaluateOnBoundDocument(current, expression, false)"))

        val upload = tools.substringAfter("internal suspend fun ChromeCdpController.upload(")
            .substringBefore("internal suspend fun ChromeCdpController.network(")
        val queried = upload.indexOf("\"DOM.querySelector\"")
        val checked = upload.indexOf("ensureCurrentDocument(current)")
        val dispatched = upload.indexOf("\"DOM.setFileInputFiles\"")
        assertTrue(queried in 0 until checked)
        assertTrue(checked < dispatched)
    }

    private fun source(name: String): String = File(
        root,
        "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
            "provider/browser/$name",
    ).readText()
}
