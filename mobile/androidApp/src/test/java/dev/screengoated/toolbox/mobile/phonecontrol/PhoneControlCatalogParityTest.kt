package dev.screengoated.toolbox.mobile.phonecontrol

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.security.MessageDigest

/** Runs for both distribution flavors because the generated contract is shared main input. */
class PhoneControlCatalogParityTest {
    @Test
    fun `generated full catalog matches the Windows canonical catalog`() {
        val canonical = Json.parseToJsonElement(Files.readAllBytes(canonicalPath()).decodeToString())
        val generated = Json.parseToJsonElement(Files.readAllBytes(generatedAssetPath()).decodeToString())
        val declarations = canonical.jsonObject
            .getValue("functionDeclarations")
            .jsonArray
        val names = declarations.map { declaration ->
            declaration.jsonObject.getValue("name").jsonPrimitive.content
        }

        assertEquals(canonical, generated)
        assertEquals(
            GeneratedPhoneControlContract.SCHEMA_VERSION,
            canonical.jsonObject.getValue("schemaVersion").jsonPrimitive.content.toInt(),
        )
        assertEquals(GeneratedPhoneControlContract.STATIC_DECLARATION_COUNT, declarations.size)
        assertEquals(names.size, names.toSet().size)
        assertEquals(
            GeneratedPhoneControlContract.CATALOG_SHA256,
            sha256(canonical.toString()),
        )
        assertEquals(
            "phone_control/catalog.json",
            GeneratedPhoneControlContract.CATALOG_ASSET_PATH,
        )
    }

    @Test
    fun `generated full prompt keeps one platform substitution boundary`() {
        val canonical = Files.readAllBytes(canonicalPromptPath()).decodeToString()
        val generated = Files.readAllBytes(generatedPromptPath()).decodeToString()
        val token = GeneratedPhoneControlContract.PLATFORM_DEVICE_TOKEN

        assertEquals(canonical, generated)
        assertEquals("{{PLATFORM_DEVICE}}", token)
        assertEquals(1, canonical.split(token).size - 1)
        assertEquals(
            GeneratedPhoneControlContract.PROMPT_CORE_SHA256,
            sha256(canonical),
        )
        assertEquals(
            "phone_control/prompt_core.txt",
            GeneratedPhoneControlContract.PROMPT_CORE_ASSET_PATH,
        )
    }

    @Test
    fun `generated full authority matrix matches the shared parity fixture`() {
        val canonical = Files.readAllBytes(canonicalAuthorityPath()).decodeToString()
        val generated = Files.readAllBytes(generatedAuthorityPath()).decodeToString()

        assertEquals(
            Json.parseToJsonElement(canonical),
            Json.parseToJsonElement(generated),
        )
        assertEquals(
            "phone_control/authority-matrix.json",
            GeneratedPhoneControlContract.AUTHORITY_MATRIX_ASSET_PATH,
        )
    }

    @Test
    fun `generated Android orb is the Windows canonical renderer`() {
        val canonical = Files.readAllBytes(canonicalOrbPath()).decodeToString()
        val generated = Files.readAllBytes(generatedOrbPath()).decodeToString()
        val canonicalContract = Json.parseToJsonElement(
            Files.readAllBytes(canonicalOrbContractPath()).decodeToString(),
        )
        val generatedContract = Json.parseToJsonElement(
            Files.readAllBytes(generatedOrbContractPath()).decodeToString(),
        )

        assertEquals(canonical, generated)
        assertEquals(canonicalContract, generatedContract)
        assertEquals(GeneratedPhoneControlContract.ORB_SHA256, sha256(canonical))
        assertEquals("phone_control/orb.html", GeneratedPhoneControlContract.ORB_ASSET_PATH)
        assertEquals(
            "phone_control/orb-contract.json",
            GeneratedPhoneControlContract.ORB_CONTRACT_ASSET_PATH,
        )
    }

