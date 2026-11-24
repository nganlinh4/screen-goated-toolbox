package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.sha256
import dev.screengoated.toolbox.mobile.phonecontrol.provider.testFileOwner
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import java.io.IOException
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** Filesystem protocol/fault tests. Native kernel atomicity has a separate acceptance gate. */
class FilePublicationTest {
    @get:Rule val temporary = TemporaryFolder()
    private val io = NioTestFileTransactionIo()
    private val root get() = temporary.root.canonicalFile
    private val destination get() = File(root, "destination.txt")
    private val output = "complete replacement".toByteArray()

    @Test fun `preparation freezes the baseline and creates no missing parents`() {
        val path = File(root, "missing/inner/output.txt")
        prepare(path).use { target ->
            assertNull(target.originalBytes())
            assertFalse(requireNotNull(path.parentFile).exists())
            assertFalse(target.lease.existedBefore)
        }
        assertTrue(root.listFiles()!!.isEmpty())
    }

    @Test fun `create publishes complete bytes without retaining a consumed stage`() {
        val success = commit() as AndroidProviderResult.Success
        assertArrayEquals(output, destination.readBytes())
        assertTrue(success.effectVerified)
        assertTrue(success.data.flag("atomic"))
        assertFalse(success.data.containsKey("retained_staging_path"))
        assertEquals(listOf(destination), root.listFiles()!!.toList())
    }

    @Test fun `verified exchange retains the exact original and reports its identity`() {
        destination.writeText("original")
        prepare().use { target ->
            val originalId = target.lease.objectIdentity
            val result = commit(target) as AndroidProviderResult.Success
            assertTrue(result.effectVerified)
            assertArrayEquals(output, destination.readBytes())
            assertEquals("original", retained(result.data, "retained_backup").readText())
            assertEquals(originalId!!.description(), result.data["retained_identity"])
            assertEquals("original".toByteArray().sha256(), result.data.text("recovery_sha256"))
            assertFalse(result.data.flag("retained_entry_ownership_unverified"))
        }
    }

    @Test fun `same bytes on a replaced object invalidate authorization before staging`() {
        destination.writeText("original")
        prepare().use { target ->
            io.replaceIdentity(destination.path)
            val failure = failure(commit(target), "ERR_FILE_TARGET_LEASE_CHANGED")
            assertEquals(EffectCertainty.PROVEN_NO_EFFECT, failure.effect)
            assertEquals("original", destination.readText())
            assertEquals(1, root.listFiles()!!.size)
        }
    }

    @Test fun `final create collision preserves a complete competitor and retained stage`() {
        io.beforePublish = { destination.writeText("competitor"); io.replaceIdentity(destination.path) }
        val failure = failure(commit(), "path_exists")
        assertFalse(failure.data.flag("target_effect_may_have_occurred"))
        assertEquals(EffectCertainty.MAY_HAVE_OCCURRED, failure.effect)
        assertEquals("competitor", destination.readText())
        assertArrayEquals(output, retained(failure.data, "retained_staging_path").readBytes())
    }

    @Test fun `expired request during staging prevents target publication`() {
        destination.writeText("original")
        var current = true
        io.beforeWrite = { current = false }
        val failure = failure(commit(authorized = { current }), "ERR_FILE_TARGET_REQUEST_CONTRACT_UNVERIFIED")
        assertEquals("original", destination.readText())
        assertFalse(failure.data.flag("target_effect_may_have_occurred"))
        assertTrue(retained(failure.data, "retained_staging_path").isFile)
    }

