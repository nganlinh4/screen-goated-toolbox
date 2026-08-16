package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import com.google.android.play.core.splitcompat.SplitCompat
import com.google.android.play.core.splitinstall.SplitInstallManagerFactory
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeFactory
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeManager
import java.io.File
import java.security.MessageDigest
import java.util.zip.ZipFile
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame

internal data class AcceptedCreationRequestEvidence(
    val fingerprint: String,
    val generationMode: String?,
    val referenceSha256: List<String>,
    val frozenAndValidated: Boolean,
)

internal data class CreationRuntimeAcceptanceEvidence(
    val distribution: String,
    val channel: String,
    val contractSha256: String,
    val runtimeArtifactSha256: String,
    val runtimeFactoryClass: String,
    val runtimeVersion: String,
    val runtimeManifestSha256: String,
    val runtimeSplitName: String?,
    val mailboxPollIntervalMs: Long,
)

internal object CreationAcceptanceAttestation {
    fun acceptedRequest(
        context: Context,
        record: CreationJournalRecord,
        expectedGenerationMode: CreationGenerationMode?,
        expectedReferenceSha256: List<String>,
    ): AcceptedCreationRequestEvidence {
        val request = record.request
        assertEquals(
            "Accepted request changed the selected generation mode",
            expectedGenerationMode?.wireName,
            request.generationMode,
        )
        assertEquals(
            "Accepted request changed its reference count",
            expectedReferenceSha256.size,
            request.sourceDescriptors.size,
        )
        check(creationRequestHasValidDeliveryIdentity(request)) {
            "Accepted request has an invalid fingerprint or descriptor binding"
        }
        val files = CreationFileStore(context)
        check(files.restoredRequestIsValid(request)) {
            "Accepted request is not a restorable immutable native request"
        }
        val sourcePaths = request.imagePaths.ifEmpty {
            request.imagePath.takeIf(String::isNotBlank)?.let(::listOf).orEmpty()
        }
        assertEquals(
            "Accepted request paths differ from their descriptors",
            request.sourceDescriptors.map(CreationSourceDescriptor::path),
            sourcePaths,
        )
        val actualHashes = request.sourceDescriptors.map { descriptor ->
            check(isManagedCreationJobInput(context.filesDir, request.jobId, descriptor.path)) {
                "Accepted reference is not frozen in the job-input store"
            }
            val source = File(descriptor.path)
            check(source.isFile && source.length() == descriptor.sizeBytes) {
                "Accepted reference bytes changed after freezing"
            }
            sha256(source).also { digest ->
                assertEquals("Accepted reference SHA-256 changed", descriptor.sha256, digest)
            }
        }
        assertEquals(
            "Accepted references differ from the selected source bytes",
            expectedReferenceSha256,
            actualHashes,
        )
        return AcceptedCreationRequestEvidence(
            fingerprint = request.requestFingerprint,
            generationMode = request.generationMode,
            referenceSha256 = actualHashes,
            frozenAndValidated = true,
        )
    }

    fun selectedRuntime(context: Context): CreationRuntimeAcceptanceEvidence {
        val arguments = androidx.test.platform.app.InstrumentationRegistry.getArguments()
        val distribution = requireNotNull(arguments.getString("sgtCreationDistribution"))
        val expectedChannel = requireNotNull(arguments.getString("sgtCreationDeliveryChannel"))
        val expectedContractSha256 =
            requireNotNull(arguments.getString("sgtCreationContractSha256"))
        val expectedArtifactSha256 =
            requireNotNull(arguments.getString("sgtCreationArtifactSha256"))
        check(distribution in setOf("full", "play")) { "Unknown acceptance distribution" }
        check(expectedChannel in setOf("production", "staging")) {
            "Unknown acceptance delivery channel"
        }
        val bytes = packagedDeliveryContract(context)
        assertEquals("Packaged creation contract changed", expectedContractSha256, sha256(bytes))
        val root = JSONObject(bytes.toString(Charsets.UTF_8))
        val android = root.getJSONObject("android")
        val record = android.getJSONObject(distribution)
        assertEquals(
            "Packaged runtime artifact changed",
            expectedArtifactSha256,
            record.getString("sha256"),
        )
        val tag = if (expectedChannel == "staging") {
            "sgt-runtime-staging"
        } else {
            "sgt-runtime-bundles"
        }
        check(record.getString("downloadUrl").contains("/download/$tag/")) {
            "Packaged runtime URL changed delivery channels"
        }
        val expectedFactoryClass = android.getString("factoryClass")
        val factory = if (distribution == "play") {
            playRuntimeFactory(context, expectedFactoryClass)
        } else {
            requireNotNull(CreationRuntimeManager.get(context).factory()) {
                "Selected creation runtime factory is unavailable after generation"
            }
        }
        assertEquals(
            "Loaded runtime factory class changed",
            expectedFactoryClass,
            factory.javaClass.name,
        )
        val manifestBytes = factory.runtimeManifest().encodeToByteArray()
        val manifest = JSONObject(manifestBytes.toString(Charsets.UTF_8))
        assertEquals(
            "Loaded runtime version changed",
            root.getString("version"),
            manifest.getString("runtimeVersion"),
        )
        val expectedFeatures = root.getJSONArray("features").stringSet()
        assertEquals(
            "Loaded runtime capabilities changed",
            expectedFeatures,
            manifest.getJSONArray("features").stringSet(),
        )
        val splitName = if (distribution == "play") {
            assertPlayRuntime(context, expectedFactoryClass, factory.javaClass)
        } else {
            assertInstalledFullRuntime(context, root)
            null
        }
        return CreationRuntimeAcceptanceEvidence(
            distribution = distribution,
            channel = expectedChannel,
            contractSha256 = expectedContractSha256,
            runtimeArtifactSha256 = expectedArtifactSha256,
            runtimeFactoryClass = factory.javaClass.name,
            runtimeVersion = manifest.getString("runtimeVersion"),
            runtimeManifestSha256 = sha256(manifestBytes),
            runtimeSplitName = splitName,
            mailboxPollIntervalMs = REQUIRED_MAILBOX_POLL_INTERVAL_MS,
        )
    }

