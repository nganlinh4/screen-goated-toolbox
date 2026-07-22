package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import java.io.RandomAccessFile
import java.nio.file.Files
import java.nio.file.StandardCopyOption

internal object CreationPreparationCooldown {
    fun read(context: Context): Long = runCatching {
        cooldownFile(context).readText().trim().toLong()
    }.getOrDefault(0L)

    fun recordUntil(context: Context, requestedUntilMs: Long): Long {
        return withLock(context) {
            val untilMs = maxOf(read(context), requestedUntilMs)
            writeAtomically(cooldownFile(context), untilMs.toString())
            untilMs
        }
    }

    fun recordMailboxFailure(context: Context): Long = withLock(context) {
        val streak = (readStreak(context) + 1).coerceAtMost(RATE_LIMIT_BACKOFF_MS.size)
        val untilMs = maxOf(
            read(context),
            System.currentTimeMillis() + mailboxFailureBackoffMs(streak),
        )
        writeAtomically(streakFile(context), streak.toString())
        writeAtomically(cooldownFile(context), untilMs.toString())
        untilMs
    }

    fun recordPreparationSucceeded(context: Context) = withLock(context) {
        streakFile(context).delete()
        if (read(context) <= System.currentTimeMillis()) cooldownFile(context).delete()
    }

    private fun cooldownFile(context: Context) =
        File(context.filesDir, "creation/mailbox-cooldown-until-ms")

    private fun streakFile(context: Context) =
        File(context.filesDir, "creation/mailbox-rate-limit-streak")

    private fun readStreak(context: Context): Int = runCatching {
        streakFile(context).readText().trim().toInt()
    }.getOrDefault(0)

    private inline fun <T> withLock(context: Context, action: () -> T): T {
        val lockFile = File(context.filesDir, "creation/mailbox-cooldown.lock")
        lockFile.parentFile?.mkdirs()
        return RandomAccessFile(lockFile, "rw").use { lock ->
            lock.channel.lock().use { action() }
        }
    }

    private fun writeAtomically(target: File, value: String) {
        target.parentFile?.mkdirs()
        val temporary = File(target.parentFile, "${target.name}.tmp-${android.os.Process.myPid()}")
        temporary.writeText(value)
        runCatching {
            Files.move(
                temporary.toPath(),
                target.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        }.getOrElse {
            Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
        }
    }

    internal fun mailboxFailureBackoffMs(streak: Int): Long {
        require(streak > 0)
        return RATE_LIMIT_BACKOFF_MS[(streak - 1).coerceAtMost(RATE_LIMIT_BACKOFF_MS.lastIndex)]
    }

    private val RATE_LIMIT_BACKOFF_MS = longArrayOf(
        5 * 60_000L,
        10 * 60_000L,
        15 * 60_000L,
    )
}
