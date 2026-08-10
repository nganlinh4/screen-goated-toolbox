package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationModelViewerCapabilityTest {
    private val token = "0123456789abcdef0123456789abcdef"
    private val root =
        "$CREATION_MODEL_VIEWER_ORIGIN/v$CREATION_MODEL_VIEWER_DOCUMENT_VERSION/$token"

    @Test
    fun `viewer capability follows the installed WebView package only`() {
        assertTrue(supportsCreationWebViewer("com.android.webview"))
        assertFalse(supportsCreationWebViewer(null))
        assertFalse(supportsCreationWebViewer("  "))
    }

    @Test
    fun `token namespace exposes only the viewer document assets and one model`() {
        assertEquals(
            CreationModelViewerResource.DOCUMENT,
            routeCreationModelViewerRequest("$root/index.html", "GET", token),
        )
        assertEquals(
            CreationModelViewerResource.SCRIPT,
            routeCreationModelViewerRequest("$root/assets/viewer.js", "GET", token),
        )
        assertEquals(
            CreationModelViewerResource.STYLE,
            routeCreationModelViewerRequest("$root/assets/viewer.css", "GET", token),
        )
        assertEquals(
            CreationModelViewerResource.FONT,
            routeCreationModelViewerRequest(
                "$CREATION_MODEL_VIEWER_ORIGIN/GoogleSansFlex.woff",
                "GET",
                token,
            ),
        )
        assertEquals(
            CreationModelViewerResource.MODEL,
            routeCreationModelViewerRequest("$root/model.glb", "GET", token),
        )
    }

    @Test
    fun `network methods aliases traversal and cross-session paths fail closed`() {
        listOf(
            "https://example.com/$token/model.glb",
            "http://appassets.androidplatform.net/creation-model-viewer/v1/$token/model.glb",
            "$root/model.glb?redirect=https://example.com",
            "$root/model.glb#fragment",
            "$root/%2e%2e/model.glb",
            "$root/extra/model.glb",
            "$CREATION_MODEL_VIEWER_ORIGIN/v1/ffffffffffffffffffffffffffffffff/model.glb",
            "https://user@appassets.androidplatform.net/creation-model-viewer/v1/$token/model.glb",
            "https://appassets.androidplatform.net:443/creation-model-viewer/v1/$token/model.glb",
        ).forEach { url ->
            assertNull(url, routeCreationModelViewerRequest(url, "GET", token))
        }
        assertNull(routeCreationModelViewerRequest("$root/model.glb", "POST", token))
        assertNull(routeCreationModelViewerRequest("$root/model.glb", "get", token))
        assertNull(routeCreationModelViewerRequest("$root/model.glb", "GET", "short"))
    }

    @Test
    fun `startup script exposes the token URL and labels but never the file path`() {
        val file = File("C:/private/user/result.glb")
        val session = CreationModelViewerSession(
            token = token,
            modelFile = file,
            segmented = true,
            darkTheme = false,
            labels = labels(previewUnavailable = "Can't show </script> this model"),
        )
        val script = creationModelViewerStartScript(session)

        assertTrue(script.startsWith("window.sgtModelViewer?.start("))
        assertTrue("$root/model.glb" in script)
        assertTrue("\"segmented\":true" in script)
        assertTrue("\"theme\":\"light\"" in script)
        assertTrue("Can't show <\\/script> this model" in script)
        assertFalse(file.path in script)
    }

    @Test
    fun `shared viewer artifact and Android host match parity security contract`() {
        val repo = repoRoot()
        val fixture = Json.parseToJsonElement(
            File(repo, "parity-fixtures/image-to-3d/state-contract.json").readText(),
        ).jsonObject
        val presentation = fixture.getValue("presentation").jsonObject
        assertEquals("shared_webview", presentation.getValue("resultRenderer").jsonPrimitive.content)
        assertTrue(presentation.getValue("sharedViewerDocument").jsonPrimitive.boolean)
        assertTrue(presentation.getValue("appControlledResultOrigin").jsonPrimitive.boolean)
        assertFalse(presentation.getValue("externalViewerResourcesAllowed").jsonPrimitive.boolean)
        assertFalse(
            presentation.getValue("platformNativeRendererFallbackAllowed").jsonPrimitive.boolean,
        )
        assertEquals(
            listOf("orbit", "zoom", "pan", "grid", "wireframe", "auto_rotate", "toon", "outline"),
            presentation.getValue("viewerControls").jsonArray.map { it.jsonPrimitive.content },
        )

        val entry = File(repo, "3d-generator-ui/src/viewer-entry.ts").readText()
        val viewer = File(repo, "3d-generator-ui/src/viewer.ts").readText()
        val document = File(
            repo,
            "3d-generator-ui/viewer-dist/creation_model_viewer/index.html",
        ).readText()
        val host = File(
            repo,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/" +
                "CreationModelViewerOrigin.kt",
        ).readText()
        assertTrue("from \"./viewer\"" in entry)
        assertTrue("VIEWER_DOCUMENT_VERSION = 1" in entry)
        assertTrue("controlState.wireframe" in entry)
        assertTrue("controlState.outline" in entry)
        assertTrue("viewer.setWireframe(controlState.wireframe)" in entry)
        assertTrue("viewer.setOutline(controlState.outline)" in entry)
        assertTrue("viewer.fitView()" in entry)
        assertTrue("dispose()" in viewer)
        assertTrue("data-viewer-version=\"1\"" in document)
        assertTrue("default-src 'none'" in document)
        assertTrue("connect-src 'self'" in document)
        assertTrue("font-src 'self'" in document)
        assertTrue("CreationArtifactValidator.validateGlb(session.modelFile)" in host)
        assertTrue("val length = stream.channel.size()" in host)
        assertFalse("session.modelFile.length()" in host)
        assertFalse("addJavascriptInterface" in host)
    }

    private fun labels(previewUnavailable: String = "Unavailable") = CreationModelViewerLabels(
        originalMaterials = "Original",
        toonOutline = "Toon",
        partColors = "Parts",
        toggleOutline = "Outline",
        toggleRotation = "Rotate",
        toggleGrid = "Grid",
        toggleWireframe = "Wireframe",
        resetView = "Fit",
        preview = "Preview",
        previewUnavailable = previewUnavailable,
    )

    private fun repoRoot(): File {
        val workingDirectory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        return generateSequence(workingDirectory) { it.parentFile }
            .take(8)
            .map(File::getCanonicalFile)
            .firstOrNull { File(it, "parity-fixtures").isDirectory }
            ?: error("Could not locate repository from $workingDirectory")
    }
}
