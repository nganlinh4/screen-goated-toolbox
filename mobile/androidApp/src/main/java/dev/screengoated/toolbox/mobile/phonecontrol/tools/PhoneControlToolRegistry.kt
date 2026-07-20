package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState

internal enum class PhoneControlHandler(
    val mutating: Boolean,
) {
    OBSERVE(false),
    ACT(true),
    DO_STEPS(true),
    CLICK_AT(true),
    ZOOM(false),
    RESET_VIEW(false),
    SEE_WHOLE_SCREEN(false),
    LOOK(false),
    CLICK_TARGET(true),
    MAP_TARGETS(false),
    CLICK_MARK(true),
    WAIT(false),
    TYPE_TEXT(true),
    SCROLL(true),
    DRAG(true),
    DRAG_TARGET(true),
    KEY_COMBINATION(true),
    OPEN_URL(true),
    LAUNCH_APP(true),
    SYSTEM_QUERY(false),
    LIST_FILES(false),
    READ_TEXT_FILE(false),
    EDIT_TEXT_FILE(true),
    RUN_COMMAND(true),
    FOCUS_WINDOW(true),
    LIST_WINDOWS(false),
    MINIMIZE_WINDOW(true),
    RESIZE_WINDOW(true),
    MOVE_WINDOW(true),
    READ_CLIPBOARD(false),
    ARTIFACT_INFO(false),
    EXTRACT_ARTIFACT(true),
    SAVE_ARTIFACT(true),
    PASTE_ARTIFACT(true),
    SEARCH_MEMORY(false),
    OPEN_MEMORY(false),
    BROWSER_SETUP(false),
    BROWSER_STATUS(false),
    BROWSER_READ_PAGE(false),
    BROWSER_EXTRACT_PAGE(false),
    BROWSER_NAVIGATE(true),
    BROWSER_HISTORY(true),
    DONE(false),
}

internal data class PhoneControlToolSpec(
    val name: String,
    val capability: String,
    /** Primary receipt providers this exact handler may select, in capability-route order. */
    val providerIds: List<String>,
    /** Non-effectful prerequisites allowed to return a typed dependency receipt. */
    val dependencyProviderIds: Set<String> = emptySet(),
    val unavailableState: CapabilityState,
    val requiredUserStep: String? = null,
    val handler: PhoneControlHandler? = null,
) {
    init {
        require(name.isNotBlank())
        require(capability.isNotBlank())
        require(providerIds.isNotEmpty())
        require(providerIds.none(String::isBlank))
        require(providerIds.distinct().size == providerIds.size)
        require(dependencyProviderIds.none(String::isBlank))
        require(dependencyProviderIds.intersect(providerIds.toSet()).isEmpty())
    }

    val requiresMutationAcknowledgement: Boolean
        get() = handler?.mutating == true
}

/**
 * Execution metadata only. Declarations and parameter schemas remain owned by
 * the generated canonical catalog; the parity test prevents name drift.
 */
