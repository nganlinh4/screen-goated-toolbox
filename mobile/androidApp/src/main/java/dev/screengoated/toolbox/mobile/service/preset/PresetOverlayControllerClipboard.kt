package dev.screengoated.toolbox.mobile.service.preset

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Rect
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import android.widget.Toast
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchRequest
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.resolvePrompt
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.service.ScreenshotCaptureFailureReason
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.service.LiveTranslateService
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import java.io.File
import kotlin.math.roundToInt

// Clipboard + accessibility-disclosure helpers extracted from PresetOverlayController.
private const val TAG = "PresetOverlayController"
private const val IMAGE_CLIPBOARD_DIR = "clipboard-images"
private const val IMAGE_CLIPBOARD_FILE = "latest-screenshot.png"

internal fun PresetOverlayController.copyColorToClipboard(hexColor: String) {
    clipboardManager?.setPrimaryClip(ClipData.newPlainText("SGT Color", hexColor))
    Toast.makeText(
        context,
        localized(
            "Copied $hexColor",
            "Đã sao chép $hexColor",
            "$hexColor 복사됨",
        ),
        Toast.LENGTH_SHORT,
    ).show()
}

internal fun PresetOverlayController.copyTextToClipboard(text: String): Boolean {
    return runCatching {
        val manager = clipboardManager ?: return false
        manager.setPrimaryClip(ClipData.newPlainText("SGT Result", text))
        true
    }.getOrElse { error ->
        Log.e(TAG, "copyTextToClipboard failed", error)
        false
    }
}

internal fun PresetOverlayController.copyImageToClipboard(pngBytes: ByteArray): Boolean {
    return runCatching {
        val manager = clipboardManager ?: return false
        val dir = File(context.cacheDir, IMAGE_CLIPBOARD_DIR).apply { mkdirs() }
        val file = File(dir, IMAGE_CLIPBOARD_FILE)
        file.writeBytes(pngBytes)
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
        manager.setPrimaryClip(ClipData.newUri(context.contentResolver, "SGT Image", uri))
        true
    }.getOrElse { error ->
        Log.e(TAG, "copyImageToClipboard failed", error)
        false
    }
}

internal fun PresetOverlayController.openAccessibilitySettings() {
    try {
        val intent = android.content.Intent(android.provider.Settings.ACTION_ACCESSIBILITY_SETTINGS)
        intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)
    } catch (_: Exception) {
    }
}

/**
 * Prominent disclosure shown before sending the user to Accessibility
 * settings. Explains why the app uses the AccessibilityService API and
 * what data it reads, then opens Settings only after the user agrees.
 */
internal fun PresetOverlayController.promptAccessibilityDisclosure() {
    accessibilityDisclosure.show(
        themeMode = uiPreferencesProvider().themeMode,
        strings = AccessibilityDisclosureStrings(
            title = localized(
                "Enable Accessibility access",
                "Bật quyền Trợ năng",
                "접근성 권한 사용",
            ),
            body = localized(
                "Screen Goated Toolbox uses Android's Accessibility service to run " +
                    "Text-Select presets and auto-paste. It reads the text you select in " +
                    "other apps, can capture the screen, and pastes processed results back " +
                    "into the active field. This content is sent only to the AI provider you " +
                    "configure, to fulfil your request — it is not collected on our own " +
                    "servers. Enable the service to continue?",
                "Screen Goated Toolbox dùng dịch vụ Trợ năng của Android để chạy " +
                    "preset Chọn văn bản và tự động dán. Ứng dụng đọc văn bản bạn " +
                    "bôi đen trong app khác, có thể chụp màn hình, và dán kết quả đã " +
                    "xử lý vào ô đang chọn. Nội dung này chỉ được gửi tới nhà cung " +
                    "cấp AI bạn đã cấu hình để thực hiện yêu cầu — không thu thập trên " +
                    "máy chủ của chúng tôi. Bật dịch vụ để tiếp tục?",
                "Screen Goated Toolbox는 텍스트 선택 프리셋과 자동 붙여넣기를 " +
                    "실행하기 위해 Android 접근성 서비스를 사용합니다. 다른 앱에서 " +
                    "선택한 텍스트를 읽고, 화면을 캡처할 수 있으며, 처리된 결과를 " +
                    "활성 입력란에 붙여넣습니다. 이 콘텐츠는 요청을 처리하기 위해 " +
                    "설정한 AI 제공자에게만 전송되며, 당사 서버에는 수집하지 " +
                    "않습니다. 계속하려면 서비스를 사용 설정하시겠어요?",
            ),
            agree = localized(
                "Agree & open settings",
                "Đồng ý & mở Cài đặt",
                "동의하고 설정 열기",
            ),
            cancel = localized(
                "Not now",
                "Để sau",
                "나중에",
            ),
        ),
        onAgree = { openAccessibilitySettings() },
    )
}

