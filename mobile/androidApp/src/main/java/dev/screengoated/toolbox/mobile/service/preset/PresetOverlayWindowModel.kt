package dev.screengoated.toolbox.mobile.service.preset

internal data class PresetOverlayWindowSpec(
    val width: Int,
    val height: Int,
    val x: Int,
    val y: Int,
    val focusable: Boolean,
    val showImeOnFocus: Boolean = false,
    val watchOutsideTouch: Boolean = false,
    val assetPage: String? = null,
    val htmlContent: String? = null,
    val baseUrl: String = "file:///android_asset/preset_overlay/",
    val clipToOutline: Boolean = true,
    val touchRegionsOnly: Boolean = false,
    val resultWindowId: String? = null,
    val resultIsDark: Boolean = false,
    val resultCapabilityMessage: String = "",
)

internal data class OverlayNavigationFailure(val url: String?, val description: String)

internal sealed interface OverlayRecoverySource {
    data class Asset(val assetPage: String) : OverlayRecoverySource
    data class Html(val htmlContent: String, val baseUrl: String, val untrusted: Boolean, val audioPreview: ByteArray? = null) : OverlayRecoverySource
}

internal data class OverlayHistoryState(val currentIndex: Int, val lastIndex: Int, val currentUrl: String?)
