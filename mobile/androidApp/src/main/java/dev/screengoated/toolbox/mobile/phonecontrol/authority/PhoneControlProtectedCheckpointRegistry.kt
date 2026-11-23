package dev.screengoated.toolbox.mobile.phonecontrol.authority

import java.util.concurrent.atomic.AtomicLong
import java.util.UUID

internal class PhoneControlProtectedCheckpointToken internal constructor(
    internal val id: Long,
) {
    internal val freshProjectionOwner: String = UUID.randomUUID().toString()
}

internal data class PhoneControlProtectedCheckpointSnapshot(
    val generation: Long,
    val active: Boolean,
    val freshProjectionRequired: Boolean,
)

/**
 * Process-local visual privacy boundary for a platform-owned setup checkpoint.
 *
 * The live runtime may remain connected while this registry blocks every model
 * tool. Provider adapters can prove ownership with the opaque token without
 * exposing checkpoint content to the model-facing dispatcher.
 */
internal object PhoneControlProtectedCheckpointRegistry {
    private val lock = Any()
    private val nextId = AtomicLong(0L)
    private var activeTokenId: Long? = null
    private var activeProjectionOwner: String? = null
    private var activeCapturePolicy: PhoneControlProtectedCapturePolicy? = null
    private var generation = 0L

    fun begin(
        capturePolicy: PhoneControlProtectedCapturePolicy,
    ): PhoneControlProtectedCheckpointToken = synchronized(lock) {
        check(activeTokenId == null) { "a protected checkpoint is already active" }
        val token = PhoneControlProtectedCheckpointToken(nextId.incrementAndGet())
        activeTokenId = token.id
        activeProjectionOwner = token.freshProjectionOwner
        activeCapturePolicy = capturePolicy
        generation += 1
        token
    }

    fun end(token: PhoneControlProtectedCheckpointToken): Boolean = synchronized(lock) {
        if (activeTokenId != token.id) return@synchronized false
        activeTokenId = null
        activeProjectionOwner = null
        activeCapturePolicy = null
        generation += 1
        true
    }

    fun owns(token: PhoneControlProtectedCheckpointToken): Boolean = synchronized(lock) {
        activeTokenId == token.id
    }

    fun freshProjectionOwner(): String? = synchronized(lock) {
        activeProjectionOwner.takeIf {
            activeCapturePolicy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
        }
    }

    fun ownsFreshProjection(owner: String?): Boolean = synchronized(lock) {
        owner != null && owner == activeProjectionOwner &&
            activeCapturePolicy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
    }

    fun ownsFreshProjection(token: PhoneControlProtectedCheckpointToken, owner: String?): Boolean =
        synchronized(lock) {
            activeTokenId == token.id && owner != null && owner == activeProjectionOwner &&
                owner == token.freshProjectionOwner &&
                activeCapturePolicy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
        }

    fun hasActiveCheckpoint(): Boolean = synchronized(lock) { activeTokenId != null }

    fun freshProjectionRequired(): Boolean = synchronized(lock) {
        activeTokenId != null &&
            activeCapturePolicy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
    }

    fun modelToolsAllowed(): Boolean = !hasActiveCheckpoint()

    fun snapshot(): PhoneControlProtectedCheckpointSnapshot = synchronized(lock) {
        PhoneControlProtectedCheckpointSnapshot(
            generation = generation,
            active = activeTokenId != null,
            freshProjectionRequired = activeTokenId != null &&
                activeCapturePolicy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
    }
}