    private fun canonicalPath(): Path = findFile(
        "canonical Phone Control catalog",
        Paths.get("..", "src", "overlay", "computer_control", "phone_control_catalog.json"),
        Paths.get("..", "..", "src", "overlay", "computer_control", "phone_control_catalog.json"),
        Paths.get("src", "overlay", "computer_control", "phone_control_catalog.json"),
    )

    private fun generatedAssetPath(): Path = findFile(
        "generated Phone Control asset",
        Paths.get("androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "catalog.json"),
        Paths.get("build", "generated", "phoneControlContract", "assets", "phone_control", "catalog.json"),
        Paths.get("mobile", "androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "catalog.json"),
    )

    private fun canonicalPromptPath(): Path = findFile(
        "canonical Phone Control prompt core",
        Paths.get("..", "src", "overlay", "computer_control", "uia_task", "prompt_core.txt"),
        Paths.get("..", "..", "src", "overlay", "computer_control", "uia_task", "prompt_core.txt"),
        Paths.get("src", "overlay", "computer_control", "uia_task", "prompt_core.txt"),
    )

    private fun generatedPromptPath(): Path = findFile(
        "generated Phone Control prompt core",
        Paths.get("androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "prompt_core.txt"),
        Paths.get("build", "generated", "phoneControlContract", "assets", "phone_control", "prompt_core.txt"),
        Paths.get("mobile", "androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "prompt_core.txt"),
    )

    private fun canonicalAuthorityPath(): Path = findFile(
        "canonical Phone Control authority matrix",
        Paths.get("..", "parity-fixtures", "phone-control", "authority-matrix.json"),
        Paths.get("..", "..", "parity-fixtures", "phone-control", "authority-matrix.json"),
        Paths.get("parity-fixtures", "phone-control", "authority-matrix.json"),
    )

    private fun generatedAuthorityPath(): Path = findFile(
        "generated Phone Control authority matrix",
        Paths.get("androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "authority-matrix.json"),
        Paths.get("build", "generated", "phoneControlContract", "assets", "phone_control", "authority-matrix.json"),
        Paths.get("mobile", "androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "authority-matrix.json"),
    )

    private fun canonicalOrbPath(): Path = findFile(
        "canonical Computer Control orb",
        Paths.get("..", "src", "overlay", "computer_control", "orb", "orb.html"),
        Paths.get("..", "..", "src", "overlay", "computer_control", "orb", "orb.html"),
        Paths.get("src", "overlay", "computer_control", "orb", "orb.html"),
    )

    private fun generatedOrbPath(): Path = findFile(
        "generated Phone Control orb",
        Paths.get("androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "orb.html"),
        Paths.get("build", "generated", "phoneControlContract", "assets", "phone_control", "orb.html"),
        Paths.get("mobile", "androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "orb.html"),
    )

    private fun canonicalOrbContractPath(): Path = findFile(
        "canonical Phone Control orb contract",
        Paths.get("..", "parity-fixtures", "phone-control", "orb-contract.json"),
        Paths.get("..", "..", "parity-fixtures", "phone-control", "orb-contract.json"),
        Paths.get("parity-fixtures", "phone-control", "orb-contract.json"),
    )

    private fun generatedOrbContractPath(): Path = findFile(
        "generated Phone Control orb contract",
        Paths.get("androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "orb-contract.json"),
        Paths.get("build", "generated", "phoneControlContract", "assets", "phone_control", "orb-contract.json"),
        Paths.get("mobile", "androidApp", "build", "generated", "phoneControlContract", "assets", "phone_control", "orb-contract.json"),
    )

    private fun findFile(label: String, vararg candidates: Path): Path =
        candidates.firstOrNull(Files::exists)
            ?: error("Missing $label. Tried: ${candidates.toList()}")

    private fun sha256(value: String): String =
        MessageDigest.getInstance("SHA-256")
            .digest(value.encodeToByteArray())
            .joinToString("") { byte -> "%02x".format(byte) }
}
