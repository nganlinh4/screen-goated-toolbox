package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import java.io.File
import java.io.InputStream
import java.nio.charset.StandardCharsets
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.io.ByteArrayOutputStream
import kotlin.concurrent.thread
import kotlinx.coroutines.CancellationException

internal class YtDlpCommand(private val url: String) {
    private val arguments = mutableListOf<String>()

    fun addOption(option: String): YtDlpCommand = apply { arguments += option }

    fun addOption(option: String, argument: String): YtDlpCommand = apply {
        arguments += option
        arguments += argument
    }

    fun addOption(option: String, argument: Number): YtDlpCommand =
        addOption(option, argument.toString())

    fun hasOption(option: String): Boolean = option in arguments

    /**
     * `--` ends yt-dlp option parsing before the user's input (Windows
     * `append_url_operand`), so a pasted ID or text that starts with `-`, such
     * as `--config-locations=...` with an `--exec` line, stays a URL operand.
     */
    fun build(): List<String> = arguments + END_OF_OPTIONS + url

    private companion object {
        const val END_OF_OPTIONS = "--"
    }
}

internal data class YtDlpProcessResult(
    val exitCode: Int,
    val out: String,
    val error: String,
)

internal class DownloaderProcessHost(
    context: Context,
    private val installer: DownloaderRuntimeInstaller,
    private val upstreamStore: YtDlpInstallStore? = null,
    private val acquireLease: () -> AutoCloseable?,
) {
    private val context = context.applicationContext
    private val processes = ConcurrentHashMap<String, Process>()

    fun execute(
        request: YtDlpCommand,
        processId: String? = null,
        callback: ((Float, Long, String) -> Unit)? = null,
        analysis: AnalysisControl? = null,
    ): YtDlpProcessResult {
        val lease = acquireLease() ?: error("Downloader runtime is unavailable or removal is pending")
        var executableLease: YtDlpLease? = null
        var process: Process? = null
        val analysisReaders = mutableListOf<Thread>()
        try {
            executableLease = upstreamStore?.acquire()
            val executable = executableLease?.tool?.executable ?: installer.ytdlpFile
            check(installer.canRun(executable)) { "Downloader runtime is not installed" }
            val nativeDir = File(context.applicationInfo.nativeLibraryDir)
            val arguments = buildYtDlpProcessArguments(
                request,
                nativeDir.resolve("libffmpeg.so").absolutePath,
                nativeDir.resolve("libqjs_runner.so").absolutePath,
            )
            if (analysis?.isRetired() == true) throw CancellationException("Analysis retired")
            process = processBuilder(executable, arguments).start()
            if (analysis != null && !analysis.attach(process)) throw CancellationException("Analysis retired")
            if (processId != null && processes.putIfAbsent(processId, process) != null) {
                process.destroyForcibly()
                error("Downloader process ID already exists")
            }

            val out = StringBuffer()
            val errors = StringBuffer()
            val progress = ProgressParser()
            val overflow = AtomicBoolean(false)
            val stdout = if (analysis != null) readAnalysisStream(process.inputStream, out, 16 * 1024 * 1024, overflow) else readStream(process.inputStream, out) { line ->
                callback?.invoke(progress.fraction(line), progress.etaSeconds(line), line)
            }
            if (analysis != null) analysisReaders.add(stdout)
            val stderr = if (analysis != null) readAnalysisStream(process.errorStream, errors, 64 * 1024, overflow) else readStream(process.errorStream, errors, null)
            if (analysis != null) analysisReaders.add(stderr)
            if (analysis != null) {
                val started = System.nanoTime()
                while (!process.waitFor(100, TimeUnit.MILLISECONDS)) {
                    if (analysis.isRetired() || overflow.get() || System.nanoTime() - started >= TimeUnit.SECONDS.toNanos(90)) {
                        process.destroyForcibly()
                        process.waitFor()
                        stdout.join()
                        stderr.join()
                        if (analysis.isRetired()) throw CancellationException("Analysis retired")
                        if (overflow.get()) error("Analysis output exceeded its size limit")
                        error("Analysis timed out")
                    }
                }
            }
            val exitCode = process.waitFor()
            stdout.join()
            stderr.join()
            if (analysis?.isRetired() == true) throw CancellationException("Analysis retired")
            if (overflow.get()) error("Analysis output exceeded its size limit")
            val cancelled = processId != null && processes[processId] !== process
            if (processId != null) processes.remove(processId, process)
            if (cancelled) throw CancellationException("Downloader process was cancelled")
            if (exitCode != 0) {
                error(boundedYtDlpError(errors.toString(), exitCode))
            }
            return YtDlpProcessResult(exitCode, out.toString(), errors.toString())
        } catch (interrupted: InterruptedException) {
            process?.destroyForcibly()
            Thread.currentThread().interrupt()
            throw CancellationException("Downloader process was interrupted")
        } finally {
            if (process != null) {
                // Keep the executable and runtime leases until the owned child and
                // its pipe readers have actually settled, even after interruption.
                var interrupted = Thread.interrupted()
                if (process.isAlive) runCatching { process.destroyForcibly() }
                while (process.isAlive) {
                    try { process.waitFor() } catch (_: InterruptedException) { interrupted = true }
                }
                for (reader in analysisReaders) {
                    while (reader.isAlive) {
                        try { reader.join() } catch (_: InterruptedException) { interrupted = true }
                    }
                }
                if (interrupted) Thread.currentThread().interrupt()
            }
            if (processId != null && process != null) processes.remove(processId, process)
            if (process != null) analysis?.detach(process)
            try {
                executableLease?.close()
            } finally {
                lease.close()
            }
        }
    }

    fun verifyCandidate(executable: File, version: YtDlpVersion) {
        val process = processBuilder(
            executable,
            listOf("--ignore-config", "--no-plugin-dirs", "--no-update", "--version"),
        ).redirectErrorStream(true).start()
        try {
            check(process.waitFor(15, TimeUnit.SECONDS) && process.exitValue() == 0) {
                "This yt-dlp release could not start with the installed Android runtime"
            }
            val output = process.inputStream.use { it.readBytes().decodeToString().trim() }
            check(output == version.tag) { "The Android launcher reported an unexpected yt-dlp version" }
        } finally {
            if (process.isAlive) {
                process.destroyForcibly()
                process.waitFor()
            }
        }
    }

    private fun processBuilder(executable: File, arguments: List<String>): ProcessBuilder {
        val nativeDir = File(context.applicationInfo.nativeLibraryDir)
        val python = nativeDir.resolve("libpython.so")
        check(python.isFile && nativeDir.resolve("libffmpeg.so").isFile && nativeDir.resolve("libqjs_runner.so").isFile) {
            "Downloader executable launchers are missing"
        }
        return ProcessBuilder(listOf(python.absolutePath, executable.absolutePath) + arguments).apply {
            environment().apply {
                val pythonHome = installer.pythonDirectory.resolve("usr")
                remove("LD_LIBRARY_PATH")
                this["SGT_PYTHON_LIBRARY_DIR"] = pythonHome.resolve("lib").absolutePath
                this["SGT_FFMPEG_LIBRARY_DIR"] = installer.ffmpegDirectory.resolve("usr/lib").absolutePath
                this["SSL_CERT_FILE"] = pythonHome.resolve("etc/tls/cert.pem").absolutePath
                this["PATH"] = "${System.getenv("PATH").orEmpty()}:$nativeDir"
                this["PYTHONHOME"] = pythonHome.absolutePath
                this["HOME"] = pythonHome.absolutePath
                this["TMPDIR"] = context.cacheDir.absolutePath
            }
        }
    }

    fun destroy(processId: String): Boolean {
        val process = processes.remove(processId) ?: return false
        process.destroy()
        if (process.isAlive) process.destroyForcibly()
        return true
    }
}

