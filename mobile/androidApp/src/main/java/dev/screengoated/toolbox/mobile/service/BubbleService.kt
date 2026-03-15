package dev.screengoated.toolbox.mobile.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.media.AudioAttributes
import android.net.Uri
import android.os.IBinder
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.ImageView
import android.widget.Toast
import androidx.core.app.NotificationCompat
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.branding.MobileBrandAssets
import kotlin.math.abs

class BubbleService : Service() {

    private lateinit var windowManager: WindowManager
    private lateinit var bubbleView: ImageView
    private lateinit var layoutParams: WindowManager.LayoutParams

    private var attached = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        isRunning = true

        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        val density = resources.displayMetrics.density
        val sizePx = (BUBBLE_SIZE_DP * density).toInt()

        bubbleView = ImageView(this).apply {
            val isDark = MobileBrandAssets.isDarkTheme(resources.configuration)
            setImageResource(MobileBrandAssets.appIcon(isDark))
            scaleType = ImageView.ScaleType.CENTER_INSIDE
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
            x = 0
            y = (200 * density).toInt()
        }

        bubbleView.setOnTouchListener(BubbleTouchListener())
        windowManager.addView(bubbleView, layoutParams)
        attached = true

        ensureChannel()
        startForeground(NOTIFICATION_ID, buildNotification())
    }

    override fun onDestroy() {
        super.onDestroy()
        isRunning = false
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
        if (intent?.action == ACTION_STOP) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
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
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = event.rawX - initialTouchX
                    val dy = event.rawY - initialTouchY
                    if (!isDragging && (abs(dx) > DRAG_THRESHOLD || abs(dy) > DRAG_THRESHOLD)) {
                        isDragging = true
                    }
                    if (isDragging) {
                        layoutParams.x = initialX + dx.toInt()
                        layoutParams.y = initialY + dy.toInt()
                        if (attached) {
                            runCatching { windowManager.updateViewLayout(bubbleView, layoutParams) }
                        }
                    }
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    if (!isDragging) {
                        Toast.makeText(this@BubbleService, "Panel coming soon", Toast.LENGTH_SHORT).show()
                    }
                    return true
                }
            }
            return false
        }
    }

    companion object {
        const val CHANNEL_ID = "sgt_bubble"
        const val NOTIFICATION_ID = 1002
        const val ACTION_STOP = "dev.screengoated.toolbox.mobile.service.STOP_BUBBLE"

        @Volatile
        var isRunning: Boolean = false
            private set

        private const val BUBBLE_SIZE_DP = 48
        private const val DRAG_THRESHOLD = 10f
    }
}
