package dev.screengoated.toolbox.mobile

import android.content.Context
import com.google.android.play.core.splitcompat.SplitCompat

internal fun installFlavorRuntime(context: Context) {
    SplitCompat.install(context)
}
