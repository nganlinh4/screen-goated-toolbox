package dev.screengoated.toolbox.mobile.updater

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

internal data class AppUpdateProjection(
    val status: AppUpdateStatus,
    val latestVersion: String? = null,
    val releaseNotes: String = "",
    val actionUrl: String? = null,
    val errorMessage: String? = null,
    val notificationVersion: String? = null,
)

internal class AppUpdateCheck
internal class AppUpdateCompletion
internal class AppUpdateLaunch<Action : Any>(val value: Action)

internal class AppUpdateStateOwner<Action : Any>(currentVersion: String) {
    private val mutableState=MutableStateFlow(AppUpdateUiState(currentVersion=currentVersion))
    val state: StateFlow<AppUpdateUiState> = mutableState.asStateFlow()
    private var autoCheckStarted=false
    private var cachedAction: Action?=null
    private val gate=Any()
    private var checking: AppUpdateCheck?=null
    private var launching: AppUpdateLaunch<Action>?=null
    private var completing: AppUpdateCompletion?=null

    fun beginAutoCheck(): AppUpdateCheck? = synchronized(gate) {
        if (autoCheckStarted) return@synchronized null
        autoCheckStarted=true
        beginCheck()
    }

    fun beginCheck(): AppUpdateCheck? = synchronized(gate) {
        if (launching!=null || mutableState.value.status in setOf(AppUpdateStatus.CHECKING,
                AppUpdateStatus.DOWNLOADING,AppUpdateStatus.DOWNLOADED,AppUpdateStatus.INSTALLING)) return@synchronized null
        cachedAction=null
        val check=AppUpdateCheck()
        checking=check
        mutableState.value=mutableState.value.copy(status=AppUpdateStatus.CHECKING,errorMessage=null,notification=null,actionUrl=null)
        check.takeIf { checking===it && state.value.status==AppUpdateStatus.CHECKING }
    }

    fun finishCheck(check: AppUpdateCheck, result: AppUpdateProjection, action: Action? = null): Boolean = synchronized(gate) {
        if (checking!==check || mutableState.value.status!=AppUpdateStatus.CHECKING) return@synchronized false
        require(result.status !in setOf(AppUpdateStatus.IDLE,AppUpdateStatus.CHECKING))
        checking=null
        cachedAction=action.takeIf { result.status==AppUpdateStatus.UPDATE_AVAILABLE }
        val current=mutableState.value
        mutableState.value=current.copy(status=result.status,
            latestVersion=result.latestVersion ?: current.currentVersion.takeIf { result.status==AppUpdateStatus.UP_TO_DATE },
            releaseNotes=result.releaseNotes.takeIf { result.status==AppUpdateStatus.UPDATE_AVAILABLE }.orEmpty(),
            actionUrl=result.actionUrl.takeIf { result.status==AppUpdateStatus.UPDATE_AVAILABLE },errorMessage=result.errorMessage,
            notification=result.notificationVersion?.takeIf { result.status==AppUpdateStatus.UPDATE_AVAILABLE }?.let(::AppUpdateNotice))
        true
    }

    fun installState(status: AppUpdateStatus, errorMessage: String? = null) = synchronized(gate) {
        require(status in setOf(AppUpdateStatus.DOWNLOADING,AppUpdateStatus.DOWNLOADED,AppUpdateStatus.INSTALLING,AppUpdateStatus.ERROR))
        val current=mutableState.value
        if (current.status in setOf(AppUpdateStatus.DOWNLOADED,AppUpdateStatus.INSTALLING) && status==AppUpdateStatus.DOWNLOADING ||
            current.status==AppUpdateStatus.INSTALLING && status==AppUpdateStatus.DOWNLOADED) return@synchronized
        checking=null;cachedAction=null;launching=null
        if (status==AppUpdateStatus.ERROR && current.status in setOf(AppUpdateStatus.DOWNLOADED,AppUpdateStatus.INSTALLING)) {
            completing=null
            mutableState.value=current.copy(status=AppUpdateStatus.DOWNLOADED,errorMessage=errorMessage,notification=null,actionUrl=null)
        } else {
            mutableState.value=current.copy(status=status,errorMessage=errorMessage,notification=null,actionUrl=null)
        }
    }

    fun claimAction(): AppUpdateLaunch<Action>? = synchronized(gate) {
        if (state.value.status!=AppUpdateStatus.UPDATE_AVAILABLE || launching!=null) return@synchronized null
        val action=cachedAction ?: return@synchronized null
        cachedAction=null
        AppUpdateLaunch(action).also { launching=it }
    }

    fun finishLaunch(launch: AppUpdateLaunch<Action>, accepted: Boolean, error: String) = synchronized(gate) {
        if (launching!==launch) return@synchronized
        launching=null
        mutableState.value=state.value.copy(status=if(accepted) AppUpdateStatus.DOWNLOADING else AppUpdateStatus.ERROR,
            errorMessage=error.takeUnless { accepted },notification=null,actionUrl=null)
    }

    fun beginCompletion(): AppUpdateCompletion? = synchronized(gate) {
        if (state.value.status!=AppUpdateStatus.DOWNLOADED || completing!=null) return@synchronized null
        val completion=AppUpdateCompletion()
        completing=completion
        mutableState.value=state.value.copy(status=AppUpdateStatus.INSTALLING,errorMessage=null,notification=null)
        completion.takeIf { completing===it && state.value.status==AppUpdateStatus.INSTALLING }
    }

    fun finishCompletion(completion: AppUpdateCompletion, error: String?) = synchronized(gate) {
        if (completing!==completion || state.value.status!=AppUpdateStatus.INSTALLING) return@synchronized
        completing=null
        if (error!=null) mutableState.value=state.value.copy(status=AppUpdateStatus.DOWNLOADED,errorMessage=error)
    }

    fun consumeNotification(notice: AppUpdateNotice?): String? = synchronized(gate) {
        if (notice==null || state.value.status!=AppUpdateStatus.UPDATE_AVAILABLE || state.value.notification!==notice) return@synchronized null
        mutableState.value=state.value.copy(notification=null)
        notice.version
    }
}
