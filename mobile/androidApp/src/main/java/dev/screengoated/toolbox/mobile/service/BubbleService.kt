package dev.screengoated.toolbox.mobile.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.graphics.Rect
import android.graphics.PixelFormat
import android.media.AudioAttributes
import android.net.Uri
import android.os.IBinder
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.Toast
import androidx.core.app.NotificationCompat
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.branding.MobileBrandAssets
import dev.screengoated.toolbox.mobile.service.preset.PresetAudioForegroundMode
import dev.screengoated.toolbox.mobile.service.tts.toRuntimeSnapshot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

class BubbleService : Service() {

    private lateinit var windowManager: WindowManager
    private lateinit var bubbleView: ImageView
    private lateinit var layoutParams: WindowManager.LayoutParams
    private lateinit var positionPrefs: android.content.SharedPreferences
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var presetOverlayController: dev.screengoated.toolbox.mobile.service.preset.PresetOverlayController? = null

    private var attached = false
    private var bubbleFrontPending = false
    private val dismissTargets = MorphDismissZone.singleDismiss()
    private var dismissZone: MorphDismissZone? = null
    private val lastFingerDistanceSq = FloatArray(dismissTargets.size) { Float.POSITIVE_INFINITY }
    private var isPanelExpanded = false
    private var opacityDecayJob: Job? = null
    private var recentInteractionUntilMs = 0L
    private var resetPositionOnDestroy = false
    internal var currentAudioForegroundMode: PresetAudioForegroundMode = PresetAudioForegroundMode.NONE

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannel()
        startBubbleForeground()

        if (!Settings.canDrawOverlays(this)) {
            Toast.makeText(this, "Overlay permission is required for the SGT bubble.", Toast.LENGTH_SHORT).show()
            val intent = Intent(
                Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                Uri.parse("package:$packageName"),
            ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            startActivity(intent)
            stopSelf()
            return
        }

        runCatching {
            windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
            positionPrefs = getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
            val appContainer = (application as SgtMobileApplication).appContainer
            val density = resources.displayMetrics.density
            val sizePx = dp(currentBubbleSizeDp())

            bubbleView = ImageView(this).apply {
                val isDark = MobileBrandAssets.isDarkTheme(resources.configuration)
                setImageResource(MobileBrandAssets.appIcon(isDark))
                scaleType = ImageView.ScaleType.CENTER_INSIDE
                alpha = BUBBLE_INACTIVE_ALPHA
            }

            layoutParams = WindowManager.LayoutParams(
                sizePx,
                sizePx,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
                PixelFormat.TRANSLUCENT,
            ).apply {
                gravity = Gravity.TOP or Gravity.START
                x = positionPrefs.getInt(KEY_BUBBLE_X, defaultBubbleX())
                y = positionPrefs.getInt(KEY_BUBBLE_Y, (200 * density).toInt())
            }

            presetOverlayController = dev.screengoated.toolbox.mobile.service.preset.PresetOverlayController(
                context = this,
                scope = serviceScope,
                windowManager = windowManager,
                presetRepository = appContainer.presetRepository,
                uiPreferencesFlow = appContainer.repository.uiPreferences,
                uiPreferencesProvider = appContainer.repository::currentUiPreferences,
                keepOpenProvider = ::isKeepOpenEnabled,
                onKeepOpenChanged = ::setKeepOpenEnabled,
                onIncreaseBubbleSize = ::increaseBubbleSize,
                onDecreaseBubbleSize = ::decreaseBubbleSize,
                onPanelExpandedChanged = ::setPanelExpanded,
                onBubbleSuppressedChanged = ::setBubbleSuppressed,
                onRequestBubbleFront = ::bringBubbleToFront,
                onAudioCaptureForegroundModeChanged = ::setAudioCaptureForegroundMode,
                ttsRuntimeService = appContainer.ttsRuntimeService,
                ttsSettingsSnapshotProvider = {
                    appContainer.repository.currentGlobalTtsSettings()
                        .toRuntimeSnapshot()
                },
            )
            bubbleView.setOnTouchListener(BubbleTouchListener())
            windowManager.addView(bubbleView, layoutParams)
            attached = true
            presetOverlayController?.updateBubbleBounds(currentBubbleBounds())
            isRunning = true
        }.onFailure {
            Log.e(TAG, "BubbleService failed to start", it)
            Toast.makeText(this, "SGT bubble could not start.", Toast.LENGTH_SHORT).show()
            stopSelf()
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        isRunning = false
        if (resetPositionOnDestroy) {
            resetBubblePosition()
        } else {
            saveBubblePosition()
        }
        hideDismissZone()
        opacityDecayJob?.cancel()
        presetOverlayController?.destroy()
        presetOverlayController = null
        serviceScope.cancel()
        if (attached) {
            runCatching { windowManager.removeView(bubbleView) }
            attached = false
        }
    }

    private fun ensureChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            CHANNEL_ID,
            "SGT Bubble",
            NotificationManager.IMPORTANCE_MIN,
        ).apply {
            description = "Floating bubble overlay"
            setSound(null as Uri?, null as AudioAttributes?)
            enableVibration(false)
            setShowBadge(false)
            lockscreenVisibility = Notification.VISIBILITY_SECRET
        }
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(): Notification {
        val openAppIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stopIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, BubbleService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("SGT Bubble")
            .setContentText("Floating bubble is active")
            .setContentIntent(openAppIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setLocalOnly(true)
            .setShowWhen(false)
            .addAction(0, "Stop", stopIntent)
            .build()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                resetPositionOnDestroy = true
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_RESUME_PENDING_AUDIO_PRESET -> {
                presetOverlayController?.resumePendingAudioLaunch()
                return START_STICKY
            }
        }
        return START_STICKY
    }

