package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put

/** The exact same bounded process description feeds execution and assessment. */
internal data class CommandRequest(val program: String, val argv: List<String>, val cwd: String) {
    fun proposal(): JsonObject = buildJsonObject {
        put("program", program)
        put("args", buildJsonArray { argv.forEach { add(JsonPrimitive(it)) } })
        put("cwd", cwd)
        put("inherited_environment", "unknown")
    }
}

internal fun parseCommandRequest(args: JsonObject): CommandRequest {
    val program = args.string("program")
    val command = args.string("command")
    if (program != null && command != null) {
        throw IllegalArgumentException("supply program or command, not both")
    }
    if (command != null && (args["args"] != null || args["cwd"] != null)) {
        throw IllegalArgumentException("args and cwd require exact program mode")
    }
    val exactProgram = if (command != null) SHELL_PROGRAM else program
        ?.takeIf(String::isNotBlank)
        ?: throw IllegalArgumentException("run_command requires program or command")
    if (exactProgram.utf8Size() > MAX_PROGRAM_BYTES || '\u0000' in exactProgram) {
        throw IllegalArgumentException("program exceeds its bounded exact-argv contract")
    }
    val argv = if (command != null) {
        val maxShellCommandBytes = MAX_TOTAL_ARG_BYTES - SHELL_COMMAND_FLAG.utf8Size()
        if (command.isBlank() ||
            command.utf8Size() > maxShellCommandBytes ||
            '\u0000' in command
        ) {
            throw IllegalArgumentException("command exceeds its bounded shell contract")
        }
        listOf(SHELL_COMMAND_FLAG, command)
    } else {
        args.stringList("args")
            ?: throw IllegalArgumentException("args must be an array of at most $MAX_ARGS strings")
    }
    if (argv.size > MAX_ARGS || argv.any { it.utf8Size() > MAX_ARG_BYTES || '\u0000' in it }) {
        throw IllegalArgumentException("args exceed the bounded exact-argv contract")
    }
    if (argv.sumOf { it.utf8Size() } > MAX_TOTAL_ARG_BYTES) {
        throw IllegalArgumentException("process arguments exceed $MAX_TOTAL_ARG_BYTES bytes")
    }
    val cwd = args.string("cwd") ?: DEFAULT_COMMAND_CWD
    if (!cwd.startsWith('/') || cwd.utf8Size() > MAX_CWD_BYTES || '\u0000' in cwd) {
        throw IllegalArgumentException("cwd must be an absolute bounded directory path")
    }

    return CommandRequest(exactProgram, argv, cwd)
}

private fun JsonObject.stringList(name: String): List<String>? {
    val value = get(name) ?: return emptyList()
    val array = value as? JsonArray ?: return null
    if (array.size > MAX_ARGS) return null
    return array.map { element ->
        (element as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull ?: return null
    }
}

private fun String.utf8Size(): Int = toByteArray(Charsets.UTF_8).size

private const val DEFAULT_COMMAND_CWD = "/data/local/tmp"
private const val SHELL_PROGRAM = "/system/bin/sh"
private const val SHELL_COMMAND_FLAG = "-c"
private const val MAX_ARGS = 16
private const val MAX_PROGRAM_BYTES = 1_024
private const val MAX_ARG_BYTES = 4_096
private const val MAX_TOTAL_ARG_BYTES = 16_384
private const val MAX_CWD_BYTES = 4_096
