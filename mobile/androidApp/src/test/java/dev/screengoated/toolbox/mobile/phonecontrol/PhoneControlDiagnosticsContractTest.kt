package dev.screengoated.toolbox.mobile.phonecontrol

import java.io.File
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class PhoneControlDiagnosticsContractTest {
    @Test
    fun `Android diagnostics obey the shared privacy and recovery contract`() {
        val root = Json.parseToJsonElement(fixture().readText()).jsonObject
        val journal = root.getValue("journal").jsonObject
        val fieldAdmission = journal.getValue("fieldAdmission").jsonObject
        val legacyCompaction = journal.getValue("legacyInvalidationCompaction").jsonObject
        val timelineTail = journal.getValue("timelineTail").jsonObject
        val toolEvents = root.getValue("toolEvents").jsonObject
        val invalidArguments = root.getValue("invalidArgumentClassification").jsonObject
        val captureRoutes = root.getValue("captureRouteDiagnostics").jsonObject
        val recovery = root.getValue("sameGenerationTargetRecovery").jsonObject
        val postconditions = root.getValue("postconditions").jsonObject

        assertEquals(5L, root.getValue("schemaVersion").jsonPrimitive.long)
        assertEquals(
            PhoneControlLog.RECORD_SCHEMA_VERSION.toLong(),
            journal.getValue("recordSchemaVersion").jsonPrimitive.long,
        )
        assertFalse(journal.getValue("persistsTranscriptText").jsonPrimitive.boolean)
        assertFalse(journal.getValue("persistsFreeFormMessage").jsonPrimitive.boolean)
        assertFalse(journal.getValue("semanticOnlyPeriodicRecords").jsonPrimitive.boolean)
        assertEquals(
            "typed_allowlist",
            fieldAdmission.getValue("mode").jsonPrimitive.content,
        )
        assertEquals(
            "allowlist",
            fieldAdmission.getValue("eventMode").jsonPrimitive.content,
        )
        assertEquals(
            "omit",
            fieldAdmission.getValue("unknownField").jsonPrimitive.content,
        )
        assertEquals(
            "omit",
            fieldAdmission.getValue("throwableMessage").jsonPrimitive.content,
        )
        assertFalse(legacyCompaction.getValue("timelineRecords").jsonPrimitive.boolean)
        assertEquals(
            listOf(
                "legacy_invalidation_records_compacted",
                "legacy_hard_invalidation_count",
                "legacy_semantic_invalidation_count",
            ),
            legacyCompaction.getValue("summaryFields").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            600L,
            timelineTail.getValue("maximumRecords").jsonPrimitive.long,
        )
        assertEquals(
            "timeline_omitted_count",
            timelineTail.getValue("summaryOmittedField").jsonPrimitive.content,
        )
        assertEquals(
            "sorted_names_only_no_values",
            toolEvents.getValue("argumentKeys").jsonPrimitive.content,
        )
        assertEquals(
            listOf("failure_class", "provider_route_error"),
            toolEvents.getValue("optionalFailureRoutingFields").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            listOf("argument_field", "contract_reason"),
            toolEvents.getValue("optionalContractFailureFields").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertEquals(
            listOf("argument_field", "contract_reason"),
            invalidArguments.getValue("requiredFields").jsonArray.map {
                it.jsonPrimitive.content
            },
        )
        assertFalse(invalidArguments.getValue("argumentValuesPersisted").jsonPrimitive.boolean)
        assertEquals(
            listOf("provider", "route", "overlay_mutated"),
            captureRoutes.getValue("fields").jsonArray.map { it.jsonPrimitive.content },
        )
        assertFalse(captureRoutes.getValue("periodicHeartbeat").jsonPrimitive.boolean)
        assertEquals(
            listOf("snapshot_generation", "display_id", "window_id"),
            recovery.getValue("scope").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(1L, recovery.getValue("requiredMatchCount").jsonPrimitive.long)
        assertFalse(recovery.getValue("crossGenerationRebind").jsonPrimitive.boolean)
        assertEquals(
            "postcondition_unavailable",
            postconditions.getValue("providerReadFailureCode").jsonPrimitive.content,
        )
        assertEquals(
            "postcondition_not_verified",
            postconditions.getValue("requiredStateMismatchCode").jsonPrimitive.content,
        )
        assertFalse(postconditions.getValue("mayReportOk").jsonPrimitive.boolean)
    }

    private fun fixture(): File = candidateRoots()
        .map { root -> File(root, FIXTURE_PATH) }
        .firstOrNull(File::isFile)
        ?: error("$FIXTURE_PATH was not found from ${System.getProperty("user.dir")}")

    private fun candidateRoots(): List<File> {
        val cwd = Paths.get(System.getProperty("user.dir")).toAbsolutePath().normalize()
        return generateSequence(cwd) { path -> path.parent }
            .map { path -> path.toFile() }
            .toList()
    }

    private companion object {
        const val FIXTURE_PATH = "parity-fixtures/phone-control/diagnostics-contract.json"
    }
}
