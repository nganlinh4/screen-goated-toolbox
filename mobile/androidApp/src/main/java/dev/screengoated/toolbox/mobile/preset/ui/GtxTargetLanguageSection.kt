@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

private fun l(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

@Composable
internal fun GtxTargetLanguageSection(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    LaunchedEffect(block.id, block.model) {
        if (block.languageVars["language1"].isNullOrBlank()) {
            onUpdate(block.copy(languageVars = block.languageVars + ("language1" to "Vietnamese")))
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                painterResource(R.drawable.ms_language),
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.tertiary,
            )
            Spacer(Modifier.width(6.dp))
            SheetLabel(l(lang, "Target language", "Ngôn ngữ đích", "대상 언어"))
        }

        TargetLanguageRow(
            value = block.languageVars["language1"] ?: "Vietnamese",
            lang = lang,
            onValueChanged = { newValue ->
                onUpdate(block.copy(languageVars = block.languageVars + ("language1" to newValue)))
            },
        )
    }
}

@Composable
private fun TargetLanguageRow(
    value: String,
    lang: String,
    onValueChanged: (String) -> Unit,
) {
    var showPicker by remember { mutableStateOf(false) }
    var searchQuery by remember { mutableStateOf("") }

    Column {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .clickable { showPicker = !showPicker },
            color = MaterialTheme.colorScheme.tertiaryContainer.copy(alpha = 0.3f),
            shape = RoundedCornerShape(8.dp),
        ) {
            Text(
                value,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                style = MaterialTheme.typography.bodySmall,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }

        AnimatedVisibility(
            visible = showPicker,
            enter = fadeIn() + expandVertically(),
            exit = fadeOut() + shrinkVertically(),
        ) {
            Column(
                modifier = Modifier.padding(top = 6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = {
                        Text(l(lang, "Search languages...", "Tìm ngôn ngữ...", "언어 검색..."))
                    },
                    leadingIcon = {
                        Icon(
                            painterResource(R.drawable.ms_search),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                    },
                )

                val filtered = remember(searchQuery) {
                    if (searchQuery.isBlank()) {
                        LanguageCatalog.names
                    } else {
                        LanguageCatalog.names.filter { it.contains(searchQuery, ignoreCase = true) }
                    }
                }

                LazyColumn(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = 200.dp),
                ) {
                    items(filtered, key = { it }) { name ->
                        val isSelected = name == value
                        Surface(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(6.dp))
                                .clickable {
                                    onValueChanged(name)
                                    showPicker = false
                                    searchQuery = ""
                                },
                            color = if (isSelected) {
                                MaterialTheme.colorScheme.tertiaryContainer
                            } else {
                                Color.Transparent
                            },
                            shape = RoundedCornerShape(6.dp),
                        ) {
                            Text(
                                name,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                        }
                    }
                }
            }
        }
    }
}