    private inner class BubbleTouchListener : View.OnTouchListener {
        private var initialX = 0
        private var initialY = 0
        private var initialTouchX = 0f
        private var initialTouchY = 0f
        private var isDragging = false

        override fun onTouch(view: View, event: MotionEvent): Boolean {
            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    initialX = layoutParams.x
                    initialY = layoutParams.y
                    initialTouchX = event.rawX
                    initialTouchY = event.rawY
                    isDragging = false
                    recordBubbleInteraction(immediate = true)
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = event.rawX - initialTouchX
                    val dy = event.rawY - initialTouchY
                    if (!isDragging && (abs(dx) > DRAG_THRESHOLD || abs(dy) > DRAG_THRESHOLD)) {
                        isDragging = true
                    }
                    if (isDragging) {
                        recordBubbleInteraction()
                        layoutParams.x = initialX + dx.toInt()
                        layoutParams.y = initialY + dy.toInt()
                        if (attached) {
                            runCatching { windowManager.updateViewLayout(bubbleView, layoutParams) }
                        }
                        presetOverlayController?.updateBubbleBounds(currentBubbleBounds())
                        updateDismissZone(event.rawX, event.rawY)
                    }
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    if (isDragging) {
                        recordBubbleInteraction()
                        val proximity = bubbleDragDismissProximity(event.rawX, event.rawY)
                        resetDismissTracking()
                        if (proximity >= 0.8f) {
                            // Swallow: shrink bubble into dismiss target
                            bubbleView.animate()
                                .scaleX(0.2f).scaleY(0.2f).alpha(0f)
                                .setDuration(180).start()
                            dismissZone?.swallow(0) {
                                resetPositionOnDestroy = true
                                dismissZone = null
                                stopForeground(STOP_FOREGROUND_REMOVE)
                                stopSelf()
                            } ?: run {
                                resetPositionOnDestroy = true
                                stopForeground(STOP_FOREGROUND_REMOVE)
                                stopSelf()
                            }
                        } else {
                            hideDismissZone()
                            saveBubblePosition()
                        }
                    } else {
                        recordBubbleInteraction()
                        runCatching {
                            presetOverlayController?.togglePanel()
                        }.onFailure {
                            Log.e(TAG, "Bubble panel failed to open", it)
                            Toast.makeText(this@BubbleService, "Bubble panel failed to open.", Toast.LENGTH_SHORT).show()
                        }
                    }
                    return true
                }
                MotionEvent.ACTION_CANCEL -> {
                    hideDismissZone()
                    resetDismissTracking()
                    return true
                }
            }
            return false
        }
    }

    private fun currentBubbleBounds(): OverlayBounds {
        return OverlayBounds(
            x = layoutParams.x,
            y = layoutParams.y,
            width = layoutParams.width,
            height = layoutParams.height,
        )
    }

    private fun bringBubbleToFront() {
        if (!attached) {
            return
        }
        // Debounce: coalesce rapid calls into a single remove+add per frame
        if (bubbleFrontPending) return
        bubbleFrontPending = true
        bubbleView.post {
            bubbleFrontPending = false
            if (!attached) return@post
            recordBubbleInteraction()
            runCatching { windowManager.removeViewImmediate(bubbleView) }
                .onSuccess { attached = false }
                .onFailure { Log.w(TAG, "Could not remove bubble before front reorder", it) }
            runCatching { windowManager.addView(bubbleView, layoutParams) }
                .onSuccess { attached = true }
                .onFailure {
                    attached = false
                    Log.w(TAG, "Could not bring bubble to front", it)
                }
        }
    }

    private fun isKeepOpenEnabled(): Boolean {
        return positionPrefs.getBoolean(KEY_KEEP_OPEN, false)
    }

    private fun setKeepOpenEnabled(enabled: Boolean) {
        positionPrefs.edit()
            .putBoolean(KEY_KEEP_OPEN, enabled)
            .apply()
    }

    private fun setPanelExpanded(expanded: Boolean) {
        isPanelExpanded = expanded
        if (expanded) {
            recordBubbleInteraction()
        } else {
            applyBubbleOpacity(animated = true)
            scheduleOpacityDecay()
        }
    }

    private fun setBubbleSuppressed(suppressed: Boolean) {
        if (!::bubbleView.isInitialized) {
            return
        }
        bubbleView.visibility = if (suppressed) View.INVISIBLE else View.VISIBLE
    }

    private fun setAudioCaptureForegroundMode(mode: PresetAudioForegroundMode) {
        applyBubbleForegroundMode(this, mode, buildNotification())
    }

    private fun startBubbleForeground() {
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification, android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        currentAudioForegroundMode = PresetAudioForegroundMode.NONE
    }

    private fun currentBubbleSizeDp(): Int {
        return positionPrefs.getInt(KEY_BUBBLE_SIZE_DP, DEFAULT_BUBBLE_SIZE_DP)
            .coerceIn(MIN_BUBBLE_SIZE_DP, MAX_BUBBLE_SIZE_DP)
    }

    private fun increaseBubbleSize() {
        updateBubbleSizeBy(BUBBLE_SIZE_STEP_DP)
    }

    private fun decreaseBubbleSize() {
        updateBubbleSizeBy(-BUBBLE_SIZE_STEP_DP)
    }

    private fun updateBubbleSizeBy(deltaDp: Int) {
        recordBubbleInteraction()
        val oldSizeDp = currentBubbleSizeDp()
        val newSizeDp = (oldSizeDp + deltaDp).coerceIn(MIN_BUBBLE_SIZE_DP, MAX_BUBBLE_SIZE_DP)
        if (newSizeDp == oldSizeDp) {
            return
        }
        val oldSizePx = dp(oldSizeDp)
        val newSizePx = dp(newSizeDp)
        val centerX = layoutParams.x + (oldSizePx / 2)
        val centerY = layoutParams.y + (oldSizePx / 2)
        layoutParams.width = newSizePx
        layoutParams.height = newSizePx
        layoutParams.x = (centerX - (newSizePx / 2)).coerceAtLeast(0)
        layoutParams.y = (centerY - (newSizePx / 2)).coerceAtLeast(0)
        positionPrefs.edit()
            .putInt(KEY_BUBBLE_SIZE_DP, newSizeDp)
            .putInt(KEY_BUBBLE_X, layoutParams.x)
            .putInt(KEY_BUBBLE_Y, layoutParams.y)
            .apply()
        if (attached) {
            runCatching { windowManager.updateViewLayout(bubbleView, layoutParams) }
                .onFailure { Log.w(TAG, "Could not resize bubble", it) }
        }
        presetOverlayController?.updateBubbleBounds(currentBubbleBounds())
    }

    private fun recordBubbleInteraction(immediate: Boolean = false) {
        recentInteractionUntilMs = SystemClock.elapsedRealtime() + RECENT_INTERACTION_MS
        applyBubbleOpacity(animated = !immediate)
        scheduleOpacityDecay()
    }

    private fun scheduleOpacityDecay() {
        opacityDecayJob?.cancel()
        if (isPanelExpanded) {
            return
        }
        val remainingMs = (recentInteractionUntilMs - SystemClock.elapsedRealtime()).coerceAtLeast(0L)
        opacityDecayJob = serviceScope.launch {
            delay(remainingMs)
            if (!isPanelExpanded && SystemClock.elapsedRealtime() >= recentInteractionUntilMs) {
                applyBubbleOpacity(animated = true)
            }
        }
    }

    private fun applyBubbleOpacity(animated: Boolean) {
        val active = isPanelExpanded || SystemClock.elapsedRealtime() < recentInteractionUntilMs
        val targetAlpha = if (active) BUBBLE_ACTIVE_ALPHA else BUBBLE_INACTIVE_ALPHA
        bubbleView.animate().cancel()
        if (animated) {
            bubbleView.animate()
                .alpha(targetAlpha)
                .setDuration(BUBBLE_OPACITY_ANIM_MS)
                .start()
        } else {
            bubbleView.alpha = targetAlpha
        }
    }

    private fun updateDismissZone(rawX: Float, rawY: Float) {
        val zone = dismissZone ?: MorphDismissZone(
            context = this,
            windowManager = windowManager,
            targets = dismissTargets,
        ).also { dismissZone = it; it.show() }
        zone.update(currentDismissHit(rawX, rawY).proximities)
    }

    private fun hideDismissZone() {
        dismissZone?.hide()
        dismissZone = null
        resetDismissTracking()
    }

    private fun bubbleDragDismissProximity(rawX: Float, rawY: Float): Float {
        return currentDismissHit(rawX, rawY).proximities.firstOrNull() ?: 0f
    }

    private fun currentDismissHit(rawX: Float, rawY: Float): MorphDismissZone.DismissHitResult {
        val metrics = resources.displayMetrics
        return MorphDismissZone.hitTest(
            rawX = rawX,
            rawY = rawY,
            screenBounds = Rect(0, 0, metrics.widthPixels, metrics.heightPixels),
            density = metrics.density,
            coordinateScale = 1f,
            targets = dismissTargets,
            previousDistanceSq = lastFingerDistanceSq,
        ).also { it.distanceSq.copyInto(lastFingerDistanceSq) }
    }

    private fun resetDismissTracking() {
        lastFingerDistanceSq.fill(Float.POSITIVE_INFINITY)
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private fun saveBubblePosition() {
        if (!::positionPrefs.isInitialized) {
            return
        }
        positionPrefs.edit()
            .putInt(KEY_BUBBLE_X, layoutParams.x)
            .putInt(KEY_BUBBLE_Y, layoutParams.y)
            .apply()
    }

    private fun resetBubblePosition() {
        if (!::positionPrefs.isInitialized) {
            return
        }
        val defaultY = (200 * resources.displayMetrics.density).toInt()
        positionPrefs.edit()
            .putInt(KEY_BUBBLE_X, defaultBubbleX())
            .putInt(KEY_BUBBLE_Y, defaultY)
            .apply()
    }

    private fun defaultBubbleX(): Int {
        val bubbleSize = dp(currentBubbleSizeDp())
        val screenWidth = resources.displayMetrics.widthPixels
        return (screenWidth - bubbleSize - dp(DEFAULT_BUBBLE_MARGIN_DP)).coerceAtLeast(0)
    }

    companion object {
        private const val TAG = "BubbleService"
        const val CHANNEL_ID = "sgt_bubble"
        const val NOTIFICATION_ID = 1002
        const val ACTION_STOP = "dev.screengoated.toolbox.mobile.service.STOP_BUBBLE"
        const val ACTION_RESUME_PENDING_AUDIO_PRESET =
            "dev.screengoated.toolbox.mobile.service.RESUME_PENDING_AUDIO_PRESET"

        @Volatile
        var isRunning: Boolean = false
            private set

        private const val PREFS_NAME = "sgt_bubble"
        private const val KEY_BUBBLE_X = "bubble_x"
        private const val KEY_BUBBLE_Y = "bubble_y"
        private const val KEY_BUBBLE_SIZE_DP = "bubble_size_dp"
        private const val KEY_KEEP_OPEN = "keep_open"
        private const val DEFAULT_BUBBLE_SIZE_DP = 28
        private const val MIN_BUBBLE_SIZE_DP = 16
        private const val MAX_BUBBLE_SIZE_DP = 56
        private const val BUBBLE_SIZE_STEP_DP = 4
        private const val DEFAULT_BUBBLE_MARGIN_DP = 12
        private const val DRAG_THRESHOLD = 10f
        private const val RECENT_INTERACTION_MS = 1_000L
        private const val BUBBLE_OPACITY_ANIM_MS = 180L
        private const val BUBBLE_INACTIVE_ALPHA = 80f / 255f
        private const val BUBBLE_ACTIVE_ALPHA = 1f

        fun resumePendingAudioPreset(context: android.content.Context) {
            context.startService(
                Intent(context, BubbleService::class.java)
                    .setAction(ACTION_RESUME_PENDING_AUDIO_PRESET),
            )
        }
    }
}
