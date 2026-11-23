package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationHistoryMaintenanceTest {
    @Test
    fun maintenanceWaitsForManagerStartupRecovery() = runTest {
        val startup = CompletableDeferred<Unit>()
        val calls = mutableListOf<String>()
        val job = launch {
            maintainCreationHistoryAfterStartup(
                awaitStartup = { startup.await() },
                maintain = { calls += "maintain" },
            )
        }
        yield()
        assertEquals(emptyList<String>(), calls)
        startup.complete(Unit)
        job.join()
        assertEquals(listOf("maintain"), calls)
    }

    @Test
    fun failedStartupOrMaintenanceNeverEscapes() = runTest {
        var maintained = false
        maintainCreationHistoryAfterStartup(
            awaitStartup = { throw IOException("journal unreadable") },
            maintain = { maintained = true },
        )
        assertFalse(maintained)
        maintainCreationHistoryAfterStartup(
            awaitStartup = {},
            maintain = { throw IllegalStateException("Creation history is unavailable") },
        )
    }

    @Test
    fun maintenanceUsesTheAppScopedManagerNotASecondFileStore() {
        val maintenance = source("CreationHistoryStore.kt")
            .substringAfter("internal object CreationHistoryMaintenance {")
            .substringBefore("\n}\n")
        assertTrue(maintenance.contains("CreationJobManager.get("))
        assertFalse(maintenance.contains("CreationFileStore("))
        assertFalse(maintenance.contains("CreationHistoryStore("))
    }

    @Test
    fun pendingCleanupLockIsSharedByEveryStoreInstance() {
        val store = source("CreationPendingCleanupStore.kt")
        assertTrue(store.contains("private val lock = creationPendingCleanupLock"))
        assertTrue(store.contains("\nprivate val creationPendingCleanupLock = Any()"))
    }

    private fun source(name: String): String {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/$name",
        ).readText()
    }
}
