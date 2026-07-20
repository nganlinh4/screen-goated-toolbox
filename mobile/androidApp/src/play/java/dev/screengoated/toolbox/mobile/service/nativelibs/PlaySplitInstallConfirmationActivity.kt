package dev.screengoated.toolbox.mobile.service.nativelibs

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import com.google.android.play.core.splitinstall.SplitInstallManagerFactory
import com.google.android.play.core.splitinstall.SplitInstallSessionState
import com.google.android.play.core.splitinstall.model.SplitInstallSessionStatus

class PlaySplitInstallConfirmationActivity : ComponentActivity() {
    private val splitManager by lazy { SplitInstallManagerFactory.create(applicationContext) }
    private var sessionId = INVALID_SESSION_ID
    private var resolutionLaunched = false

    private val confirmationLauncher = registerForActivityResult(
        ActivityResultContracts.StartIntentSenderForResult(),
    ) { result ->
        PlaySplitInstallConfirmationCoordinator.promptResolved(
            sessionId = sessionId,
            accepted = result.resultCode == Activity.RESULT_OK,
        )
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        sessionId = intent.getIntExtra(EXTRA_SESSION_ID, INVALID_SESSION_ID)
        if (!PlaySplitInstallConfirmationCoordinator.ensureActive(sessionId)) {
            finish()
            return
        }
        resolutionLaunched = savedInstanceState?.getBoolean(STATE_RESOLUTION_LAUNCHED) == true
        if (!resolutionLaunched) loadSessionAndLaunch()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putBoolean(STATE_RESOLUTION_LAUNCHED, resolutionLaunched)
        super.onSaveInstanceState(outState)
    }

    private fun loadSessionAndLaunch() {
        splitManager.getSessionState(sessionId)
            .addOnSuccessListener(::launchConfirmation)
            .addOnFailureListener { error ->
                PlaySplitInstallConfirmationCoordinator.fail(
                    sessionId,
                    error.message ?: "Unable to read Play feature install session",
                )
                finish()
            }
    }

    private fun launchConfirmation(state: SplitInstallSessionState) {
        if (state.status() != SplitInstallSessionStatus.REQUIRES_USER_CONFIRMATION) {
            when (state.status()) {
                SplitInstallSessionStatus.INSTALLED,
                SplitInstallSessionStatus.CANCELED,
                SplitInstallSessionStatus.FAILED ->
                    PlaySplitInstallConfirmationCoordinator.release(sessionId)
                else -> PlaySplitInstallConfirmationCoordinator.resetRequest(sessionId)
            }
            finish()
            return
        }
        resolutionLaunched = true
        val started = try {
            splitManager.startConfirmationDialogForResult(state, confirmationLauncher)
        } catch (error: RuntimeException) {
            PlaySplitInstallConfirmationCoordinator.fail(
                sessionId,
                error.message ?: "Unable to open Play feature confirmation",
            )
            finish()
            return
        }
        if (!started) {
            PlaySplitInstallConfirmationCoordinator.fail(
                sessionId,
                "Play feature confirmation was not available",
            )
            finish()
        }
    }

    companion object {
        private const val EXTRA_SESSION_ID =
            "dev.screengoated.toolbox.mobile.extra.SPLIT_INSTALL_SESSION_ID"
        private const val STATE_RESOLUTION_LAUNCHED = "resolution_launched"
        private const val INVALID_SESSION_ID = 0

        fun intent(context: Context, sessionId: Int): Intent =
            Intent(context, PlaySplitInstallConfirmationActivity::class.java)
                .putExtra(EXTRA_SESSION_ID, sessionId)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
    }
}