    @Test fun `expired request before commit creates no output`() {
        val failure = failure(commit(authorized = { false }), "ERR_FILE_TARGET_REQUEST_CONTRACT_UNVERIFIED")
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT, failure.effect)
        assertTrue(temporary.root.listFiles()!!.isEmpty())
    }

    @Test fun `cancellation before commit creates no stage`() {
        val owner = testFileOwner().apply { requestCancellation() }
        val failure = failure(commit(owner = owner), "ERR_CANCELLED")
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT, failure.effect)
        assertTrue(temporary.root.listFiles()!!.isEmpty())
    }

    @Test fun `cancellation during staging retains its partial file and original`() {
        destination.writeText("original")
        val owner = testFileOwner()
        io.beforeWrite = { owner.requestCancellation() }
        val failure = failure(commit(owner = owner), "ERR_CANCELLED")
        assertEquals("original", destination.readText())
        assertFalse(failure.data.flag("target_effect_may_have_occurred"))
        assertEquals(0L, retained(failure.data, "retained_staging_path").length())
        assertEquals(failure.data["retained_staging_path"], owner.terminalEffectEvidence()
            .single().let { (it as JsonObject)["retained_staging_path"] })
    }

    @Test fun `cancellation after publication admission audits and preserves recovery`() {
        destination.writeText("original")
        val owner = testFileOwner()
        io.beforePublish = { owner.requestCancellation() }
        val success = commit(owner = owner) as AndroidProviderResult.Success
        assertTrue(success.effectVerified)
        assertEquals("original", retained(success.data, "retained_backup").readText())
        assertArrayEquals(output, destination.readBytes())
        assertEquals(success.data["retained_backup"], (owner.terminalEffectEvidence().single() as JsonObject)["retained_backup"])
    }

    @Test fun `partial stage write cannot truncate an existing destination`() {
        destination.writeText("original")
        io.writeFailureAfter = 3
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_FAILED")
        assertEquals("original", destination.readText())
        assertArrayEquals(output.copyOf(3), retained(failure.data, "retained_staging_path").readBytes())
        assertFalse(failure.data.flag("target_effect_may_have_occurred"))
    }

    @Test fun `unsupported atomic publication never falls back to overwriting`() {
        destination.writeText("original")
        io.beforePublish = { throw AtomicPublicationUnavailable(95) }
        val failure = failure(commit(), "ERR_FILE_ATOMIC_PUBLICATION_UNAVAILABLE")
        assertEquals(CapabilityState.UNSUPPORTED, failure.providerState)
        assertFalse(failure.data.flag("target_effect_may_have_occurred"))
        assertEquals("original", destination.readText())
        assertArrayEquals(output, retained(failure.data, "retained_staging_path").readBytes())
    }

    @Test fun `late replacement is retained and never silently rolled back`() {
        destination.writeText("original")
        io.beforePublish = { destination.writeText("competitor"); io.replaceIdentity(destination.path) }
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_UNVERIFIED")
        assertTrue(failure.data.flag("target_effect_may_have_occurred"))
        assertEquals("competitor", retained(failure.data, "retained_backup").readText())
        assertTrue(failure.data.flag("retained_entry_ownership_unverified"))
        assertArrayEquals(output, destination.readBytes())
    }

    @Test fun `error returned after exchange still reports original recovery`() {
        destination.writeText("original")
        io.afterPublish = { throw IOException("Uncertain native result") }
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_UNVERIFIED")
        assertTrue(failure.data.flag("target_effect_may_have_occurred"))
        assertArrayEquals(output, destination.readBytes())
        assertEquals("original", retained(failure.data, "retained_backup").readText())
    }

    @Test fun `ambiguous error with unreadable target cannot prove absence`() {
        io.beforePublish = { io.readFailure = { true }; throw IOException("Uncertain native result") }
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_UNVERIFIED")
        assertTrue(failure.data.flag("target_effect_may_have_occurred"))
        assertEquals(EffectCertainty.MAY_HAVE_OCCURRED, failure.effect)
        assertFalse(failure.data.flag("original_unchanged"))
        assertTrue(failure.data.flag("retained_entry_ownership_unverified"))
        assertFalse(failure.data.flag("retained_location_observed"))
    }

    @Test fun `failed installed readback retains available original recovery`() {
        destination.writeText("original")
        io.afterPublish = { io.readFailure = { File(it) == destination } }
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_UNVERIFIED")
        assertTrue(failure.data.flag("target_effect_may_have_occurred"))
        assertEquals("original", retained(failure.data, "retained_backup").readText())
        assertArrayEquals(output, destination.readBytes())
    }

    @Test fun `failed recovery readback cannot claim verified overwrite`() {
        destination.writeText("original")
        io.afterPublish = { io.readFailure = { File(it) != destination } }
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_UNVERIFIED")
        assertTrue(failure.data.flag("retained_entry_ownership_unverified"))
        assertEquals("original", retained(failure.data, "retained_backup").readText())
    }

    @Test fun `replaced staging name is neither published nor deleted`() {
        destination.writeText("original")
        io.afterWrite = {
            val stage = root.listFiles()!!.single { it.name.startsWith(".sgt-file-") }
            stage.writeText("foreign")
            io.replaceIdentity(stage.path)
        }
        val failure = failure(commit(), "ERR_FILE_TRANSACTION_FAILED")
        assertFalse(failure.data.flag("target_effect_may_have_occurred"))
        assertTrue(failure.data.flag("retained_entry_ownership_unverified"))
        assertEquals("foreign", retained(failure.data, "retained_staging_path").readText())
        assertEquals("original", destination.readText())
    }

    @Test fun `parent creation error reports attempted effects without claiming no effect`() {
        val path = File(root, "missing/output.txt")
        io.afterDirectoryCreate = { throw IOException("Uncertain mkdir result") }
        prepare(path).use { target ->
            val failure = failure(commit(target), "ERR_FILE_TRANSACTION_FAILED")
            assertEquals(EffectCertainty.MAY_HAVE_OCCURRED, failure.effect)
            assertTrue(requireNotNull(path.parentFile).isDirectory)
            assertEquals(path.parent, failure.data.getValue("attempted_parent_paths").jsonArray.single().jsonPrimitive.content)
            assertFalse(path.exists())
        }
    }

    @Test fun `prepared target is one shot even after successful publication`() {
        prepare().use { target ->
            assertTrue(commit(target) is AndroidProviderResult.Success)
            val failure = failure(commit(target), "ERR_FILE_TARGET_LEASE_CHANGED")
            assertEquals(EffectCertainty.PROVEN_NO_EFFECT, failure.effect)
            assertArrayEquals(output, destination.readBytes())
        }
    }

    private fun prepare(path: File = destination) = PreparedFileTarget.prepare(path.absolutePath, 1024, io)
    private fun commit(owner: PhoneControlEffectOwner = testFileOwner(), authorized: () -> Boolean = { true }): AndroidProviderResult =
        prepare().use { commit(it, owner, authorized) }
    private fun commit(target: PreparedFileTarget, owner: PhoneControlEffectOwner = testFileOwner(), authorized: () -> Boolean = { true }) =
        FilePublication.commit(target, output, output.sha256(), owner, authorized)
    private fun failure(result: AndroidProviderResult, code: String): AndroidProviderResult.Failure {
        assertTrue("Expected failure $code, got $result", result is AndroidProviderResult.Failure)
        return (result as AndroidProviderResult.Failure).also { assertEquals(code, it.code) }
    }
    private fun retained(data: JsonObject, key: String) = File(data.text(key)).also { assertTrue(it.isFile) }
    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.flag(key: String) = getValue(key).jsonPrimitive.boolean
}
