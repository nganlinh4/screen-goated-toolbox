use crate::gui::locale::WorkspaceLocaleText;

pub(super) fn get() -> WorkspaceLocaleText {
    WorkspaceLocaleText {
        history_btn: "History",
        history_title: "Result Library",
        max_items_label: "Max Items:",
        cc_memory_max_label: "CC Memory:",
        history_empty: "No history yet.",
        clear_all_history_btn: "Clear All",
        view_image_btn: "View Image",
        listen_audio_btn: "Listen Audio",
        view_text_btn: "View Text",
        tips_title: "Usage Tips",
        tips_list: super::super::tips::en(),
        tips_click_hint: "Click text to view tip list",
        restore_preset_btn: "Restore",
        restore_preset_tooltip: "Reset preset to default settings",
        search_doing: "Running",
        search_searching: "searching",
        search_query_label: "Search queries:",
        search_found_sources: "FOUND {} SOURCES",
        search_sources_label: "Reference sources (by relevance):",
        search_no_title: "(No title)",
        search_synthesizing: "SYNTHESIZING INFO...",
        search_analyzed_sources: "Analyzed {} sources",
        search_processing: "Processing and summarizing results...",
    }
}
