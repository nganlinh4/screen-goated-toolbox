@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.ApiKeys
import dev.screengoated.toolbox.mobile.preset.GeneratedPresetModelCatalogData
import dev.screengoated.toolbox.mobile.preset.PresetAdaptiveModelPriority
import dev.screengoated.toolbox.mobile.preset.PresetLiveModelOverrides
import dev.screengoated.toolbox.mobile.preset.PresetModelPriorityChains
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.PresetProviderSettings
import dev.screengoated.toolbox.mobile.preset.PresetRetryChainKind
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
fun PresetRuntimeSettingsDialog(
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
    locale: MobileLocaleText,
    uiLanguage: String = "en",
    onDismiss: () -> Unit,
    onSave: (PresetRuntimeSettings) -> Unit,
) {
    var imageChain by remember(settings) { mutableStateOf(settings.modelPriorityChains.imageToText) }
    var textChain by remember(settings) { mutableStateOf(settings.modelPriorityChains.textToText) }
    var adaptive by remember(settings) { mutableStateOf(settings.adaptiveModelPriority) }
    var providers by remember(settings) { mutableStateOf(settings.providerSettings) }
    var showHelpDialog by remember { mutableStateOf(false) }

    if (showHelpDialog) {
        ExpressiveDialogSurface(
            title = locale.presetRuntimeTitle,
            icon = R.drawable.ms_info,
            accent = MaterialTheme.colorScheme.primary,
            morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie4Sided),
            onDismiss = { showHelpDialog = false },
            fitContentHeight = true,
            maxWidth = 460.dp,
        ) {
            ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.primary) {
                Text(
                    text = locale.presetRuntimeDescription,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }

    fun save(
        image: List<String> = imageChain,
        text: List<String> = textChain,
        adaptivePriority: PresetAdaptiveModelPriority = adaptive,
        providerSettings: PresetProviderSettings = providers,
    ) {
        onSave(
            settings.copy(
                modelPriorityChains = PresetModelPriorityChains(image, text),
                adaptiveModelPriority = adaptivePriority,
                providerSettings = providerSettings,
            ),
        )
    }

    fun updateProvider(provider: CredentialsProviderId) {
        val next = providers.withEnabled(provider, !providers.isEnabled(provider))
        providers = next
        save(providerSettings = next)
    }

    fun updateImage(
        chain: List<String>,
        enabled: Boolean,
        overrides: PresetLiveModelOverrides,
    ) {
        val next = adaptive.copy(
            imageToText = enabled,
            imageToTextOverrides = overrides,
        )
        imageChain = chain
        adaptive = next
        save(image = chain, adaptivePriority = next)
    }

    fun updateText(
        chain: List<String>,
        enabled: Boolean,
        overrides: PresetLiveModelOverrides,
    ) {
        val next = adaptive.copy(
            textToText = enabled,
            textToTextOverrides = overrides,
        )
        textChain = chain
        adaptive = next
        save(text = chain, adaptivePriority = next)
    }

    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    val windowWidth = with(density) { windowInfo.containerSize.width.toDp() }
    val windowHeight = with(density) { windowInfo.containerSize.height.toDp() }
    val isLandscape = windowWidth > windowHeight
    val imageEditor: @Composable () -> Unit = {
        PriorityChainEditor(
            title = locale.presetRuntimeImageChainLabel,
            authoredChain = imageChain,
            chainKind = PresetRetryChainKind.IMAGE_TO_TEXT,
            modelType = PresetModelType.VISION,
            defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.imageToText,
            adaptiveEnabled = adaptive.imageToText,
            overrides = adaptive.imageToTextOverrides,
            settings = settings.copy(
                providerSettings = providers,
                modelPriorityChains = PresetModelPriorityChains(imageChain, textChain),
                adaptiveModelPriority = adaptive,
            ),
            apiKeys = apiKeys,
            locale = locale,
            uiLanguage = uiLanguage,
            accent = MaterialTheme.colorScheme.primary,
            onStateChanged = ::updateImage,
        )
    }
    val textEditor: @Composable () -> Unit = {
        PriorityChainEditor(
            title = locale.presetRuntimeTextChainLabel,
            authoredChain = textChain,
            chainKind = PresetRetryChainKind.TEXT_TO_TEXT,
            modelType = PresetModelType.TEXT,
            defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.textToText,
            adaptiveEnabled = adaptive.textToText,
            overrides = adaptive.textToTextOverrides,
            settings = settings.copy(
                providerSettings = providers,
                modelPriorityChains = PresetModelPriorityChains(imageChain, textChain),
                adaptiveModelPriority = adaptive,
            ),
            apiKeys = apiKeys,
            locale = locale,
            uiLanguage = uiLanguage,
            accent = MaterialTheme.colorScheme.secondary,
            onStateChanged = ::updateText,
        )
    }

    ExpressiveDialogSurface(
        title = locale.presetRuntimeTitle,
        icon = R.drawable.ms_settings,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(MaterialShapes.Square, MaterialShapes.Cookie6Sided),
        onDismiss = onDismiss,
        widthFraction = if (isLandscape) 0.92f else 0.96f,
        maxWidth = if (isLandscape) 900.dp else 520.dp,
        heightFraction = 0.88f,
        maxHeight = 760.dp,
        fitContentHeight = true,
        headerTrailing = {
            IconButton(onClick = { showHelpDialog = true }) {
                Icon(
                    painterResource(R.drawable.ms_info),
                    contentDescription = locale.presetRuntimeDescription,
                )
            }
        },
    ) {
        Text(
            text = locale.presetRuntimeProvidersLabel,
            style = MaterialTheme.typography.labelLarge,
        )
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            credentialsProviderOrder().forEach { provider ->
                FilterChip(
                    selected = providers.isEnabled(provider),
                    onClick = { updateProvider(provider) },
                    label = { Text(provider.label) },
                )
            }
        }
        if (isLandscape) {
            Row(
                modifier = Modifier.fillMaxWidth().heightIn(max = 520.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) { imageEditor() }
                Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) { textEditor() }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 560.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                imageEditor()
                textEditor()
            }
        }
    }
}

internal fun PresetProviderSettings.isEnabled(provider: CredentialsProviderId): Boolean =
    when (provider) {
        CredentialsProviderId.GROQ -> useGroq
        CredentialsProviderId.GEMINI -> useGemini
        CredentialsProviderId.OPEN_ROUTER -> useOpenRouter
        CredentialsProviderId.NVIDIA -> useNvidia
        CredentialsProviderId.OLLAMA -> useOllama
    }

internal fun PresetProviderSettings.withEnabled(
    provider: CredentialsProviderId,
    enabled: Boolean,
): PresetProviderSettings = when (provider) {
    CredentialsProviderId.GROQ -> copy(useGroq = enabled)
    CredentialsProviderId.GEMINI -> copy(useGemini = enabled)
    CredentialsProviderId.OPEN_ROUTER -> copy(useOpenRouter = enabled)
    CredentialsProviderId.NVIDIA -> copy(useNvidia = enabled)
    CredentialsProviderId.OLLAMA -> copy(useOllama = enabled)
}
