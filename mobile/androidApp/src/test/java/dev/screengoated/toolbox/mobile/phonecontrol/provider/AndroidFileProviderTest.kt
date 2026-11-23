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

        assertEquals(6, contract.getValue("schemaVersion").jsonPrimitive.int)
        assertTrue(invariants.getValue("sameCanonicalPathSerializedWithinProcess").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("createWithoutOverwriteIsAtomic").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("exactReplaceRevalidatesExpectedHashAtCommit").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("exactReplacementGroupsArePlannedAgainstOneBaseline").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("overlappingReplacementRangesAreRejected").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("ordinaryDelimitedEditPreservesRecordShape").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("ordinaryDelimitedEditPreservesFormulaCells").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("formulaAndOrdinaryDataCannotChangeInOneEdit").jsonPrimitive.boolean)
        assertEquals(65_536, invariants.getValue("maxTotalReplacementOccurrences").jsonPrimitive.int)
        assertTrue(invariants.getValue("finalByteLimitIncludesUtf8Bom").jsonPrimitive.boolean)
        assertTrue(invariants.getValue("unpairedSurrogateNeverCommitted").jsonPrimitive.boolean)
        assertTrue(
            invariants
                .getValue("requestedReplacementGroupsAreAppliedVerbatimOrRejected")
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
    fun `shared exact replacement cases preserve requested bytes or reject`() {
        val cases = contract.getValue("exactReplacementCases").jsonArray
        cases.forEachIndexed { index, entry ->
            val case = entry.jsonObject
            val original = case.getValue("original").jsonPrimitive.content
            val extension = case.getValue("path").jsonPrimitive.content.substringAfterLast('.')
            val destination = temporaryFolder.newFile("exact-$index.$extension").apply {
                writeText(original)
            }
            val replacements = case.getValue("replacements").jsonArray.map { item ->
                val replacement = item.jsonObject
                ExactReplacement(
                    replacement.getValue("old_text").jsonPrimitive.content,
                    replacement.getValue("new_text").jsonPrimitive.content,
                    replacement.getValue("expected_count").jsonPrimitive.int,
                )
            }
            val result = replace(destination, *replacements.toTypedArray())
            val expectedCode = case.getValue("resultCode").jsonPrimitive.content
            if (expectedCode == "ok") {
                assertTrue(result is AndroidProviderResult.Success)
                assertTrue((result as AndroidProviderResult.Success).effectVerified)
                assertEquals(case.getValue("output").jsonPrimitive.content, destination.readText())
            } else {
                val failure = assertFailure(result, expectedCode)
                assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
                assertTrue(original.toByteArray().contentEquals(destination.readBytes()))
            }
        }
    }

    @Test
    fun `shared resource cases reject before writing`() {
        contract.getValue("resourceCases").jsonArray.forEachIndexed { index, entry ->
            val case = entry.jsonObject
            val unit = case.getValue("originalUnit").jsonPrimitive.content
            val count = case.getValue("repeatCount").jsonPrimitive.int
            val original = unit.repeat(count)
            val destination = temporaryFolder.newFile("resource-$index.txt").apply { writeText(original) }
            val result = replace(
                destination,
                ExactReplacement(unit, case.getValue("replacementUnit").jsonPrimitive.content, count),
            )
            val failure = assertFailure(result, case.getValue("resultCode").jsonPrimitive.content)
            assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
            assertTrue(original.toByteArray().contentEquals(destination.readBytes()))
        }
    }

    @Test
    fun `utf8 sizing accounts for surrogate pairs across replacement boundaries`() {
        val parts = listOf("a\uD83D", "\uDE00", "é")
        val expected = parts.joinToString("").toByteArray(Charsets.UTF_8).size.toLong()
        val exact = BoundedUtf8Counter(expected)
        parts.forEach { assertTrue(exact.append(it, 0, it.length)) }
        assertTrue(exact.complete())
        val tooSmall = BoundedUtf8Counter(expected - 1)
        val accepted = parts.all { tooSmall.append(it, 0, it.length) } && tooSmall.complete()
        assertFalse(accepted)
    }

    @Test
    fun `unpaired replacement surrogate is rejected without writing`() {
        val case = contract.getValue("encodingCases").jsonArray.single().jsonObject
        val replacement = case.getValue("utf16Units").jsonArray
            .map { it.jsonPrimitive.int.toChar() }
            .joinToString("")
        val destination = temporaryFolder.newFile("invalid-unicode.txt").apply { writeText("before") }
        val failure = assertFailure(
            replace(destination, ExactReplacement("before", replacement, 1)),
            case.getValue("androidResultCode").jsonPrimitive.content,
        )
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertEquals("before", destination.readText())
    }

    @Test
    fun `edited byte limit includes an existing utf8 bom`() {
        val destination = temporaryFolder.newFile("bounded-bom.txt")
        val prefix = byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte())
        val original = prefix + ("x".repeat(8 * 1024 * 1024 - 4) + "z").toByteArray()
        destination.writeBytes(original)
        val failure = assertFailure(
            replace(destination, ExactReplacement("z", "zz", 1)),
            "ERR_TEXT_FILE_TOO_LARGE",
        )
        assertTrue(failure.data.getValue("original_unchanged").jsonPrimitive.boolean)
        assertTrue(original.contentEquals(destination.readBytes()))
    }

    @Test
    fun `single replacement cannot silently alter a formula`() {
        val original = "name,status,total\r\nalpha,Unknown,\"=B2*12\"\r\n"
        val destination = temporaryFolder.newFile("formula.csv").apply { writeText(original) }
        val result = replace(
            destination,
            ExactReplacement(
                "alpha,Unknown,\"=B2*12\"",
                "alpha,Ready,\"=99\"",
                1,
            ),
        )
        assertFailure(result, "ERR_TEXT_FILE_FORMULA_MIXED_EDIT")
        assertEquals(original, destination.readText())
    }

    @Test
    fun `ordinary edit preserves an unchanged formula exactly`() {
        val original = "name,status,total\r\nalpha,Unknown,\"=B2*12\"\r\n"
        val destination = temporaryFolder.newFile("unchanged-formula.csv").apply { writeText(original) }
        val result = replace(
            destination,
            ExactReplacement(
                "alpha,Unknown,\"=B2*12\"",
                "alpha,Ready,\"=B2*12\"",
                1,
            ),
        )
        assertTrue(result is AndroidProviderResult.Success)
        assertTrue((result as AndroidProviderResult.Success).effectVerified)
        assertEquals("name,status,total\r\nalpha,Ready,\"=B2*12\"\r\n", destination.readText())
    }

    @Test
    fun `ordinary edit retains multiline quoted formula bytes`() {
        val original = "name,total\r\nalpha,\"=SUM(\r\nA1,B1)\"\r\n"
        val destination = temporaryFolder.newFile("formula-multiline.csv").apply { writeText(original) }
        val result = replace(destination, ExactReplacement("alpha", "beta", 1))
        assertTrue(result is AndroidProviderResult.Success)
        assertEquals("name,total\r\nbeta,\"=SUM(\r\nA1,B1)\"\r\n", destination.readText())
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
        val provider = testFileProvider { null }
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
        val provider = testFileProvider { null }
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
        val provider = testFileProvider { null }
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

        val committed = provider.prepareTarget(destination.absolutePath).use { target -> provider.commitStructuralAfterAuthorization(
            target,
            expectedSha256,
            replacements,
            token,
            testFileOwner(), { true },
        ) }

        assertFailure(committed, "hash_mismatch")
        assertEquals("external,bytes\nremain,here\n", destination.readText())
    }

    @Test
    fun `structural token cannot authorize different proposed bytes`() {
        val original = "name,value\nalpha,1\n"
        val destination = temporaryFolder.newFile("proposal-bound.csv").apply {
            writeText(original)
        }
        val provider = testFileProvider { null }
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

        val result = provider.prepareTarget(destination.absolutePath).use { target -> provider.commitStructuralAfterAuthorization(
            target,
            expectedSha256,
            secondProposal,
            firstToken,
            testFileOwner(), { true },
        ) }

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

    private fun replace(destination: File, vararg replacements: ExactReplacement): AndroidProviderResult {
        val provider = testFileProvider { null }
        val target = try { provider.prepareTarget(destination.absolutePath) }
            catch (error: Throwable) { return provider.preparationFailure(error) }
        return target.use { provider.exactReplace(it, destination.readBytes().sha256(),
            replacements.toList(), testFileOwner(), { true }) }
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
