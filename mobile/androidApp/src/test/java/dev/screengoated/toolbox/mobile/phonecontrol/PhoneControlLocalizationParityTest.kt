package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import javax.xml.parsers.DocumentBuilderFactory
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import org.w3c.dom.Element

class PhoneControlLocalizationParityTest {
    private val fixture = Json.parseToJsonElement(
        Files.readAllBytes(fixturePath()).decodeToString(),
    ).jsonObject

    @Test
    fun `app card title follows the in-app language contract`() {
        fixture.getValue("locales").jsonArray.forEach { localeCase ->
            val expected = localeCase.jsonObject
            val locale = MobileLocaleText.forLanguage(
                expected.getValue("code").jsonPrimitive.content,
            )

            assertEquals(
                expected.getValue("appCardTitle").jsonPrimitive.content,
                locale.appPhoneControlTitle,
            )
        }

        val fallback = MobileLocaleText.forLanguage("unsupported")
        assertEquals(
            fixture.getValue("fallbackLocale").jsonPrimitive.content,
            fallback.localeCode,
        )
        assertEquals("Phone Control", fallback.appPhoneControlTitle)
    }

    @Test
    fun `localized Phone Control resources preserve keys and format arguments`() {
        val locales = fixture.getValue("locales").jsonArray.map { localeCase ->
            val contract = localeCase.jsonObject
            val qualifier = contract.getValue("androidQualifier").jsonPrimitive.content
            contract to phoneControlStrings(resourcePath(qualifier))
        }
        val defaultStrings = locales.first().second

        locales.forEach { (contract, strings) ->
            assertEquals(defaultStrings.keys, strings.keys)
            defaultStrings.forEach { (name, defaultValue) ->
                assertEquals(
                    "Format arguments differ for $name in " +
                        contract.getValue("code").jsonPrimitive.content,
                    formatArguments(defaultValue),
                    formatArguments(strings.getValue(name)),
                )
            }
            assertEquals(
                contract.getValue("appCardTitle").jsonPrimitive.content,
                strings.getValue("phone_control_title"),
            )
        }
    }

    private fun phoneControlStrings(path: Path): Map<String, String> {
        val document = DocumentBuilderFactory.newInstance()
            .newDocumentBuilder()
            .parse(path.toFile())
        val strings = document.getElementsByTagName("string")
        return buildMap {
            repeat(strings.length) { index ->
                val element = strings.item(index) as Element
                val name = element.getAttribute("name")
                if (name.startsWith("phone_control_")) {
                    put(name, element.textContent)
                }
            }
        }
    }

    private fun formatArguments(value: String): List<String> =
        FORMAT_ARGUMENT.findAll(value).map { it.value }.toList()

    private fun fixturePath(): Path = findFile(
        Paths.get("..", "parity-fixtures", "phone-control", "localization-contract.json"),
        Paths.get("..", "..", "parity-fixtures", "phone-control", "localization-contract.json"),
        Paths.get("parity-fixtures", "phone-control", "localization-contract.json"),
    )

    private fun resourcePath(qualifier: String): Path = findFile(
        Paths.get("src", "main", "res", qualifier, "strings.xml"),
        Paths.get("androidApp", "src", "main", "res", qualifier, "strings.xml"),
        Paths.get("mobile", "androidApp", "src", "main", "res", qualifier, "strings.xml"),
    )

    private fun findFile(vararg candidates: Path): Path =
        candidates.firstOrNull(Files::exists)
            ?: error("Missing file. Tried: ${candidates.toList()}")

    private companion object {
        val FORMAT_ARGUMENT = Regex("%\\d+\\$[a-zA-Z]")
    }
}