    private fun assertPlayRuntime(
        context: Context,
        expectedFactoryClass: String,
        loadedFactoryClass: Class<*>,
    ): String {
        check(!File(context.filesDir, "creation/runtime").exists()) {
            "Play acceptance retained Full runtime bytes"
        }
        val installedModules = SplitInstallManagerFactory.create(context.applicationContext)
            .installedModules
        check(PLAY_RUNTIME_SPLIT in installedModules) {
            "Play acceptance did not install the selected runtime module"
        }
        check(SplitCompat.install(context)) {
            "Play acceptance could not activate the selected runtime module"
        }
        assertSame(
            "Play module does not own the loaded runtime factory",
            loadedFactoryClass,
            context.classLoader.loadClass(expectedFactoryClass),
        )
        return PLAY_RUNTIME_SPLIT
    }

    private fun playRuntimeFactory(
        context: Context,
        expectedFactoryClass: String,
    ): CreationRuntimeFactory {
        check(SplitCompat.install(context)) {
            "Play acceptance could not activate the selected runtime module"
        }
        val type = context.classLoader.loadClass(expectedFactoryClass)
        check(CreationRuntimeFactory::class.java.isAssignableFrom(type)) {
            "Play runtime factory does not implement the creation contract"
        }
        return type.getDeclaredConstructor().newInstance() as CreationRuntimeFactory
    }

    private fun assertInstalledFullRuntime(context: Context, contract: JSONObject) {
        val version = contract.getString("version")
        val entries = contract.getJSONObject("android").getJSONArray("entries")
        val runtimeRoot = File(context.filesDir, "creation/runtime/$version").canonicalFile
        repeat(entries.length()) { index ->
            val entry = entries.getJSONObject(index)
            val installed = File(runtimeRoot, entry.getString("installPath")).canonicalFile
            check(installed.toPath().startsWith(runtimeRoot.toPath()) && installed.isFile) {
                "Installed Full runtime entry is unavailable"
            }
            assertEquals(
                "Installed Full runtime entry size changed",
                entry.getLong("sizeBytes"),
                installed.length(),
            )
            assertEquals(
                "Installed Full runtime entry SHA-256 changed",
                entry.getString("sha256"),
                sha256(installed),
            )
        }
    }

    private fun packagedDeliveryContract(context: Context): ByteArray {
        val contexts = buildList {
            add(context)
            context.applicationInfo.splitNames.orEmpty().forEach { split ->
                runCatching { context.createContextForSplit(split) }.getOrNull()?.let(::add)
            }
        }
        return contexts.firstNotNullOfOrNull { candidate ->
            runCatching {
                candidate.assets.open("creation-runtime/delivery.json").use { it.readBytes() }
            }.getOrNull()
        } ?: installedPackageDeliveryContract(context)
        ?: splitCompatDeliveryContract(context)
        ?: error("Installed acceptance app has no creation delivery contract")
    }

    private fun installedPackageDeliveryContract(context: Context): ByteArray? {
        val application = context.packageManager.getApplicationInfo(context.packageName, 0)
        val packagePaths = listOfNotNull(application.sourceDir) +
            application.splitSourceDirs.orEmpty()
        return packagePaths.firstNotNullOfOrNull { path ->
            runCatching {
                ZipFile(path).use { archive ->
                    val entry = archive.getEntry("assets/creation-runtime/delivery.json")
                        ?: return@use null
                    archive.getInputStream(entry).use { it.readBytes() }
                }
            }.getOrNull()
        }
    }

    private fun splitCompatDeliveryContract(context: Context): ByteArray? {
        val versionCode = context.packageManager
            .getPackageInfo(context.packageName, 0)
            .longVersionCode
        val splitRoot = File(
            context.filesDir,
            "splitcompat/$versionCode/verified-splits",
        ).canonicalFile
        val runtimeSplit = File(splitRoot, "$PLAY_RUNTIME_SPLIT.apk").canonicalFile
        if (!runtimeSplit.toPath().startsWith(splitRoot.toPath()) || !runtimeSplit.isFile) {
            return null
        }
        return runCatching {
            ZipFile(runtimeSplit).use { archive ->
                val entry = archive.getEntry("assets/creation-runtime/delivery.json")
                    ?: return@use null
                archive.getInputStream(entry).use { it.readBytes() }
            }
        }.getOrNull()
    }

    private fun org.json.JSONArray.stringSet(): Set<String> = buildSet {
        repeat(length()) { index -> add(getString(index)) }
    }

    private const val PLAY_RUNTIME_SPLIT = "feature_creation_runtime"
    private const val REQUIRED_MAILBOX_POLL_INTERVAL_MS = 1_000L
}

internal fun sha256(file: File): String = file.inputStream().use { input ->
    val digest = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
    while (true) {
        val read = input.read(buffer)
        if (read < 0) break
        digest.update(buffer, 0, read)
    }
    digest.digest().joinToString("") { "%02x".format(it) }
}

internal fun sha256(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(bytes)
    .joinToString("") { "%02x".format(it) }
