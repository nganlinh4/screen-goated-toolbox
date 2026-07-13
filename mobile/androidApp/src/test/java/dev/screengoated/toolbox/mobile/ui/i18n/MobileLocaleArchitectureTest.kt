package dev.screengoated.toolbox.mobile.ui.i18n

import java.lang.reflect.Modifier
import java.util.ArrayDeque
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Android/JVM implementation guard. This is intentionally local rather than part of the
 * cross-platform mobile-shell parity fixture.
 */
class MobileLocaleArchitectureTest {
    @Test
    fun localeCatalogStaysSectionedAndJvmSafe() {
        val rootFields = instanceFields(MobileLocaleText::class.java)
        assertEquals(EXPECTED_ROOT_PROPERTIES, rootFields.keys)
        assertConstructorParameterLimit(MobileLocaleText::class.java, MAX_ROOT_PARAMETERS)

        val localePackage = MobileLocaleText::class.java.packageName
        val pending = ArrayDeque<Class<*>>()
        OWNED_SECTION_PROPERTIES.forEach { sectionName ->
            val sectionType = rootFields.getValue(sectionName).type
            assertEquals("$sectionName must be a locale-owned section", localePackage, sectionType.packageName)
            pending.addLast(sectionType)
        }

        val seen = mutableSetOf<Class<*>>()
        while (pending.isNotEmpty()) {
            val sectionType = pending.removeFirst()
            if (!seen.add(sectionType)) continue

            assertConstructorParameterLimit(sectionType, MAX_SECTION_PARAMETERS)
            instanceFields(sectionType).values
                .map { it.type }
                .filter { it.packageName == localePackage && it != MobileLocaleText::class.java }
                .forEach(pending::addLast)
        }
    }

    private fun instanceFields(type: Class<*>) = type.declaredFields
        .filterNot { Modifier.isStatic(it.modifiers) }
        .associateBy { it.name }

    private fun assertConstructorParameterLimit(type: Class<*>, maximum: Int) {
        val widestConstructor = type.declaredConstructors
            .filterNot { it.isSynthetic }
            .maxByOrNull { it.parameterCount }
            ?: error("${type.name} has no non-synthetic constructor")
        assertTrue(
            "${type.name} constructor has ${widestConstructor.parameterCount} parameters; maximum is $maximum",
            widestConstructor.parameterCount <= maximum,
        )
    }

    private companion object {
        const val MAX_ROOT_PARAMETERS = 16
        const val MAX_SECTION_PARAMETERS = 32

        val EXPECTED_ROOT_PROPERTIES = setOf(
            "localeCode",
            "shell",
            "history",
            "help",
            "translationGummy",
            "providers",
            "presetRuntime",
            "updates",
            "customModels",
            "appearance",
            "ttsSettings",
            "ttsVoice",
            "download",
            "downloadOptions",
            "downloader",
        )
        val OWNED_SECTION_PROPERTIES = EXPECTED_ROOT_PROPERTIES - "localeCode"
    }
}
