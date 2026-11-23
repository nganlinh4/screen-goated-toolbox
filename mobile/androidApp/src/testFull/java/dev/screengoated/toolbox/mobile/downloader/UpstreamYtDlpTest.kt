package dev.screengoated.toolbox.mobile.downloader

import java.io.File
import java.security.MessageDigest
import java.util.Base64
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class UpstreamYtDlpTest {
    @get:Rule val temporaryFolder = TemporaryFolder()

    @Test
    fun upstreamProofAcceptsOnlyAuthenticChecksumBytes() {
        val proof = JSONObject(repoFile("parity-fixtures/video-downloader/upstream-release-proof.json").readText())
        val key = repoFile("mobile/androidApp/src/full/assets/downloader-runtime/yt-dlp-release-key.asc")
        val checksums = Base64.getDecoder().decode(proof.getString("checksums"))
        val signature = Base64.getDecoder().decode(proof.getString("signature"))
        assertEquals(proof.getString("ytDlpSha256"), key.inputStream().use {
            verifyYtDlpChecksum(it, checksums, signature)
        })
        val modified = checksums.clone().apply { this[0] = '0'.code.toByte() }
        assertFalse(key.inputStream().use { runCatching { verifyYtDlpChecksum(it, modified, signature) }.isSuccess })
        val brokenSignature = signature.clone().apply {
            this[lastIndex] = (this[lastIndex].toInt() xor 1).toByte()
        }
        assertFalse(key.inputStream().use {
            runCatching { verifyYtDlpChecksum(it, checksums, brokenSignature) }.isSuccess
        })
    }

    @Test
    fun candidateFailuresKeepBootstrapAndRemoveStaging() = runBlocking {
        val fixture = JSONObject(repoFile("parity-fixtures/video-downloader/recovery.json").readText())
            .getJSONObject("androidFullUpdate")
        assertEquals(fixture.getLong("automaticCheckHours") * 3_600_000, UPDATE_CHECK_MS)
        assertTrue(fixture.getBoolean("useExistingBootstrap"))
        assertFalse(fixture.getBoolean("dependsOnSgtCatalog"))
        val root = temporaryFolder.newFolder()
        val bootstrap = bootstrap()
        val store = YtDlpInstallStore(root, bootstrap, "native")
        val (asset, contents) = candidate("2026.02.01")
        assertTrue(runCatching {
            store.install(asset, { it.writeText("bad bytes") }) { _, _ -> error("must not execute") }
        }.isFailure)
        assertTrue(runCatching {
            store.install(asset, { it.writeText(contents) }) { _, _ -> error("incompatible interpreter") }
        }.isFailure)
        assertEquals(bootstrap, store.current())
        assertTrue(root.listFiles().orEmpty().isEmpty())
    }

    @Test
    fun versionSwitchPreservesRunningJobAndCanReopenOffline() = runBlocking {
        val root = temporaryFolder.newFolder()
        val bootstrap = bootstrap()
        val store = YtDlpInstallStore(root, bootstrap, "native")
        suspend fun put(tag: String) {
            val (asset, contents) = candidate(tag)
            assertTrue(store.install(asset, { it.writeText(contents) }) { file, expected ->
                assertEquals(tag, expected.tag)
                assertEquals(contents, file.readText())
            })
        }
        try {
            put("2026.02.01")
            val oldJob = store.acquire()
            oldJob.use {
                put("2026.02.01.2")
                put("2026.02.01.10")
                assertEquals("2026.02.01", oldJob.tool.version.tag)
                assertTrue(oldJob.tool.executable.isFile)
                assertEquals("2026.02.01.10", store.current().version.tag)
                oldJob.tool.executable.setWritable(true)
            }
            assertFalse(oldJob.tool.executable.exists())
            val restarted = YtDlpInstallStore(root, bootstrap, "native")
            assertEquals("2026.02.01.10", restarted.current().version.tag)
            restarted.current().executable.apply { setWritable(true); writeText("corrupted") }
            assertEquals("2026.02.01.2", restarted.current().version.tag)
            assertEquals(bootstrap, YtDlpInstallStore(root, bootstrap, "different native runtime").current())
        } finally {
            root.walkBottomUp().forEach { it.setWritable(true) }
        }
    }

    private fun bootstrap() = InstalledYtDlp(
        requireNotNull(YtDlpVersion.parse("2026.01.01")),
        temporaryFolder.newFile(),
    )

    private fun candidate(tag: String): Pair<VerifiedYtDlpAsset, String> {
        val bytes = "script $tag"
        val digest = MessageDigest.getInstance("SHA-256").apply { update(bytes.encodeToByteArray()) }.hex()
        return VerifiedYtDlpAsset(requireNotNull(YtDlpVersion.parse(tag)), bytes.length.toLong(), digest) to bytes
    }
}

private fun repoFile(path: String): File =
    generateSequence(File(requireNotNull(System.getProperty("user.dir"))).canonicalFile) { it.parentFile }
        .map { it.resolve(path) }
        .first { it.isFile }