internal fun readAnalysisStream(stream: InputStream, output: StringBuffer, limit: Int, overflow: AtomicBoolean): Thread =
    thread(name = "sgt-analysis-stream", start = true) {
        val collected = ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        stream.use {
            while (true) {
                val count = it.read(buffer)
                if (count < 0) break
                val remaining = limit - collected.size()
                if (count > remaining) overflow.set(true)
                if (remaining > 0) collected.write(buffer, 0, minOf(count, remaining))
            }
        }
        output.append(collected.toString(StandardCharsets.UTF_8.name()))
    }

internal fun buildYtDlpProcessArguments(
    request: YtDlpCommand,
    ffmpegPath: String,
    quickJsPath: String,
): List<String> = buildList {
    add("--ignore-config")
    add("--no-plugin-dirs")
    add("--no-update")
    add("--ffmpeg-location")
    add(ffmpegPath)
    add("--js-runtimes")
    add("quickjs:$quickJsPath")
    if (!request.hasOption("--cache-dir") && !request.hasOption("--no-cache-dir")) {
        add("--no-cache-dir")
    }
    addAll(request.build())
}

internal fun boundedYtDlpError(stderr: String, exitCode: Int): String {
    val lines = stderr.lineSequence().filter { it.isNotBlank() }.toList()
    val message = lines.takeLast(12).joinToString("\n").take(4_096)
    return message.ifBlank { "yt-dlp exited with code $exitCode" }
}

private fun readStream(
    stream: InputStream,
    output: StringBuffer,
    onLine: ((String) -> Unit)?,
): Thread = thread(name = "sgt-downloader-stream", start = true) {
    stream.bufferedReader(StandardCharsets.UTF_8).useLines { lines ->
        lines.forEach { line ->
            output.append(line).append('\n')
            onLine?.invoke(line)
        }
    }
}

private class ProgressParser {
    private var lastFraction = -1f
    private var lastEta = -1L

    fun fraction(line: String): Float {
        PROGRESS.find(line)?.groupValues?.get(1)?.toFloatOrNull()?.let { lastFraction = it }
        return lastFraction
    }

    fun etaSeconds(line: String): Long {
        ETA.find(line)?.let { match ->
            val hours = match.groupValues[1].toLongOrNull() ?: 0L
            val minutes = match.groupValues[2].toLongOrNull() ?: 0L
            val seconds = match.groupValues[3].toLongOrNull() ?: 0L
            lastEta = hours * 3600 + minutes * 60 + seconds
        }
        return lastEta
    }

    private companion object {
        val PROGRESS = Regex("\\[download]\\s+(\\d+(?:\\.\\d+)?)%")
        val ETA = Regex("ETA (?:(\\d+):)?(\\d+):(\\d+)")
    }
}
