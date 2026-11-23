package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlStructuralEditAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.session.PhoneControlContractAssets
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatcher
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder

/** One runtime owns the shared committed-request history and dispatch state. */
internal class PhoneControlSessionCheckpoints(context: Context, assets: PhoneControlContractAssets) {
    private val structural = PhoneControlStructuralEditAuthorization(context)
    private val resources = PhoneControlResourceAuthorization(context)
    val dispatcher = PhoneControlToolDispatcher(context, assets.functionDeclarations, structural, resources)
    val recorders: List<PhoneControlTurnRecorder> = listOf(dispatcher, structural, resources)
}
