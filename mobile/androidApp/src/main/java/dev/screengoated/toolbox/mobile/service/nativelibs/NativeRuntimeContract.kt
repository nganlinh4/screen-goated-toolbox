package dev.screengoated.toolbox.mobile.service.nativelibs

import android.content.Context
import org.json.JSONObject

internal data class NativeRuntimeEntry(
    val fileName: String,
    val byteCount: Long,
    val sha256: String,
)

internal data class NativeRuntimeArchive(
    val engine: String,
    val fileName: String,
    val byteCount: Long,
    val sha256: String,
    val fullDelivery: String,
    val entries: List<NativeRuntimeEntry>,
)

internal data class NativeRuntimeManifest(
    val abi: String,
    val archives: List<NativeRuntimeArchive>,
) {
    fun archive(engine: String): NativeRuntimeArchive =
        archives.singleOrNull { it.engine == engine }
            ?: error("Native runtime contract has no unique engine '$engine'")
}

internal object NativeRuntimeContract {
    const val ASSET_PATH = "native-runtime/contract.json"
    const val FULL_ORT_ASSET_PATH = "native-runtime/ort-runtime.zip"

    fun load(context: Context): NativeRuntimeManifest =
        context.assets.open(ASSET_PATH).bufferedReader(Charsets.UTF_8).use { reader ->
            parse(reader.readText())
        }

    fun parse(json: String): NativeRuntimeManifest {
        val root = JSONObject(json)
        requireFields(root, setOf("schemaVersion", "abi", "archives"), "contract")
        require(root.getInt("schemaVersion") == 1) { "Unsupported native runtime schema" }
        val abi = root.getString("abi")
        require(abi == "arm64-v8a") { "Unsupported native runtime ABI: $abi" }
        val rawArchives = root.getJSONArray("archives")
        val archives = buildList {
            repeat(rawArchives.length()) { index ->
                val raw = rawArchives.getJSONObject(index)
                requireFields(
                    raw,
                    setOf(
                        "engine",
                        "fileName",
                        "byteCount",
                        "sha256",
                        "fullDelivery",
                        "entries",
                    ),
                    "archive[$index]",
                )
                val rawEntries = raw.getJSONArray("entries")
                val entries = buildList {
                    repeat(rawEntries.length()) { entryIndex ->
                        val entry = rawEntries.getJSONObject(entryIndex)
                        requireFields(
                            entry,
                            setOf("fileName", "byteCount", "sha256"),
                            "archive[$index].entries[$entryIndex]",
                        )
                        add(
                            NativeRuntimeEntry(
                                fileName = entry.getString("fileName").also(::requireFlatLibraryName),
                                byteCount = entry.getLong("byteCount").also(::requirePositive),
                                sha256 = entry.getString("sha256").also(::requireSha256),
                            ),
                        )
                    }
                }
                require(entries.isNotEmpty()) { "Native archive entries cannot be empty" }
                require(entries.map { it.fileName }.distinct().size == entries.size) {
                    "Native archive contract contains duplicate members"
                }
                val engine = raw.getString("engine")
                require(engine.matches(Regex("[a-z][a-z0-9_-]*"))) {
                    "Invalid native runtime engine: $engine"
                }
                val fileName = raw.getString("fileName")
                requireFlatArchiveName(fileName)
                val delivery = raw.getString("fullDelivery")
                require(delivery in setOf("bundled_asset", "verified_download")) {
                    "Invalid Full native delivery: $delivery"
                }
                add(
                    NativeRuntimeArchive(
                        engine = engine,
                        fileName = fileName,
                        byteCount = raw.getLong("byteCount").also(::requirePositive),
                        sha256 = raw.getString("sha256").also(::requireSha256),
                        fullDelivery = delivery,
                        entries = entries,
                    ),
                )
            }
        }
        require(archives.map { it.engine }.distinct().size == archives.size) {
            "Native runtime contract contains duplicate engines"
        }
        require(archives.map { it.fileName }.distinct().size == archives.size) {
            "Native runtime contract contains duplicate archives"
        }
        require(archives.map { it.engine }.toSet() == setOf("ort", "moonshine", "sherpa")) {
            "Native runtime contract has an unexpected engine set"
        }
        require(archives.single { it.engine == "ort" }.fullDelivery == "bundled_asset") {
            "Full ORT must use the bundled archive"
        }
        return NativeRuntimeManifest(abi = abi, archives = archives)
    }
}

private fun requireFields(value: JSONObject, expected: Set<String>, label: String) {
    val actual = value.keys().asSequence().toSet()
    require(actual == expected) { "$label fields differ: expected=$expected actual=$actual" }
}

private fun requireFlatArchiveName(value: String) {
    require(
        value.isNotBlank() &&
            !value.contains('/') &&
            !value.contains('\\') &&
            value !in setOf(".", "..") &&
            value.endsWith("-runtime.zip"),
    ) { "Native archive name must be flat: $value" }
}

internal fun requireFlatLibraryName(value: String) {
    require(
        value.isNotBlank() &&
            !value.contains('/') &&
            !value.contains('\\') &&
            value !in setOf(".", "..") &&
            value.startsWith("lib") &&
            value.endsWith(".so"),
    ) { "Native library name must be flat: $value" }
}

private fun requirePositive(value: Long) {
    require(value > 0L) { "Native runtime byte count must be positive" }
}

private fun requireSha256(value: String) {
    require(value.matches(Regex("[0-9a-f]{64}"))) { "Invalid native runtime SHA-256" }
}
