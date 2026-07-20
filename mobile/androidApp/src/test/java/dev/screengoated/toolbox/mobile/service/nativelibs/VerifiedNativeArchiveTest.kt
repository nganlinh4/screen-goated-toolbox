package dev.screengoated.toolbox.mobile.service.nativelibs

import java.io.ByteArrayInputStream
import java.io.File
import java.security.MessageDigest
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class VerifiedNativeArchiveTest {
    @get:Rule
    val temporary = TemporaryFolder()

    @Test
    fun `checked in ORT extracts only exact verified members`() {
        val manifest = NativeRuntimeContract.parse(
            File(
                repositoryRoot(),
                "parity-fixtures/phone-control/native-runtime-contract.json",
            ).readText(),
        )
        val contract = manifest.archive("ort")
        val archive = File(repositoryRoot(), "mobile/androidApp/libs/${contract.fileName}")
        val libDir = temporary.newFolder("native-libs")

        VerifiedNativeArchive.install(archive, libDir, contract)

        assertTrue(VerifiedNativeArchive.isInstalled(libDir, contract))
        assertEquals(contract.entries.map { it.fileName }.toSet(), libDir.list()!!.toSet())
    }

    @Test
    fun `identity failure leaves an existing install untouched`() {
        val bytes = "expected archive".toByteArray()
        val contract = archiveContract(bytes)
        val archive = temporary.newFile("runtime.zip").apply {
            writeBytes(bytes + byteArrayOf(1))
        }
        val libDir = temporary.newFolder("existing")
        val existing = File(libDir, "libtest.so").apply { writeText("old") }

        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.install(archive, libDir, contract)
        }
        assertEquals("old", existing.readText())
        assertEquals(setOf("libtest.so"), libDir.list()!!.toSet())
    }

    @Test
    fun `member verification failure does not replace an existing install`() {
        val memberBytes = "new native bytes".toByteArray()
        val archive = temporary.newFile("verified-runtime.zip")
        ZipOutputStream(archive.outputStream()).use { zip ->
            zip.putNextEntry(ZipEntry("libtest.so"))
            zip.write(memberBytes)
            zip.closeEntry()
        }
        val contract = NativeRuntimeArchive(
            engine = "test",
            fileName = archive.name,
            byteCount = archive.length(),
            sha256 = VerifiedNativeArchive.sha256(archive),
            fullDelivery = "bundled_asset",
            entries = listOf(
                NativeRuntimeEntry(
                    fileName = "libtest.so",
                    byteCount = memberBytes.size.toLong(),
                    sha256 = "0".repeat(64),
                ),
            ),
        )
        val libDir = temporary.newFolder("rollback")
        val existing = File(libDir, "libtest.so").apply { writeText("old native bytes") }

        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.install(archive, libDir, contract)
        }
        assertEquals("old native bytes", existing.readText())
        assertEquals(setOf("libtest.so"), libDir.list()!!.toSet())
    }

    @Test
    fun `entry contract rejects duplicates missing unexpected and traversal`() {
        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.validateArchiveEntryNames(
                listOf("liba.so", "liba.so"),
                listOf("liba.so", "libb.so"),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.validateArchiveEntryNames(
                listOf("liba.so"),
                listOf("liba.so", "libb.so"),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.validateArchiveEntryNames(
                listOf("liba.so", "libc.so"),
                listOf("liba.so", "libb.so"),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.validateArchiveEntryNames(
                listOf("../liba.so"),
                listOf("liba.so"),
            )
        }
    }

    @Test
    fun `archive materialization is exact and atomic`() {
        val bytes = ByteArray(16_384) { index -> (index % 251).toByte() }
        val contract = archiveContract(bytes)
        val destination = temporary.newFile("materialized.zip").apply { writeText("old") }

        VerifiedNativeArchive.materialize(ByteArrayInputStream(bytes), destination, contract)
        assertArrayEquals(bytes, destination.readBytes())
        assertFalse(requireNotNull(destination.parentFile).list()!!.any { it.endsWith(".part") })

        val old = destination.readBytes()
        assertThrows(IllegalArgumentException::class.java) {
            VerifiedNativeArchive.materialize(
                ByteArrayInputStream(bytes + byteArrayOf(1)),
                destination,
                contract,
            )
        }
        assertArrayEquals(old, destination.readBytes())
    }

    private fun archiveContract(bytes: ByteArray): NativeRuntimeArchive =
        NativeRuntimeArchive(
            engine = "test",
            fileName = "test-runtime.zip",
            byteCount = bytes.size.toLong(),
            sha256 = sha256(bytes),
            fullDelivery = "bundled_asset",
            entries = listOf(
                NativeRuntimeEntry(
                    fileName = "libtest.so",
                    byteCount = 1L,
                    sha256 = "0".repeat(64),
                ),
            ),
        )

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes)
            .joinToString("") { byte -> "%02x".format(byte) }
}
