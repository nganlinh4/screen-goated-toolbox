package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.io.File
import java.util.Base64
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class FileListingToolHandlerTest {
    @get:Rule
    val temporary = TemporaryFolder()

    @Test
    fun `standard folders resolve from primary shared storage without phrase guessing`() {
        val root = temporary.newFolder("shared")

        val downloads = resolveAndroidFileListPath("DoWnLoAdS", root)
        val unknown = resolveAndroidFileListPath("recent videos", root)

        assertEquals(
            File(root, "Download").canonicalFile,
            (downloads as AndroidFileListPathResolution.Resolved).file,
        )
        assertEquals("downloads", downloads.standardFolder)
        assertEquals(AndroidFileListPathResolution.Invalid, unknown)
    }

    @Test
    fun `privileged listing protocol preserves arbitrary utf8 names`() {
        val directory = temporary.newFolder("listing")
        val entry = File(directory, "Résumé\n2026.txt")
        val encoded = Base64.getEncoder().encodeToString(
            entry.absolutePath.toByteArray(Charsets.UTF_8),
        )
        val record = "f\t17\t123\t$encoded"
        assertEquals(4, record.split('\t').size)
        assertEquals(
            entry.absolutePath,
            Base64.getDecoder().decode(record.split('\t')[3]).toString(Charsets.UTF_8),
        )
        assertEquals(
            directory.canonicalFile,
            requireNotNull(entry.absoluteFile.parentFile).canonicalFile,
        )

        val parsed = parsePrivilegedFileListing(
            "$record\n",
            directory,
        ) as PrivilegedFileListingParseResult.Success

        assertEquals(1, parsed.entries.size)
        assertEquals(entry.absolutePath, parsed.entries.single().path)
        assertEquals(entry.name, parsed.entries.single().name)
        assertEquals("file", parsed.entries.single().kind)
        assertEquals(123_000L, parsed.entries.single().modifiedMs)
    }

    @Test
    fun `privileged listing rejects entries outside the exact directory`() {
        val directory = temporary.newFolder("listing")
        val outside = File(directory, "../outside.txt").absoluteFile
        val encoded = Base64.getEncoder().encodeToString(
            outside.path.toByteArray(Charsets.UTF_8),
        )

        val parsed = parsePrivilegedFileListing(
            "f\t1\t1\t$encoded\n",
            directory,
        )

        assertTrue(parsed is PrivilegedFileListingParseResult.Failure)
        assertEquals(
            "provider_contract_failure",
            (parsed as PrivilegedFileListingParseResult.Failure).code,
        )
    }

    @Test
    fun `registry allows complete listing through every implemented selected authority`() {
        assertEquals(
            listOf("android_app_api", "sgt_adb_bridge", "shizuku_shell", "root_bridge"),
            PhoneControlToolRegistry.byName.getValue("list_files").providerIds,
        )
    }

    @Test
    fun `resource launch classification separates apps paths and unsupported schemes`() {
        val root = temporary.newFolder("resource-root")
        val absolute = File(root, "clip with spaces.mp4")

        assertTrue(classifyResourceLaunchInput("Gallery", root) is ResourceLaunchInput.App)
        assertEquals(
            absolute.canonicalFile,
            (
                classifyResourceLaunchInput(
                    absolute.absolutePath,
                    root,
                ) as ResourceLaunchInput.Path
                ).file,
        )
        assertTrue(
            classifyResourceLaunchInput(
                "https://example.com",
                root,
            ) is ResourceLaunchInput.Invalid,
        )
    }

    @Test
    fun `launch app registry includes every implemented resource authority`() {
        assertEquals(
            listOf("android_app_api", "sgt_adb_bridge", "shizuku_shell", "root_bridge"),
            PhoneControlToolRegistry.byName.getValue("launch_app").providerIds,
        )
    }

    @Test
    fun `invalid list arguments identify the exact structural field`() {
        val execution = invalidFileListRequest(
            job = PhoneControlToolJobContext(1, "job", 1),
            args = buildJsonObject {
                put("path", "downloads")
                put("limit", 0)
            },
            fileKinds = setOf("any", "file", "directory"),
            sortFields = setOf("modified", "created", "name", "size"),
            orders = setOf("descending", "ascending"),
            maxLimit = 200,
        )

        assertEquals(
            "limit",
            execution.response.getValue("argument_field").jsonPrimitive.content,
        )
        assertEquals(
            "out_of_range",
            execution.response.getValue("contract_reason").jsonPrimitive.content,
        )
    }
}
