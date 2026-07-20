package dev.screengoated.toolbox.mobile.phonecontrol.capability

import android.app.admin.DevicePolicyManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Process
import android.os.SystemClock
import android.provider.Settings
import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorModelManager
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorReadiness
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.RootCommandBridge
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal data class PhoneControlAuthorityCatalog(
    val providers: List<ProviderDefinition>,
    val routes: List<CapabilityRoute>,
) {
    val capabilitiesByProvider: Map<String, Set<String>> = buildMap {
        routes.forEach { route ->
            route.providerIds.forEach { providerId ->
                put(providerId, get(providerId).orEmpty() + route.capability)
            }
        }
    }
}

internal data class PhoneControlProviderEvidence(
    val catalog: PhoneControlAuthorityCatalog,
    val snapshots: List<ProviderSnapshot>,
) {
    fun modelContext(): String = buildString {
        append("\nANDROID CAPABILITY SNAPSHOT\n")
        snapshots.forEach { snapshot ->
            append("- ")
            append(snapshot.providerId)
            append(": ")
            append(snapshot.state.wireName)
            snapshot.requiredUserStep?.let { append("; user_step=").append(it) }
            append('\n')
        }
    }
}

internal object PhoneControlProviderRegistry {
    private val json = Json { ignoreUnknownKeys = true }

    fun probe(context: Context): PhoneControlProviderEvidence {
        val catalog = catalog(context)
        val timestamp = SystemClock.elapsedRealtime()
        val snapshots = catalog.providers.map { provider ->
            val probe = probeProvider(context, provider.id)
            val capabilities = catalog.capabilitiesByProvider[provider.id].orEmpty()
                .associateWith { emptySet<String>() }
            ProviderSnapshot(
                providerId = provider.id,
                state = probe.state,
                supportedCapabilities = capabilities,
                evidenceTimestampMs = timestamp,
                requiredUserStep = probe.requiredUserStep,
            )
        }
        return PhoneControlProviderEvidence(catalog, snapshots)
    }

    fun router(context: Context): ProviderRouter {
        val catalog = catalog(context)
        return ProviderRouter(catalog.providers, catalog.routes)
    }

    private fun catalog(context: Context): PhoneControlAuthorityCatalog {
        val root = context.assets.open(GeneratedPhoneControlContract.AUTHORITY_MATRIX_ASSET_PATH)
            .bufferedReader()
            .use { json.parseToJsonElement(it.readText()).jsonObject }
        val providers = root.getValue("providers").jsonArray.map { element ->
            val provider = element.jsonObject
            ProviderDefinition(
                id = provider.getValue("id").jsonPrimitive.content,
                authority = provider.getValue("authority").jsonPrimitive.content,
                optional = provider["optional"]?.jsonPrimitive?.booleanOrNull ?: true,
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
        return PhoneControlAuthorityCatalog(providers, routes)
    }

    private fun probeProvider(context: Context, id: String): Probe = when (id) {
        "android_app_api" -> Probe(CapabilityState.READY)
        "accessibility" -> probeAccessibility(context)
        "accessibility_input_method" -> when {
            Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ->
                Probe(CapabilityState.UNSUPPORTED, "Android 13 or newer is required.")
            SgtAccessibilityService.isAvailable -> Probe(CapabilityState.READY)
            else -> probeAccessibility(context)
        }
        "media_projection" -> Probe(
            CapabilityState.NEEDS_USER_STEP,
            "Approve the Android screen-capture prompt for this session.",
        )
        "notification_listener" -> Probe(
            CapabilityState.UNAVAILABLE,
            "Notification access is not enabled for Phone Control.",
        )
        "custom_tabs_session" -> probeCustomTabs(context)
        "owned_webview_bridge" -> Probe(
            CapabilityState.UNAVAILABLE,
            "No SGT-owned browser surface is active.",
        )
        "browser_cdp" -> Probe(
            CapabilityState.UNAVAILABLE,
            "Connect a verified browser debugging provider.",
        )
        "local_ui_detector" -> probeLocalDetector(context)
        "shizuku_shell" -> ShizukuCommandBridge.probe(context).let { probe ->
            Probe(probe.state, probe.requiredUserStep)
        }
        "root_bridge" -> RootCommandBridge.probe().let { probe ->
            Probe(probe.state, probe.requiredUserStep)
        }
        "device_owner" -> probeDeviceOwner(context)
        "privileged_system" -> if (Process.myUid() == Process.SYSTEM_UID) {
            Probe(CapabilityState.READY)
        } else {
            Probe(CapabilityState.UNSUPPORTED, "Install the separate privileged-system build.")
        }
        else -> Probe(CapabilityState.UNAVAILABLE, "No provider probe is installed.")
    }

    private fun probeAccessibility(context: Context): Probe = when {
        SgtAccessibilityService.isAvailable -> Probe(CapabilityState.READY)
        isAccessibilityEnabled(context) -> Probe(
            CapabilityState.DEGRADED,
            "Wait for the Accessibility service to reconnect or toggle it off and on.",
        )
        else -> Probe(
            CapabilityState.NEEDS_USER_STEP,
            "Enable SGT in Android Accessibility settings.",
        )
    }

    private fun probeCustomTabs(context: Context): Probe {
        val services = context.packageManager.queryIntentServices(
            Intent(CUSTOM_TABS_SERVICE_ACTION),
            PackageManager.MATCH_ALL,
        )
        return if (services.isNotEmpty()) {
            Probe(CapabilityState.READY)
        } else {
            Probe(CapabilityState.UNAVAILABLE, "Install a browser with Custom Tabs support.")
        }
    }

    private fun probeDeviceOwner(context: Context): Probe {
        val manager = context.getSystemService(DevicePolicyManager::class.java)
        return if (manager?.isDeviceOwnerApp(context.packageName) == true) {
            Probe(CapabilityState.READY)
        } else {
            Probe(CapabilityState.UNSUPPORTED, "This device is not provisioned with SGT as owner.")
        }
    }

    private fun probeLocalDetector(context: Context): Probe =
        when (val readiness = UiDetectorModelManager.get(context).readiness()) {
            UiDetectorReadiness.Ready -> Probe(CapabilityState.READY)
            is UiDetectorReadiness.Downloading -> Probe(CapabilityState.DEGRADED, readiness.message)
            is UiDetectorReadiness.Missing -> Probe(CapabilityState.UNAVAILABLE, readiness.message)
            is UiDetectorReadiness.Failed -> Probe(CapabilityState.DEGRADED, readiness.message)
        }

    private fun isAccessibilityEnabled(context: Context): Boolean {
        val expected = "${context.packageName}/${SgtAccessibilityService::class.java.name}"
        val enabled = Settings.Secure.getString(
            context.contentResolver,
            Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
        ).orEmpty()
        return enabled.split(':').any { it.equals(expected, ignoreCase = true) }
    }

    private data class Probe(
        val state: CapabilityState,
        val requiredUserStep: String? = null,
    )

    private const val CUSTOM_TABS_SERVICE_ACTION =
        "android.support.customtabs.action.CustomTabsService"
}
