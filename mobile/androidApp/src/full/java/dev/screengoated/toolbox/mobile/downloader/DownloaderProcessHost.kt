package dev.screengoated.toolbox.mobile.downloader

import android.content.Context
import java.io.InputStream
import java.nio.charset.StandardCharsets
import java.util.concurrent.ConcurrentHashMap
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

    fun build(): List<String> = arguments + url
}

internal data class YtDlpProcessResult(
    val exitCode: Int,
    val out: String,
    val error: String,
)

internal class DownloaderProcessHost(
    context: Context,
    private val installer: DownloaderRuntimeInstaller,
    private val acquireLease: () -> AutoCloseable?,
) {
    private val context = context.applicationContext
    private val processes = ConcurrentHashMap<String, Process>()

    fun execute(
        request: YtDlpCommand,
        processId: String? = null,
        callback: ((Float, Long, String) -> Unit)? = null,
    ): YtDlpProcessResult {
        val lease = acquireLease() ?: error("Downloader runtime is unavailable or removal is pending")
        var process: Process? = null
        try {
            check(installer.isInstalled()) { "Downloader runtime is not installed" }
            val nativeDir = context.applicationInfo.nativeLibraryDir
            val python = java.io.File(nativeDir, "libpython.so")
            val ffmpeg = java.io.File(nativeDir, "libffmpeg.so")
            val quickJs = java.io.File(nativeDir, "libqjs_runner.so")
            check(python.isFile && ffmpeg.isFile && quickJs.isFile) {
                "Downloader executable launchers are missing"
            }

            val arguments = buildYtDlpProcessArguments(
                request,
                ffmpeg.absolutePath,
                quickJs.absolutePath,
            )
            val command = listOf(python.absolutePath, installer.ytdlpFile.absolutePath) + arguments
            val builder = ProcessBuilder(command)
            builder.environment().apply {
                val pythonLibs = java.io.File(installer.pythonDirectory, "usr/lib")
                val ffmpegLibs = java.io.File(installer.ffmpegDirectory, "usr/lib")
                val pythonHome = java.io.File(installer.pythonDirectory, "usr")
                remove("LD_LIBRARY_PATH")
                this["SGT_PYTHON_LIBRARY_DIR"] = pythonLibs.absolutePath
                this["SGT_FFMPEG_LIBRARY_DIR"] = ffmpegLibs.absolutePath
                this["SSL_CERT_FILE"] = java.io.File(
                    installer.pythonDirectory,
                    "usr/etc/tls/cert.pem",
                ).absolutePath
                this["PATH"] = "${System.getenv("PATH").orEmpty()}:$nativeDir"
                this["PYTHONHOME"] = pythonHome.absolutePath
                this["HOME"] = pythonHome.absolutePath
                this["TMPDIR"] = context.cacheDir.absolutePath
            }
            process = builder.start()
            if (processId != null && processes.putIfAbsent(processId, process) != null) {
                process.destroyForcibly()
                error("Downloader process ID already exists")
            }

            val out = StringBuffer()
            val errors = StringBuffer()
            val progress = ProgressParser()
            val stdout = readStream(process.inputStream, out) { line ->
                callback?.invoke(progress.fraction(line), progress.etaSeconds(line), line)
            }
            val stderr = readStream(process.errorStream, errors, null)
            val exitCode = process.waitFor()
            stdout.join()
            stderr.join()
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
            if (processId != null && process != null) processes.remove(processId, process)
            lease.close()
        }
    }

    fun destroy(processId: String): Boolean {
        val process = processes.remove(processId) ?: return false
        process.destroy()
        if (process.isAlive) process.destroyForcibly()
        return true
    }
}

internal fun buildYtDlpProcessArguments(
    request: YtDlpCommand,
    ffmpegPath: String,
    quickJsPath: String,
): List<String> = buildList {
    add("--ignore-config")
    add("--no-plugin-dirs")
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