internal object PhoneControlToolRegistry {
    val specs: List<PhoneControlToolSpec> = listOf(
        real("observe", "ui.semantic_observe", "accessibility", PhoneControlHandler.OBSERVE),
        real("act", "ui.pointer_action", "accessibility", PhoneControlHandler.ACT),
        real("do_steps", "ui.pointer_action", "accessibility", PhoneControlHandler.DO_STEPS),
        real("click_at", "ui.pointer_action", "accessibility", PhoneControlHandler.CLICK_AT),
        real("zoom", "ui.visual_observe", "accessibility", PhoneControlHandler.ZOOM),
        real("reset_view", "ui.visual_observe", "accessibility", PhoneControlHandler.RESET_VIEW),
        real(
            "see_whole_screen",
            "ui.visual_observe",
            "accessibility",
            PhoneControlHandler.SEE_WHOLE_SCREEN,
        ),
        real("look", "ui.visual_observe", "accessibility", PhoneControlHandler.LOOK),
        real("click_target", "ui.pointer_action", "local_ui_detector", PhoneControlHandler.CLICK_TARGET),
        real("map_targets", "blind_surface_grounding", "local_ui_detector", PhoneControlHandler.MAP_TARGETS),
        real("click_mark", "ui.pointer_action", "local_ui_detector", PhoneControlHandler.CLICK_MARK),
        real("wait", "local_completion_and_cleanup", "android_app_api", PhoneControlHandler.WAIT),
        realWithProviders(
            "type_text",
            "ui.text_edit",
            listOf("accessibility", "accessibility_input_method"),
            PhoneControlHandler.TYPE_TEXT,
        ),
        real("scroll", "ui.pointer_action", "accessibility", PhoneControlHandler.SCROLL),
        real("drag", "ui.pointer_action", "accessibility", PhoneControlHandler.DRAG),
        real("drag_target", "ui.pointer_action", "local_ui_detector", PhoneControlHandler.DRAG_TARGET),
        unsupported("click_here", "ui.pointer_action", "accessibility"),
        unavailable("point_at", "ui.pointer_action", "local_ui_detector"),
        realWithProviders(
            "key_combination",
            "ui.key_action",
            listOf("accessibility", "accessibility_input_method"),
            PhoneControlHandler.KEY_COMBINATION,
        ),
        real(
            "open_url",
            "browser_authenticated_navigation",
            "android_app_api",
            PhoneControlHandler.OPEN_URL,
        ),
        real("launch_app", "app_and_task_control", "android_app_api", PhoneControlHandler.LAUNCH_APP),
        realWithProviders(
            "system_query",
            "system_query",
            listOf("android_app_api", "accessibility"),
            PhoneControlHandler.SYSTEM_QUERY,
        ),
        real("list_files", "file_resource_access", "android_app_api", PhoneControlHandler.LIST_FILES),
        real(
            "read_text_file",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.READ_TEXT_FILE,
        ),
        real(
            "edit_text_file",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.EDIT_TEXT_FILE,
        ),
        unavailable("edit_text_file_structure", "file_resource_access", "android_app_api"),
        realWithProviders(
            "run_command",
            "command_execution",
            listOf("shizuku_shell", "root_bridge"),
            PhoneControlHandler.RUN_COMMAND,
        ),
        realWithProviders(
            "focus_window",
            "app_and_task_control",
            listOf("android_app_api", "accessibility"),
            PhoneControlHandler.FOCUS_WINDOW,
        ),
        real("list_windows", "app_and_task_control", "accessibility", PhoneControlHandler.LIST_WINDOWS),
        real(
            "minimize_window",
            "app_and_task_control",
            "accessibility",
            PhoneControlHandler.MINIMIZE_WINDOW,
        ),
        real(
            "resize_window",
            "app_and_task_control",
            "privileged_system",
            PhoneControlHandler.RESIZE_WINDOW,
        ),
        real(
            "move_window",
            "app_and_task_control",
            "privileged_system",
            PhoneControlHandler.MOVE_WINDOW,
        ),
        real("read_clipboard", "system_query", "accessibility", PhoneControlHandler.READ_CLIPBOARD),
        real(
            "artifact_info",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.ARTIFACT_INFO,
        ),
        real(
            "extract_artifact",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.EXTRACT_ARTIFACT,
        ),
        real(
            "save_artifact",
            "file_resource_access",
            "android_app_api",
            PhoneControlHandler.SAVE_ARTIFACT,
        ),
        realWithProviders(
            "paste_artifact",
            "ui.text_edit",
            listOf("accessibility", "accessibility_input_method"),
            PhoneControlHandler.PASTE_ARTIFACT,
            dependencyProviders = setOf("android_app_api"),
        ),
        real("done", "local_completion_and_cleanup", "android_app_api", PhoneControlHandler.DONE),
        real("search_memory", "system_query", "android_app_api", PhoneControlHandler.SEARCH_MEMORY),
        real("open_memory", "system_query", "android_app_api", PhoneControlHandler.OPEN_MEMORY),
        realWithProviders(
            "browser_setup",
            "browser_authenticated_navigation",
            listOf("custom_tabs_session"),
            PhoneControlHandler.BROWSER_SETUP,
            dependencyProviders = setOf("accessibility"),
        ),
        realWithProviders(
            "browser_status",
            "browser_semantic",
            listOf("accessibility"),
            PhoneControlHandler.BROWSER_STATUS,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        browserUnavailable("browser_reset", "browser_devtools"),
        realWithProviders(
            "browser_read_page",
            "browser_semantic",
            listOf("accessibility"),
            PhoneControlHandler.BROWSER_READ_PAGE,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        browserUnavailable("research_web", "browser_semantic"),
        realWithProviders(
            "browser_extract_page",
            "browser_semantic",
            listOf("accessibility"),
            PhoneControlHandler.BROWSER_EXTRACT_PAGE,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        browserUnavailable("browser_wait_for", "browser_semantic"),
        browserUnavailable("browser_eval", "browser_devtools"),
        realWithProviders(
            "browser_navigate",
            "browser_authenticated_navigation",
            listOf("custom_tabs_session"),
            PhoneControlHandler.BROWSER_NAVIGATE,
            dependencyProviders = setOf("accessibility"),
        ),
        realWithProviders(
            "browser_history",
            "browser_authenticated_navigation",
            listOf("accessibility"),
            PhoneControlHandler.BROWSER_HISTORY,
            dependencyProviders = setOf("custom_tabs_session"),
        ),
        browserUnavailable("browser_open_tab", "browser_devtools"),
        browserUnavailable("browser_upload", "browser_devtools"),
        browserUnavailable("browser_tabs", "browser_devtools"),
        browserUnavailable("browser_switch_tab", "browser_devtools"),
        browserUnavailable("browser_close_tab", "browser_devtools"),
        browserUnavailable("browser_network", "browser_devtools"),
        browserUnavailable("browser_console", "browser_devtools"),
        unavailable("list_app_integrations", "system_query", "android_app_api"),
        unavailable("setup_app_integration", "app_and_task_control", "android_app_api"),
        unavailable("app_integration_status", "system_query", "android_app_api"),
        unavailable("read_app_integration_docs", "system_query", "android_app_api"),
        unavailable("remove_app_integration", "app_and_task_control", "android_app_api"),
    )

    val byName: Map<String, PhoneControlToolSpec> = specs.associateBy(PhoneControlToolSpec::name)

    init {
        require(byName.size == specs.size) { "Phone Control tool registry names must be unique" }
    }

    private fun real(
        name: String,
        capability: String,
        provider: String,
        handler: PhoneControlHandler,
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = listOf(provider),
        unavailableState = CapabilityState.UNAVAILABLE,
        handler = handler,
    )

    private fun realWithProviders(
        name: String,
        capability: String,
        providers: List<String>,
        handler: PhoneControlHandler,
        dependencyProviders: Set<String> = emptySet(),
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = providers,
        dependencyProviderIds = dependencyProviders,
        unavailableState = CapabilityState.UNAVAILABLE,
        handler = handler,
    )

    private fun unavailable(
        name: String,
        capability: String,
        provider: String,
        requiredUserStep: String? = null,
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = listOf(provider),
        unavailableState = CapabilityState.UNAVAILABLE,
        requiredUserStep = requiredUserStep,
    )

    private fun unsupported(
        name: String,
        capability: String,
        provider: String,
    ) = PhoneControlToolSpec(
        name = name,
        capability = capability,
        providerIds = listOf(provider),
        unavailableState = CapabilityState.UNSUPPORTED,
    )

    private fun browserUnavailable(
        name: String,
        capability: String,
    ) = unavailable(
        name = name,
        capability = capability,
        provider = "browser_cdp",
        requiredUserStep = "configure_browser_control",
    )
}
