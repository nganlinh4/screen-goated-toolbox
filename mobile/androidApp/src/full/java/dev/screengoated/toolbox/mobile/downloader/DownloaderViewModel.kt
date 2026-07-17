package dev.screengoated.toolbox.mobile.downloader

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.flow.StateFlow

class DownloaderViewModel(
    private val repository: DownloaderRepository,
) : ViewModel() {

    val state: StateFlow<DownloaderUiState> = repository.state

    init {
        repository.checkTools() // fast file check, no init()
    }

    // Tool management
    fun installTools() = repository.installTools()
    fun deleteTools() = repository.deleteTools()
    fun checkUpdates() = repository.checkUpdates()
    fun totalDepsSize() = repository.calculateTotalDepsSize()

    // Multi-tab
    fun addTab() = repository.addTab()
    fun closeTab(idx: Int) = repository.closeTab(idx)
    fun switchTab(idx: Int) = repository.switchTab(idx)

    // URL & analysis
    fun updateUrl(url: String) = repository.updateUrl(url)

    // Download config
    fun setDownloadType(type: DownloadType) = repository.setDownloadType(type)
    fun setFormat(format: String?) = repository.setFormat(format)
    fun setSubtitle(subtitle: String?) = repository.setSubtitle(subtitle)

    // Download actions
    fun startDownload() = repository.startDownload()
    fun cancelDownload() = repository.cancelDownload()
    fun resetSession() = repository.resetSession()
    fun toggleErrorLog() = repository.toggleErrorLog()

    // Settings
    fun updateSettings(transform: (DownloaderSettings) -> DownloaderSettings) =
        repository.updateSettings(transform)
    fun setDownloadPath(path: String?) = repository.setDownloadPath(path)
    fun getDownloadDir() = repository.getDownloadDir()

    companion object {
        fun factory(repository: DownloaderRepository): ViewModelProvider.Factory {
            return object : ViewModelProvider.Factory {
                @Suppress("UNCHECKED_CAST")
                override fun <T : ViewModel> create(modelClass: Class<T>): T {
                    return DownloaderViewModel(repository) as T
                }
            }
        }
    }
}
