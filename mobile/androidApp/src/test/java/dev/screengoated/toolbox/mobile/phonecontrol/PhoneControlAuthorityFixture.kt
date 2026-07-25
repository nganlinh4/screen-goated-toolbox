package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityRoute
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderDefinition
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal data class PhoneControlAuthorityFixtureData(
    val root: JsonObject,
    val capabilityStates: List<String>,
    val providers: List<ProviderDefinition>,
    val routes: List<CapabilityRoute>,
)

internal object PhoneControlAuthorityFixture {
    private const val FIXTURE_PATH = "parity-fixtures/phone-control/authority-matrix.json"

    fun load(): PhoneControlAuthorityFixtureData {
        val root = Json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject
        val providers = root.getValue("providers").jsonArray.map { element ->
            val provider = element.jsonObject
            ProviderDefinition(
                id = provider.getValue("id").jsonPrimitive.content,
                authority = provider.getValue("authority").jsonPrimitive.content,
                optional = provider.getValue("optional").jsonPrimitive.boolean,
            )
        }
        val routes = root.getValue("routes").jsonArray.map { element ->
            val route = element.jsonObject
            CapabilityRoute(
                capability = route.getValue("capability").jsonPrimitive.content,
                providerIds = route.getValue("providers").jsonArray.map {
                    it.jsonPrimitive.content
                },
            )
        }
        return PhoneControlAuthorityFixtureData(
            root = root,
            capabilityStates = root.getValue("capabilityStates").jsonArray.map {
                it.jsonPrimitive.content
            },
            providers = providers,
            routes = routes,
        )
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }
}
