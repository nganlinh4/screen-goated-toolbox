package dev.screengoated.toolbox.mobile.creation

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable

@Composable
internal fun CreationInputValidationDialog(error: String, language: String, dismiss: () -> Unit, retry: () -> Unit, choose: () -> Unit) {
    val unavailable = error == "validation_unavailable"
    val title = when (language) { "vi" -> "Kiểm tra ảnh"; "ko" -> "이미지 확인"; else -> "Check image" }
    val body = when (language) {
        "vi" -> when (error) { "image_too_small" -> "Ảnh quá nhỏ cho chế độ Nhanh (tối thiểu 32 × 32)."; "image_too_large" -> "Ảnh vượt quá giới hạn dung lượng của chế độ này."; "validation_unavailable" -> "Chưa thể kiểm tra ảnh. Hãy thử lại."; else -> "Không thể đọc ảnh này. Hãy chọn ảnh khác." }
        "ko" -> when (error) { "image_too_small" -> "빠른 모드에는 32 × 32 이상의 이미지가 필요합니다."; "image_too_large" -> "이미지가 이 모드의 파일 크기 제한을 초과합니다."; "validation_unavailable" -> "이미지를 확인할 수 없습니다. 다시 시도하세요."; else -> "이미지를 읽을 수 없습니다. 다른 이미지를 선택하세요." }
        else -> when (error) { "image_too_small" -> "Fast mode requires an image at least 32 × 32 pixels."; "image_too_large" -> "The image exceeds this mode’s file size limit."; "validation_unavailable" -> "The image could not be checked. Try again."; else -> "This image could not be read. Choose another image." }
    }
    val action = when (language) { "vi" -> if (unavailable) "Thử lại" else "Chọn ảnh khác"; "ko" -> if (unavailable) "다시 시도" else "다른 이미지 선택"; else -> if (unavailable) "Retry" else "Choose another image" }
    val close = when (language) { "vi" -> "Đóng"; "ko" -> "닫기"; else -> "Close" }
    AlertDialog(onDismissRequest = dismiss, title = { Text(title) }, text = { Text(body) },
        confirmButton = { TextButton(onClick = if (unavailable) retry else choose) { Text(action) } },
        dismissButton = { TextButton(onClick = dismiss) { Text(close) } })
}
