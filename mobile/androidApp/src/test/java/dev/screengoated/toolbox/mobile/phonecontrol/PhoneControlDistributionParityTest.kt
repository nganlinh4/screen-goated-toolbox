package dev.screengoated.toolbox.mobile.phonecontrol

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

class PhoneControlDistributionParityTest {
    @Test
    fun `shared manifest gives both distributions private Phone Control components`() {
        val manifest = xml("mobile/androidApp/src/main/AndroidManifest.xml")
        val activities = manifest.getElementsByTagName("activity").elements()
        val services = manifest.getElementsByTagName("service").elements()
        val activity = activities.single {
            it.androidAttribute("name").endsWith(".phonecontrol.ui.PhoneControlActivity")
        }
        val service = services.single {
            it.androidAttribute("name").endsWith(".phonecontrol.PhoneControlService")
        }

        assertEquals("false", activity.androidAttribute("exported"))
        assertEquals("false", service.androidAttribute("exported"))
        assertEquals("true", service.androidAttribute("enabled"))
        assertEquals(
            setOf("specialUse", "microphone", "mediaPlayback", "mediaProjection"),
            service.androidAttribute("foregroundServiceType").split('|').toSet(),
        )
        val property = service.getElementsByTagName("property").elements().single()
        assertEquals(
            "android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE",
            property.androidAttribute("name"),
        )
        assertEquals(
            "user_started_voice_agent_controlling_visible_phone_interfaces",
            property.androidAttribute("value"),
        )
    }

    @Test
    fun `shared Accessibility declaration exposes required structural capabilities`() {
        val service = xml(
            "mobile/androidApp/src/main/res/xml/accessibility_service_config.xml",
        ).documentElement

        assertEquals("typeAllMask", service.androidAttribute("accessibilityEventTypes"))
        assertEquals("true", service.androidAttribute("canPerformGestures"))
        assertEquals("true", service.androidAttribute("canRetrieveWindowContent"))
        assertEquals("true", service.androidAttribute("canTakeScreenshot"))
        assertTrue(
            service.androidAttribute("accessibilityFlags")
                .split('|')
                .containsAll(
                    listOf(
                        "flagReportViewIds",
                        "flagIncludeNotImportantViews",
                        "flagRetrieveInteractiveWindows",
                    ),
                ),
        )
    }

    @Test
    fun `full and play share behavior and vary only detector asset delivery`() {
        val appCatalog = file(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/AppsCarouselSection.kt",
        ).readText()
        val sharedSources = file(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol",
        ).walkTopDown().filter(File::isFile).toList()
        val flavorSources = listOf(
            file("mobile/androidApp/src/full"),
            file("mobile/androidApp/src/play"),
        ).flatMap { root ->
            root.walkTopDown()
                .filter(File::isFile)
                .filter { source ->
                    source.invariantSeparatorsPath.contains("/phonecontrol/") ||
                        source.name in FLAVOR_OVERRIDE_FILES
                }
                .toList()
        }

        assertTrue(sharedSources.isNotEmpty())
        assertTrue(appCatalog.contains("app-card-phone-control"))
        assertTrue(appCatalog.contains("PhoneControlActivity.activationIntent"))
        assertTrue(appCatalog.contains("PhoneControlService.stop"))
        assertEquals(
            setOf(
                "full/java/dev/screengoated/toolbox/mobile/phonecontrol/provider/detector/" +
                    "UiDetectorBundledModelSource.kt",
                "play/java/dev/screengoated/toolbox/mobile/phonecontrol/provider/detector/" +
                    "UiDetectorBundledModelSource.kt",
            ),
            flavorSources.map { source ->
                source.relativeTo(file("mobile/androidApp/src")).invariantSeparatorsPath
            }.toSet(),
        )
        flavorSources.forEach { source ->
            val deliveryShim = source.readText()
            assertTrue(deliveryShim.contains("internal object UiDetectorBundledModelSource"))
            assertTrue(deliveryShim.contains("suspend fun copyTo("))
            assertTrue(deliveryShim.contains("UiDetectorBundledModelResult"))
            assertFalse(deliveryShim.contains("PhoneControlToolRegistry"))
            assertFalse(deliveryShim.contains("PhoneControlHandler"))
        }
    }

    private fun xml(path: String) = DocumentBuilderFactory.newInstance()
        .apply { isNamespaceAware = true }
        .newDocumentBuilder()
        .parse(file(path))

    private fun file(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private fun org.w3c.dom.NodeList.elements(): List<Element> {
        return (0 until length).map { index -> item(index) as Element }
    }

    private fun Element.androidAttribute(name: String): String {
        return getAttributeNS(ANDROID_NAMESPACE, name)
    }

    private companion object {
        private const val ANDROID_NAMESPACE = "http://schemas.android.com/apk/res/android"
        private val FLAVOR_OVERRIDE_FILES = setOf(
            "DistributionAccessibilityHooks.kt",
            "accessibility_service_config.xml",
        )
    }
}
