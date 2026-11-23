package dev.screengoated.toolbox.mobile.phonecontrol.provider

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PreparedArtifactSaveTest {
    @get:Rule val temporary = TemporaryFolder()
    private val store = PhoneControlArtifactStore(nowMillis = { 1 })
    private val io = NioTestFileTransactionIo()
    private val provider = AndroidFileProvider(store, io)

    @Test fun `source and target metadata are frozen before authorization without exporting payload`() {
        val input = "private source payload".toByteArray()
        val artifact = store.put(input, "text/plain")
        val destination = File(temporary.root, "absent/output.txt")
        provider.prepareArtifactSave(artifact.id, destination.path, true).use { prepared ->
            input.fill(0)
            artifact.bytes.fill(0)
            val proposal = prepared.mutation.proposal
            assertEquals(artifact.sha256, proposal.getValue("source_sha256").jsonPrimitive.content)
            assertFalse(proposal.toString().contains("private source payload"))
            assertFalse(requireNotNull(destination.parentFile).exists())
            val result = prepared.commit(testFileOwner(), { true }) as AndroidProviderResult.Success
            assertTrue(result.effectVerified)
            assertEquals("private source payload", destination.readText())
        }
    }

    @Test fun `retired source cannot admit an export or create missing parents`() {
        val artifact = store.put("payload".toByteArray(), "text/plain")
        val destination = File(temporary.root, "absent/output.txt")
        provider.prepareArtifactSave(artifact.id, destination.path, true).use { prepared ->
            store.close()
            val result = prepared.commit(testFileOwner(), { true }) as AndroidProviderResult.Failure
            assertEquals("artifact_store_closed", result.code)
            assertEquals(EffectCertainty.PROVEN_NO_EFFECT, result.effect)
            assertFalse(requireNotNull(destination.parentFile).exists())
        }
    }

    @Test fun `already admitted explicit export finishes from immutable source after retirement`() {
        val artifact = store.put("payload".toByteArray(), "text/plain")
        val destination = File(temporary.root, "output.txt")
        io.beforeWrite = { store.close() }
        provider.prepareArtifactSave(artifact.id, destination.path, true).use { prepared ->
            val result = prepared.commit(testFileOwner(), { true }) as AndroidProviderResult.Success
            assertTrue(result.effectVerified)
            assertEquals("payload", destination.readText())
            assertNull(store.get(artifact.id))
        }
    }

    @Test fun `missing overwrite target stays create only at the final namespace boundary`() {
        val artifact = store.put("payload".toByteArray(), "text/plain")
        val destination = File(temporary.root, "output.txt")
        io.beforePublish = { destination.writeText("competitor"); io.replaceIdentity(destination.path) }
        provider.prepareArtifactSave(artifact.id, destination.path, true).use { prepared ->
            val result = prepared.commit(testFileOwner(), { true }) as AndroidProviderResult.Failure
            assertEquals("ERR_FILE_TARGET_LEASE_CHANGED", result.code)
            assertEquals("competitor", destination.readText())
            assertEquals("payload", File(result.data.getValue("retained_staging_path").jsonPrimitive.content).readText())
        }
    }
}
