package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject

/** Enter the actual dispatch context before sampling authority and reserving the effect. */
internal suspend fun runSgtAdbDispatch(
    dispatcher: CoroutineDispatcher,
    beforeDispatch: suspend () -> PrivilegedCommandResult.Failure?,
    reserve: () -> Boolean,
    dispatch: () -> JsonObject,
): PrivilegedCommandResult = withContext(dispatcher) {
    beforeDispatch()?.let { return@withContext it }
    if (!reserve()) throw CancellationException()
    PrivilegedCommandResult.Success(dispatch())
}
