package dev.screengoated.toolbox.mobile.phonecontrol.ui

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlLocaleResourcesTest {
    @Test
    fun `every explicit locale has the complete phone control resource contract`() {
        val resources = resourceDirectory()
        val base = phoneControlStrings(File(resources, "values/strings.xml"))
        val localizedDirectories = resources.listFiles()
            .orEmpty()
            .filter { directory ->
                directory.isDirectory &&
                    directory.name.startsWith("values-") &&
                    File(directory, "strings.xml").isFile
            }
            .sortedBy(File::getName)

        assertTrue(
            "Expected at least one explicit Phone Control locale",
            localizedDirectories.isNotEmpty(),
        )
        localizedDirectories.forEach { directory ->
            val localized = phoneControlStrings(File(directory, "strings.xml"))
            assertEquals("${directory.name} keys", base.keys, localized.keys)
            base.forEach { (key, value) ->
                assertEquals(
                    "${directory.name}/$key format arguments",
                    formatArguments(value),
                    formatArguments(localized.getValue(key)),
                )
            }
        }
    }

    @Test
    fun `phone control toast copy stays glanceable in every explicit locale`() {
        val resources = resourceDirectory()
        resources.listFiles()
            .orEmpty()
            .filter { directory ->
                directory.isDirectory &&
                    (directory.name == "values" || directory.name.startsWith("values-")) &&
                    File(directory, "strings.xml").isFile
            }
            .forEach { directory ->
                val toastStrings = phoneControlStrings(File(directory, "strings.xml"))
                    .filterKeys { it.endsWith("_toast") }
                assertTrue("${directory.name} has toast copy", toastStrings.isNotEmpty())
                toastStrings.forEach { (key, value) ->
                    val renderedWorstCase = FORMAT_ARGUMENT.replace(
                        value,
                        FORMAT_ARGUMENT_SAMPLE,
                    )
                    assertTrue(
                        "${directory.name}/$key exceeds $MAXIMUM_TOAST_CHARACTERS characters",
                        renderedWorstCase.length <= MAXIMUM_TOAST_CHARACTERS,
                    )
                }
            }
    }

    private fun phoneControlStrings(file: File): Map<String, String> {
        val document = DocumentBuilderFactory.newInstance()
            .newDocumentBuilder()
            .parse(file)
        val nodes = document.getElementsByTagName("string")
        return buildMap {
            repeat(nodes.length) { index ->
                val element = nodes.item(index)
                val name = element.attributes.getNamedItem("name")?.nodeValue.orEmpty()
                if (name.startsWith("phone_control_")) {
                    put(name, element.textContent)
                }
            }
        }.toSortedMap()
    }

    private fun formatArguments(value: String): List<String> =
        FORMAT_ARGUMENT.findAll(value).map { it.value }.toList()

    private fun resourceDirectory(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, RESOURCE_PATH) }
            .firstOrNull(File::isDirectory)
            ?: error("Could not locate $RESOURCE_PATH from $workingDirectory")
    }

    private companion object {
        val FORMAT_ARGUMENT = Regex("%(?:\\d+\\$)?[a-zA-Z]")
        const val FORMAT_ARGUMENT_SAMPLE = "XXXXXXXXXXXXXXXX"
        const val RESOURCE_PATH = "mobile/androidApp/src/main/res"
        const val MAXIMUM_TOAST_CHARACTERS = 32
    }
}
