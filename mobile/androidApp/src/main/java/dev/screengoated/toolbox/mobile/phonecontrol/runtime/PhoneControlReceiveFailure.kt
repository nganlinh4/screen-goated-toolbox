package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleAdapter
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleConnection
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure

internal suspend fun observePhoneControlReceiveFailure(
    lifecycle: GeminiLiveLifecycleAdapter,
    connection: GeminiLiveLifecycleConnection,
    failure: GeminiLiveSessionFailure,
) {
    Log.w("SGTPhoneControl", "transport_receive_failed type=${failure::class.simpleName}")
    if (failure is GeminiLiveSessionFailure.Server) {
        lifecycle.serverError(connection.generation, failure.retryable)
    } else {
        lifecycle.transportFailed(connection.generation)
    }
}
