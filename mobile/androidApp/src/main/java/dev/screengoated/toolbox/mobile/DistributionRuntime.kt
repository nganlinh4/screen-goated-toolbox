package dev.screengoated.toolbox.mobile

import android.content.Context

/** Flavor-owned process setup needed before distribution-specific code is used. */
internal fun installDistributionRuntime(context: Context) = installFlavorRuntime(context)
