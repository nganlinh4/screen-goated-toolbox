package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityCommandDispatchLease

internal data class PrivilegedCommandProbe(
    val state: CapabilityState,
    val requiredUserStep: String? = null,
)

internal interface PrivilegedCommandProvider {
    val providerId: String

    fun probe(context: Context): PrivilegedCommandProbe

    suspend fun executeAuthorized(
        context: Context,
        lease: AccessibilityCommandDispatchLease,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult
}

internal object PrivilegedCommandProviderRegistry {
    private val providers: List<PrivilegedCommandProvider> = listOf(
        SgtAdbPrivilegedCommandProvider,
        ShizukuPrivilegedCommandProvider,
        RootPrivilegedCommandProvider,
    )
    private val providersById = providers.associateBy(PrivilegedCommandProvider::providerId)

    fun find(providerId: String): PrivilegedCommandProvider? = providersById[providerId]

    fun ordered(providerIds: List<String>): List<PrivilegedCommandProvider> =
        providerIds.mapNotNull(providersById::get)
}

private object SgtAdbPrivilegedCommandProvider : PrivilegedCommandProvider {
    override val providerId: String = "sgt_adb_bridge"

    override fun probe(context: Context): PrivilegedCommandProbe =
        SgtAdbCommandBridge.probe(context).let {
            PrivilegedCommandProbe(it.state, it.requiredUserStep)
        }

    override suspend fun executeAuthorized(
        context: Context,
        lease: AccessibilityCommandDispatchLease,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult = SgtAdbCommandBridge.executeAuthorized(
        context,
        lease,
        effectOwner,
        program,
        args,
        cwd,
        timeoutMs,
    )
}

private object ShizukuPrivilegedCommandProvider : PrivilegedCommandProvider {
    override val providerId: String = "shizuku_shell"

    override fun probe(context: Context): PrivilegedCommandProbe =
        ShizukuCommandBridge.probe(context).let {
            PrivilegedCommandProbe(it.state, it.requiredUserStep)
        }

    override suspend fun executeAuthorized(
        context: Context,
        lease: AccessibilityCommandDispatchLease,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult = ShizukuCommandBridge.executeAuthorized(
        context,
        lease,
        effectOwner,
        program,
        args,
        cwd,
        timeoutMs,
    )
}

private object RootPrivilegedCommandProvider : PrivilegedCommandProvider {
    override val providerId: String = "root_bridge"

    override fun probe(context: Context): PrivilegedCommandProbe =
        RootCommandBridge.probe().let {
            PrivilegedCommandProbe(it.state, it.requiredUserStep)
        }

    override suspend fun executeAuthorized(
        context: Context,
        lease: AccessibilityCommandDispatchLease,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult = RootCommandBridge.executeAuthorized(
        lease,
        effectOwner,
        program,
        args,
        cwd,
        timeoutMs,
    )
}
