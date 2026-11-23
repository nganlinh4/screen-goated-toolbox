package dev.screengoated.toolbox.mobile.storage

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Test
import org.w3c.dom.Element

/**
 * Secure settings are bound to a device keystore key, so no backup or
 * device-transfer domain may carry app data off the device.
 */
class BackupRulesTest {
    @Test
    fun everyDomainExcluded() {
        val extraction = parse("data_extraction_rules.xml")
        for (section in listOf("cloud-backup", "device-transfer")) {
            val element = extraction.getElementsByTagName(section).item(0) as Element
            assertEquals(section, ALL_DOMAINS, excludedRootDomains(element))
            assertEquals(section, 0, element.getElementsByTagName("include").length)
        }
        val fullBackup = parse("backup_rules.xml")
        assertEquals(ALL_DOMAINS, excludedRootDomains(fullBackup))
        assertEquals(0, fullBackup.getElementsByTagName("include").length)
    }

    private fun excludedRootDomains(parent: Element): Set<String> {
        val excludes = parent.getElementsByTagName("exclude")
        return (0 until excludes.length)
            .map { excludes.item(it) as Element }
            .filter { it.getAttribute("path") == "." }
            .map { it.getAttribute("domain") }
            .toSet()
    }

    private fun parse(name: String): Element =
        DocumentBuilderFactory.newInstance().newDocumentBuilder()
            .parse(repoFile("mobile/androidApp/src/main/res/xml/$name"))
            .documentElement

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }

    private companion object {
        val ALL_DOMAINS = setOf("root", "file", "database", "sharedpref", "external")
    }
}
