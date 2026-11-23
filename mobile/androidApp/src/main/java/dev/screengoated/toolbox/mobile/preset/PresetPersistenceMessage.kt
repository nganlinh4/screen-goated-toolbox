package dev.screengoated.toolbox.mobile.preset

fun presetPersistenceMessage(language: String, failure: PresetPersistenceFailure): String = when (failure) {
    PresetPersistenceFailure.READ -> when (language) {
        "vi" -> "Không thể đọc cài đặt mẫu. Dữ liệu được giữ lại; hãy khắc phục lỗi lưu trữ và khởi động lại trước khi chỉnh sửa."
        "ko" -> "저장된 프리셋을 읽을 수 없습니다. 데이터는 보존되며, 저장소 문제를 해결하고 다시 시작한 후 편집하세요."
        else -> "Saved presets could not be read. Their data is preserved; resolve the storage problem and restart before editing."
    }
    PresetPersistenceFailure.WRITE -> when (language) {
        "vi" -> "Không thể lưu thay đổi mẫu. Dữ liệu đã lưu trước đó được giữ lại."
        "ko" -> "프리셋 변경 사항을 저장할 수 없습니다. 이전에 저장된 데이터는 보존됩니다."
        else -> "Preset changes could not be saved. Previously saved data was preserved."
    }
}
