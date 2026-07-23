package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.IBinder
import java.io.Closeable
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withTimeout

internal class SgtAdbServiceClient(
    context: Context,
    private val onUnavailable: () -> Unit = {},
) : Closeable {
    private val appContext = context.applicationContext
    private val lock = Any()

    @Volatile
    private var remote: IPhoneControlAdbService? = null
    private var activeBinding: Binding? = null

    suspend fun await(): IPhoneControlAdbService {
        readyRemote()?.let { return it }
        var lastFailure: Throwable? = null
        repeat(BIND_ATTEMPTS) {
            val binding = binding()
            binding.start()
            try {
                return withTimeout(BIND_TIMEOUT_MS) { binding.pending.await() }
            } catch (timeout: TimeoutCancellationException) {
                lastFailure = timeout
                retire(binding, "The local ADB bridge binding timed out.")
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                lastFailure = error
            }
        }
        throw lastFailure ?: IllegalStateException("The local ADB bridge is unavailable.")
    }

    override fun close() {
        val binding = synchronized(lock) {
            remote = null
            activeBinding.also { activeBinding = null }
        }
        binding?.pending?.cancel()
        binding?.unbind()
    }

    private fun readyRemote(): IPhoneControlAdbService? {
        val candidate = remote ?: return null
        if (candidate.asBinder().pingBinder()) return candidate
        synchronized(lock) {
            if (remote === candidate) remote = null
        }
        retireCurrent("The local ADB bridge binder is no longer alive.")
        return null
    }

    private fun binding(): Binding = synchronized(lock) {
        readyRemote()?.let { connected ->
            return Binding().also { it.pending.complete(connected) }
        }
        activeBinding?.takeIf { it.pending.isActive }?.let { return it }
        Binding().also { activeBinding = it }
    }

    private fun connected(binding: Binding, binder: IBinder?) {
        val service = binder
            ?.takeIf(IBinder::pingBinder)
            ?.let(IPhoneControlAdbService.Stub::asInterface)
        if (service == null) {
            retire(binding, "The local ADB bridge returned an invalid binder.")
            return
        }
        val accepted = synchronized(lock) {
            if (activeBinding !== binding || !binding.pending.isActive) {
                false
            } else {
                remote = service
                binding.pending.complete(service)
                true
            }
        }
        if (!accepted) binding.unbind()
    }

    private fun retireCurrent(message: String) {
        val binding = synchronized(lock) { activeBinding } ?: return
        retire(binding, message)
    }

    private fun retire(binding: Binding, message: String) {
        val owned = synchronized(lock) {
            if (activeBinding !== binding) return
            activeBinding = null
            remote = null
            binding.pending.completeExceptionally(IllegalStateException(message))
            true
        }
        if (owned) {
            binding.unbind()
            onUnavailable()
        }
    }

    private inner class Binding : ServiceConnection {
        val pending = CompletableDeferred<IPhoneControlAdbService>()
        private val started = AtomicBoolean(false)
        private val registered = AtomicBoolean(false)

        fun start() {
            if (!started.compareAndSet(false, true) || pending.isCompleted) return
            registered.set(true)
            val accepted = runCatching {
                appContext.bindService(
                    Intent(appContext, SgtAdbBridgeService::class.java),
                    this,
                    Context.BIND_AUTO_CREATE,
                )
            }.getOrDefault(false)
            if (!accepted) {
                registered.set(false)
                retire(this, "The local ADB bridge could not be bound.")
            }
        }

        fun unbind() {
            if (!registered.compareAndSet(true, false)) return
            runCatching { appContext.unbindService(this) }
        }

        override fun onServiceConnected(name: ComponentName, service: IBinder?) {
            connected(this, service)
        }

        override fun onServiceDisconnected(name: ComponentName) {
            retire(this, "The local ADB bridge service disconnected.")
        }

        override fun onBindingDied(name: ComponentName) {
            retire(this, "The local ADB bridge binding died.")
        }

        override fun onNullBinding(name: ComponentName) {
            retire(this, "The local ADB bridge returned no binder.")
        }
    }

    private companion object {
        const val BIND_TIMEOUT_MS = 10_000L
        const val BIND_ATTEMPTS = 2
    }
}
