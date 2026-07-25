package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class AndroidFileProviderTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `file mutations consume the shared parity invariants`() {
        val invariants = contract.getValue("invariants").jsonObject

        assertEquals(4, contract.getValue("schemaVersion").jsonPrimitive.int)
        assertTrue(invariants.getValue("sameCanonicalPathSerializedWithinProcess").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("createWithoutOverwriteIsAtomic").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("exactReplaceRevalidatesExpectedHashAtCommit").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("exactReplacementGroupsArePlannedAgainstOneBaseline").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("overlappingReplacementRangesAreRejected").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("ordinaryDelimitedEditPreservesRecordShape").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("ordinaryDelimitedEditPreservesFormulaCells").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("formulaAndOrdinaryDataCannotChangeInOneEdit").jsonPrimitive.boolean)
        assertTrue(
            invariants
                .getValue("unambiguousDelimitedSerializationRepairIsCrossPlatform")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("everyDedicatedFileWriteRequiresIndependentTargetScopeQuorum")
                .jsonPrimitive
                .boolean,
        )
        assertEquals(
            2,
            invariants
                .getValue("targetScopeQuorumMinimumPositiveVerdicts")
                .jsonPrimitive
                .int,
        )
        assertTrue(
            invariants
                .getValue("targetScopeQuorumRejectsAnyNegativeVerdict")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("targetScopeProposalExcludesReplacementAndArtifactContent")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("targetScopeProposalIsPayloadTypeAgnostic")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("completedTurnScopeDoesNotLeakIntoNextIndependentTurn")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("ordinaryAndStructuralEditsShareCanonicalTargetScopeIdentity")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("targetScopeAuthorizationDoesNotAuthorizeMutationContent")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("authorizedTargetLeaseRevalidatedAtCommit")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("structuralTokenIdentifiesExactBytesButDoesNotAuthorize")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("structuralCommitRequiresIndependentUserRequestQuorum")
                .jsonPrimitive
                .boolean,
        )
        assertEquals(
            2,
            invariants
                .getValue("structuralQuorumMinimumPositiveVerdicts")
                .jsonPrimitive
                .int,
        )
        assertTrue(
            invariants
                .getValue("structuralQuorumRejectsAnyNegativeVerdict")
                .jsonPrimitive
                .boolean,
        )
        assertTrue(
            invariants
                .getValue("structuralCommitRevalidatesHashTokenAndProposal")
                .jsonPrimitive
                .boolean,
        )
    }

    @Test
    fun `replacement groups are planned against one immutable baseline`() {
        val destination = temporaryFolder.newFile("baseline.txt").apply { writeText("alpha beta") }

        val result = replace(
            destination,
            ExactReplacement("alpha", "gamma", 1),
            ExactReplacement("gamma", "delta", 1),
        )

        assertFailure(result, "ERR_TEXT_FILE_MATCH_MISSING")
        assertEquals("alpha beta", destination.readText())
    }

    @Test
    fun `overlapping replacement ranges are rejected before writing`() {
        val destination = temporaryFolder.newFile("overlap.txt").apply { writeText("abcd") }

        val result = replace(
            destination,
            ExactReplacement("abc", "left", 1),
            ExactReplacement("bcd", "right", 1),
        )

        assertFailure(result, "ERR_TEXT_FILE_OVERLAPPING_REPLACEMENTS")
        assertEquals("abcd", destination.readText())
    }

    @Test
    fun `provider rejects empty exact match without entering an unbounded search`() {
        val destination = temporaryFolder.newFile("empty-match.txt").apply { writeText("base") }

        val result = replace(
            destination,
            ExactReplacement("", "value", 1),
        )

        assertFailure(result, "ERR_TEXT_FILE_BAD_ARGUMENT")
        assertEquals("base", destination.readText())
    }

    @Test
    fun `ordinary delimited edit rejects record shape drift`() {
        val original = "name,value,eligible\r\nalpha,1,\"=A2>0\"\r\n"
        val destination = temporaryFolder.newFile("shape.csv").apply { writeText(original) }

        val result = replace(
            destination,
            ExactReplacement("alpha,1,\"=A2>0\"", "alpha,2", 1),
        )

        val failure = assertFailure(
            result,
            expected("ordinary delimited edit rejects record-shape drift", "resultCode"),
        )
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertEquals(original, destination.readText())
    }

    @Test
    fun `ordinary row edit restores exact formula bytes`() {
        val original = "name,status,total\r\nalpha,Unknown,\"=B2*12\"\r\n"
        val destination = temporaryFolder.newFile("formula.csv").apply { writeText(original) }

        val result = replace(
            destination,
            ExactReplacement(
                "alpha,Unknown,\"=B2*12\"",
                "alpha,Ready,\"=99\"",
                1,
            ),
        ) as AndroidProviderResult.Success

        assertEquals(
            "name,status,total\r\nalpha,Ready,\"=B2*12\"\r\n",
            destination.readText(),
        )
        assertEquals(1L, result.data.getValue("formula_cells_auto_preserved").jsonPrimitive.long)
        assertEquals(
            1,
            result.data.getValue("formula_replacement_groups_rewritten").jsonPrimitive.int,
        )
        val structure = result.data.getValue("structure").jsonObject
        assertTrue(structure.getValue("record_shape_preserved").jsonPrimitive.boolean)
        assertTrue(structure.getValue("formulas_preserved").jsonPrimitive.boolean)
    }

    @Test
    fun `ordinary table work omits a separate formula-only group`() {
        val destination = temporaryFolder.newFile("formula.tsv").apply {
            writeText("name\tstatus\ttotal\nalpha\tUnknown\t=B2*12\n")
        }

        val result = replace(
            destination,
            ExactReplacement("Unknown", "Ready", 1),
            ExactReplacement("=B2*12", "=99", 1),
        ) as AndroidProviderResult.Success

        assertEquals("name\tstatus\ttotal\nalpha\tReady\t=B2*12\n", destination.readText())
        assertEquals(2, result.data.getValue("requested_replacement_groups").jsonPrimitive.int)
        assertEquals(1, result.data.getValue("replacement_groups").jsonPrimitive.int)
        assertEquals(1, result.data.getValue("formula_only_groups_omitted").jsonPrimitive.int)
    }

    @Test
    fun `ordinary row edit repairs split formula quoting`() {
        val original = "name,status,eligible\r\nalpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\n"
        val destination = temporaryFolder.newFile("quoted-formula.csv").apply { writeText(original) }

        val result = replace(
            destination,
            ExactReplacement(
                "alpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"",
                "alpha,Ready,=AND(B2>0,C2=\"Yes\")",
                1,
            ),
        )

        assertTrue(result is AndroidProviderResult.Success)
        assertEquals(
            "name,status,eligible\r\nalpha,Ready,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\n",
            destination.readText(),
        )
    }

    @Test
    fun `ordinary row edit removes only redundant trailing empty fields`() {
        val destination = temporaryFolder.newFile("trailing-empty.csv").apply {
            writeText("Label,Pending\r\n")
        }

        val result = replace(
            destination,
            ExactReplacement("Label,Pending", "Label,Ready,,", 1),
        ) as AndroidProviderResult.Success

        assertEquals("Label,Ready\r\n", destination.readText())
        assertEquals(2L, result.data.getValue("trailing_empty_fields_omitted").jsonPrimitive.long)
    }

    @Test
    fun `ordinary row edit quotes an unambiguous split trailing value`() {
        val destination = temporaryFolder.newFile("trailing-value.csv").apply {
            writeText("Field,Value\r\nRationale,Unknown\r\n")
        }

        val result = replace(
            destination,
            ExactReplacement(
                "Rationale,Unknown",
                "Rationale,Supports families, shared vaults, and recovery",
                1,
            ),
        ) as AndroidProviderResult.Success

        assertEquals(
            "Field,Value\r\nRationale,\"Supports families, shared vaults, and recovery\"\r\n",
            destination.readText(),
        )
        assertEquals(2L, result.data.getValue("trailing_value_fields_repaired").jsonPrimitive.long)
    }

    @Test
    fun `formula-only edit remains rejected by the ordinary tool`() {
        val original = "name,total\nalpha,=B2*12\n"
        val destination = temporaryFolder.newFile("formula-only.csv").apply { writeText(original) }

        val result = replace(
            destination,
            ExactReplacement("=B2*12", "=99", 1),
        )

        assertFailure(result, "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL")
        assertEquals(original, destination.readText())
    }

    @Test
    fun `structural preflight returns a cross-platform token without writing`() {
        val original = "name,value\nalpha,1\n"
        val destination = temporaryFolder.newFile("structure.csv").apply { writeText(original) }
        val provider = AndroidFileProvider { null }
        val expectedSha256 = destination.readBytes().sha256()
        val replacements = listOf(
            ExactReplacement(
                "name,value\nalpha,1\n",
                "name,value,extra\nalpha,1,x\n",
                1,
            ),
        )

        val result = provider.structuralPreflight(
            destination.absolutePath,
            expectedSha256,
            replacements,
            suppliedToken = null,
        )

        val failure = assertFailure(
            result,
            expected("structural preflight never commits", "resultCode"),
        )
        assertEquals(
            "0bd5b6ed018b8ef3b0460750e9c4be5b705430fbb48b1d088ca0742f6f3f5385",
            failure.data.getValue("structural_change_token").jsonPrimitive.content,
        )
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertEquals(original, destination.readText())
    }

    @Test
    fun `validated structural preflight stays no-effect until private commit`() {
        val original = "name,value\nalpha,1\n"
        val destination = temporaryFolder.newFile("validated-structure.csv").apply {
            writeText(original)
        }
        val provider = AndroidFileProvider { null }
        val expectedSha256 = destination.readBytes().sha256()
        val replacements = listOf(
            ExactReplacement(
                "name,value\nalpha,1\n",
                "name,value,extra\nalpha,1,x\n",
                1,
            ),
        )
        val first = provider.structuralPreflight(
            destination.absolutePath,
            expectedSha256,
            replacements,
            suppliedToken = null,
        ) as AndroidProviderResult.Failure
        val token = first.data.getValue("structural_change_token").jsonPrimitive.content

        val ready = provider.structuralPreflight(
            destination.absolutePath,
            expectedSha256,
            replacements,
            token,
        ) as AndroidProviderResult.Success

        assertTrue(ready.data.getValue("ready_for_request_contract_check").jsonPrimitive.boolean)
        assertTrue(ready.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertFalse(ready.effectMayHaveOccurred)
        assertEquals(original, destination.readText())
    }

    @Test
    fun `structural commit revalidates current hash and preserves competing bytes`() {
        val original = "name,value\nalpha,1\n"
        val destination = temporaryFolder.newFile("stale-structure.csv").apply {
            writeText(original)
        }
        val provider = AndroidFileProvider { null }
        val expectedSha256 = destination.readBytes().sha256()
        val replacements = listOf(
            ExactReplacement(
                "name,value\nalpha,1\n",
                "name,value,extra\nalpha,1,x\n",
                1,
            ),
        )
        val first = provider.structuralPreflight(
            destination.absolutePath,
            expectedSha256,
            replacements,
            suppliedToken = null,
        ) as AndroidProviderResult.Failure
        val token = first.data.getValue("structural_change_token").jsonPrimitive.content
        destination.writeText("external,bytes\nremain,here\n")

        val committed = provider.commitStructuralAfterAuthorization(
            destination.absolutePath,
            expectedSha256,
            replacements,
            token,
            targetLease(destination),
        )

        assertFailure(committed, "hash_mismatch")
        assertEquals("external,bytes\nremain,here\n", destination.readText())
    }

    @Test
    fun `structural token cannot authorize different proposed bytes`() {
        val original = "name,value\nalpha,1\n"
        val destination = temporaryFolder.newFile("proposal-bound.csv").apply {
            writeText(original)
        }
        val provider = AndroidFileProvider { null }
        val expectedSha256 = destination.readBytes().sha256()
        val firstProposal = listOf(
            ExactReplacement(
                "name,value\nalpha,1\n",
                "name,value,extra\nalpha,1,x\n",
                1,
            ),
        )
        val secondProposal = listOf(
            ExactReplacement(
                "name,value\nalpha,1\n",
                "name,value,other\nalpha,1,y\n",
                1,
            ),
        )
        val first = provider.structuralPreflight(
            destination.absolutePath,
            expectedSha256,
            firstProposal,
            suppliedToken = null,
        ) as AndroidProviderResult.Failure
        val firstToken = first.data.getValue("structural_change_token").jsonPrimitive.content

        val result = provider.commitStructuralAfterAuthorization(
            destination.absolutePath,
            expectedSha256,
            secondProposal,
            firstToken,
            targetLease(destination),
        )

        val rejected = assertFailure(result, "ERR_TEXT_FILE_STRUCTURE_CHANGE")
        assertFalse(
            firstToken == rejected.data.getValue("structural_change_token").jsonPrimitive.content,
        )
        assertEquals(original, destination.readText())
    }

    @Test
    fun `utf8 bom survives a verified exact replacement`() {
        val destination = temporaryFolder.newFile("bom.txt")
        val original = byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte()) +
            "before".toByteArray()
        destination.writeBytes(original)

        val result = replace(
            destination,
            ExactReplacement("before", "after", 1),
        )

        assertTrue(result is AndroidProviderResult.Success)
        assertTrue(
            destination.readBytes().contentEquals(
                byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte()) +
                    "after".toByteArray(),
            ),
        )
    }

    private fun replace(
        destination: File,
        vararg replacements: ExactReplacement,
    ): AndroidProviderResult = AndroidFileProvider { null }.exactReplace(
        destination.absolutePath,
        destination.readBytes().sha256(),
        replacements.toList(),
        targetLease(destination),
    )

    private fun targetLease(file: File): FileMutationTargetLease {
        val target = file.canonicalFile
        val existedBefore = target.isFile
        return FileMutationTargetLease(
            canonicalPath = target.absolutePath,
            existedBefore = existedBefore,
            expectedSha256 = if (existedBefore) target.readBytes().sha256() else null,
        )
    }

    private fun assertFailure(
        result: AndroidProviderResult,
        code: String,
    ): AndroidProviderResult.Failure {
        assertTrue(result is AndroidProviderResult.Failure)
        return (result as AndroidProviderResult.Failure).also {
            assertEquals(code, it.code)
        }
    }

    private fun expected(caseName: String, field: String): String = contract
        .getValue("cases")
        .jsonArray
        .map { it.jsonObject }
        .single { it.getValue("name").jsonPrimitive.content == caseName }
        .getValue("expect")
        .jsonObject
        .getValue(field)
        .jsonPrimitive
        .content

    private val contract by lazy {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val root = generateSequence(File(workingDirectory).canonicalFile) { it.parentFile }
            .first { File(it, CONTRACT_PATH).isFile }
        Json.parseToJsonElement(File(root, CONTRACT_PATH).readText()).jsonObject
    }
}

private const val CONTRACT_PATH = "parity-fixtures/phone-control/file-mutation-contract.json"
