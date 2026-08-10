package dev.screengoated.toolbox.mobile.componentupdate

import android.content.Context
import dev.screengoated.toolbox.mobile.BuildConfig
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONObject

internal data class ComponentUpdatePolicy(
    val mode: String,
    val checkHours: Long,
    val group: String,
)

internal data class ComponentCatalogCandidate(
    val name: String,
    val catalog: ByteArray,
    val signature: ByteArray,
    val verified: VerifiedComponentCatalog,
)

internal object ComponentUpdateCatalog {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val initialized = AtomicBoolean(false)
    private val lock = Any()
    @Volatile private var active: VerifiedComponentCatalog? = null

    fun loadCached(context: Context) {
        if (!initialized.compareAndSet(false, true)) return
        val application = context.applicationContext
        runCatching { ComponentUpdateCache.loadHighest(application) }
            .getOrNull()
            ?.let(::activate)
    }

    fun refreshInBackground(context: Context) {
        loadCached(context)
        val application = context.applicationContext
        scope.launch {
            runCatching { refreshNow(application) }
                .onFailure { android.util.Log.i("SGT-Components", "Catalog refresh skipped", it) }
        }
    }

    fun refreshNow(context: Context): Boolean = synchronized(lock) {
        val candidate = ComponentUpdateNetwork.fetchHighest(
            minimumSequence = active?.sequence ?: 0L,
            hostVersion = BuildConfig.VERSION_NAME,
        ) ?: return@synchronized false
        ComponentUpdateCache.store(
            context.applicationContext,
            candidate.name,
            candidate.catalog,
            candidate.signature,
        )
        activate(candidate.verified)
        true
    }

    fun contract(name: String, platforms: Set<String>): JSONObject? {
        val catalog = active ?: return null
        val contracts = catalog.root.getJSONArray("contracts")
        for (index in 0 until contracts.length()) {
            val contract = contracts.getJSONObject(index)
            if (contract.getString("name") == name &&
                contract.getString("platform") in platforms
            ) {
                return JSONObject(contract.getJSONObject("delivery").toString())
            }
        }
        return null
    }

    fun policy(id: String): ComponentUpdatePolicy? {
        val policies = active?.root?.getJSONArray("policies") ?: return null
        for (index in 0 until policies.length()) {
            val policy = policies.getJSONObject(index)
            if (policy.getString("id") == id) {
                return ComponentUpdatePolicy(
                    mode = policy.getString("mode"),
                    checkHours = policy.getLong("checkHours"),
                    group = policy.getString("group"),
                )
            }
        }
        return null
    }

    private fun activate(candidate: VerifiedComponentCatalog) {
        synchronized(lock) {
            if (active == null || candidate.sequence > requireNotNull(active).sequence) {
                active = candidate
            }
        }
    }
}
