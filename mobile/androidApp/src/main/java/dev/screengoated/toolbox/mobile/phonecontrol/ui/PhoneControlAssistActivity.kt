package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.app.role.RoleManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlExternalGoalRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlExternalGoalSlot
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService

/** Stateless system-assistant entry; all setup and runtime ownership stays elsewhere. */
class PhoneControlAssistActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        dispatchAssistantInvocation(intent)
        finish()
    }

    private fun dispatchAssistantInvocation(source: Intent) {
        val goal = source.phoneControlAssistGoal(defaultAssistantRoleHeld())
        val goalQueued = goal != null && PhoneControlExternalGoalRegistry.offer(goal)
        val route = phoneControlAssistantInvocationRoute(
            action = source.action,
            running = PhoneControlService.state.value.running,
            captureSuspended = PhoneControlService.captureSuspended,
        )
        val target = when (route) {
            PhoneControlAssistantInvocationRoute.ACTIVATE ->
                PhoneControlActivity.activationIntent(this)
            PhoneControlAssistantInvocationRoute.RESUME_CAPTURE ->
                PhoneControlActivity.resumeCaptureIntent(this)
            PhoneControlAssistantInvocationRoute.PRESERVE_RUNNING,
            PhoneControlAssistantInvocationRoute.IGNORE -> null
        }
        PhoneControlLog.i(
            TAG,
            "assistant_invocation route=${route.wireName} gateway_task_id=$taskId " +
                "dispatch_requested=${target != null} goal_queued=$goalQueued",
        )
        if (goalQueued && route == PhoneControlAssistantInvocationRoute.PRESERVE_RUNNING) {
            PhoneControlService.submitExternalGoal(this)
        }
        target
            ?.addFlags(PhoneControlActivity.COORDINATOR_REENTRY_FLAGS)
            ?.putExtra(EXTRA_COORDINATOR_SOURCE, SOURCE_SYSTEM_ASSISTANT)
            ?.let(::startActivity)
    }

    private fun defaultAssistantRoleHeld(): Boolean = runCatching {
        getSystemService(RoleManager::class.java)?.isRoleHeld(RoleManager.ROLE_ASSISTANT) == true
    }.getOrDefault(false)

    private companion object {
        const val TAG = "SGTPhoneControlAssistant"
    }
}

internal fun Intent.phoneControlAssistGoal(defaultAssistantRoleHeld: Boolean): String? {
    return phoneControlAssistGoal(
        action = action,
        defaultAssistantRoleHeld = defaultAssistantRoleHeld,
        text = getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString(),
    )
}

internal fun phoneControlAssistGoal(
    action: String?,
    defaultAssistantRoleHeld: Boolean,
    text: String?,
): String? {
    if (action != Intent.ACTION_ASSIST || !defaultAssistantRoleHeld) return null
    return text
        ?.trim()
        ?.takeIf { it.isNotEmpty() && it.length <= PhoneControlExternalGoalSlot.MAXIMUM_CHARS }
}

internal enum class PhoneControlAssistantInvocationRoute(val wireName: String) {
    ACTIVATE("activate"),
    RESUME_CAPTURE("resume_capture"),
    PRESERVE_RUNNING("preserve_running"),
    IGNORE("ignore"),
}

internal fun phoneControlAssistantInvocationRoute(
    action: String?,
    running: Boolean,
    captureSuspended: Boolean,
): PhoneControlAssistantInvocationRoute = when {
    action != Intent.ACTION_ASSIST -> PhoneControlAssistantInvocationRoute.IGNORE
    captureSuspended -> PhoneControlAssistantInvocationRoute.RESUME_CAPTURE
    running -> PhoneControlAssistantInvocationRoute.PRESERVE_RUNNING
    else -> PhoneControlAssistantInvocationRoute.ACTIVATE
}

internal fun Intent.phoneControlCoordinatorEvent(event: String, mode: String): String =
    "$event mode=$mode source=${phoneControlCoordinatorSource()}"

private fun Intent.phoneControlCoordinatorSource(): String =
    if (getStringExtra(EXTRA_COORDINATOR_SOURCE) == SOURCE_SYSTEM_ASSISTANT) {
        SOURCE_SYSTEM_ASSISTANT
    } else {
        SOURCE_APP
    }

private const val EXTRA_COORDINATOR_SOURCE =
    "dev.screengoated.toolbox.mobile.phonecontrol.COORDINATOR_SOURCE"
private const val SOURCE_SYSTEM_ASSISTANT = "system_assistant"
private const val SOURCE_APP = "app"
