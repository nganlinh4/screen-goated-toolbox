package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class CreationMiniAppActivity : ComponentActivity() {
    private lateinit var tool: CreationTool
    private lateinit var viewModel: CreationNativeViewModel

    private val imagePicker = registerForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris ->
        Log.d("CreationImageImport", "Picker returned ${uris.size} image(s)")
        if (uris.isEmpty()) return@registerForActivityResult
        lifecycleScope.launch {
            runCatching {
                val maximum = if (tool == CreationTool.IMAGE_CREATOR) {
                    val configurableReferences = viewModel.state.value.selectedItem
                        ?.takeIf {
                            !it.submitted && it.stage == CreationNativeStage.DRAFT
                        }
                        ?.referencePaths
                        ?.size
                        ?: 0
                    CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES -
                        configurableReferences
                } else {
                    CreationContract.MAXIMUM_PICKER_BATCH_IMAGES
                }
                val existingReferences = if (tool == CreationTool.IMAGE_CREATOR) {
                    viewModel.state.value.selectedItem
                        ?.takeIf {
                            !it.submitted && it.stage == CreationNativeStage.DRAFT
                        }
                        ?.referencePaths
                        .orEmpty()
                } else {
                    emptyList()
                }
                withContext(Dispatchers.IO) {
                    CreationJobManager.get(this@CreationMiniAppActivity).files.importImages(
                        uris,
                        tool,
                        maximum,
                        existingReferences,
                    )
                }
            }.onSuccess { paths ->
                Log.d("CreationImageImport", "Imported ${paths.size} image(s)")
                viewModel.addImages(paths)
            }.onFailure { error ->
                Log.e("CreationImageImport", "Image import failed", error)
                viewModel.showError(error)
            }
        }
    }

    private val outputPicker = registerForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        uri?.let(viewModel::rememberOutputDirectory)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        tool = CreationTool.fromWireName(intent.getStringExtra(EXTRA_TOOL)) ?: run {
            finish()
            return
        }
        if (!creationToolReleased(tool)) {
            finish()
            return
        }
        val ownerId = intent.getStringExtra(EXTRA_OWNER_ID)
            ?.takeIf(String::isNotBlank)
            ?: UUID.randomUUID().toString().also { intent.putExtra(EXTRA_OWNER_ID, it) }
        enableEdgeToEdge()
        viewModel = ViewModelProvider(
            this,
            CreationNativeViewModelFactory(application, tool, ownerId),
        )[CreationNativeViewModel::class.java]
        val preferences = (application as SgtMobileApplication).appContainer.repository
            .currentUiPreferences()
        val locale = MobileLocaleText.forLanguage(preferences.uiLanguage)

        setContent {
            val state = viewModel.state.collectAsStateWithLifecycle().value
            SgtMobileTheme(themeMode = preferences.themeMode) {
                CreationNativeScreen(
                    tool = tool,
                    state = state,
                    locale = locale,
                    viewModel = viewModel,
                    onBack = ::finish,
                    onPickImages = {
                        imagePicker.launch(arrayOf("image/png", "image/jpeg", "image/webp"))
                    },
                    onPickOutputDirectory = { outputPicker.launch(null) },
                )
            }
        }
        onBackPressedDispatcher.addCallback(
            this,
            object : androidx.activity.OnBackPressedCallback(true) {
                override fun handleOnBackPressed() = finish()
            },
        )
    }

    override fun onDestroy() {
        if (isFinishing && ::viewModel.isInitialized) viewModel.closeMiniApp()
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_TOOL = "creation_tool"
        private const val EXTRA_OWNER_ID = "creation_owner_id"

        internal fun intent(context: Context, tool: CreationTool): Intent = Intent(
            context,
            CreationMiniAppActivity::class.java,
        )
            .putExtra(EXTRA_TOOL, tool.wireName)
            .putExtra(EXTRA_OWNER_ID, UUID.randomUUID().toString())
    }
}
