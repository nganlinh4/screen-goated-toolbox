package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.whisperSpeechLanguages
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

@Composable
internal fun SpeechLanguagePicker(block: ProcessingBlock, language: String, update: (ProcessingBlock) -> Unit) {
    if (PresetModelCatalog.getById(block.model)?.inputLanguageSet != "whisper") return
    var open by remember { mutableStateOf(false) }
    var search by remember { mutableStateOf("") }
    val title = when (language) { "vi" -> "Ngôn ngữ đầu vào"; "ko" -> "입력 언어"; else -> "Input language" }
    val automatic = when (language) { "vi" -> "Tự động"; "ko" -> "자동"; else -> "Auto" }
    val close = when (language) { "vi" -> "Đóng"; "ko" -> "닫기"; else -> "Close" }
    val options = listOf("auto" to automatic) + whisperSpeechLanguages
    val selected = options.firstOrNull { it.first == block.languageVars["input_language"] }?.second ?: automatic
    OutlinedButton(onClick = { search = ""; open = true }) { Text("$title: $selected") }
    if (open) AlertDialog(
        onDismissRequest = { open = false },
        title = { Text(title) },
        confirmButton = { TextButton(onClick = { open = false }) { Text(close) } },
        text = {
            Column {
                OutlinedTextField(search, { search = it }, label = { Text(nodeGraphLanguageSearchPlaceholder(language)) })
                LazyColumn(Modifier.heightIn(max = 320.dp)) {
                    items(options.filter { it.first.contains(search, true) || it.second.contains(search, true) }, key = { it.first }) { option ->
                        Text(option.second, Modifier.fillMaxWidth().clickable {
                            update(block.copy(languageVars = block.languageVars + ("input_language" to option.first)))
                            open = false
                        }.padding(12.dp))
                    }
                }
            }
        },
    )
}
