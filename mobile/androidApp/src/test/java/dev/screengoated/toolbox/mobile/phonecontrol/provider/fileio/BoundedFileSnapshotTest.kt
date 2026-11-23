package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class BoundedFileSnapshotTest {
    @Test fun sharedObservationsRejectChangedOrIncompleteSnapshots() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("phone-control/file-snapshot-observation.json") }.first(Files::exists)
        val cases = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject.getValue("cases").jsonArray
        val before = FileSnapshotMetadata(3, 10, 11, 20, 21)
        for (entry in cases) {
            val case = entry.jsonObject
            val event = case.getValue("event").jsonPrimitive.content
            var metadataCalls = 0
            var reads = 0
            val outcome = runCatching {
                boundedFileSnapshot(if (event == "size_exceeds_limit") 2 else 3, metadata = {
                    if (metadataCalls++ == 0) before else when (event) {
                        "modified_nanoseconds_change" -> before.copy(modifiedNanoseconds = 12)
                        "changed_nanoseconds_change" -> before.copy(changedNanoseconds = 22)
                        else -> before
                    }
                }, read = {
                    reads++
                    when {
                        event == "short_read" -> "ab".toByteArray()
                        event == "different_reads" && reads == 2 -> "abd".toByteArray()
                        else -> "abc".toByteArray()
                    }
                })
            }
            assertEquals(event, case.getValue("result").jsonPrimitive.content,
                if (outcome.isSuccess) "accepted" else "rejected")
        }
    }
}
