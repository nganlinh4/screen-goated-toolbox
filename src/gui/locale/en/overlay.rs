use crate::gui::locale::OverlayLocaleText;

pub(super) fn get() -> OverlayLocaleText {
    OverlayLocaleText {
        overlay_copy_tooltip: "Copy",
        overlay_undo_tooltip: "Undo",
        overlay_redo_tooltip: "Redo",
        overlay_edit_tooltip: "Edit / Refine",
        overlay_refine_placeholder: "Refine result...",
        overlay_markdown_tooltip: "Toggle Markdown",
        overlay_download_tooltip: "Save HTML",
        overlay_speaker_tooltip: "Speak (TTS)",
        overlay_broom_tooltip: "Broom: Left - Close | Right - Close Group | Middle - Close All | Drag - Move | Right-Drag - Move Group | Middle-Drag - Move All",
        overlay_back_tooltip: "Back",
        overlay_forward_tooltip: "Forward",
        overlay_opacity_tooltip: "Opacity",
        overlay_cancel_tooltip: "Cancel",
        text_input_close_tooltip: "Close",
        text_input_speech_to_text_tooltip: "Speech to text",
        text_input_send_tooltip: "Send",
        preset_wheel_cancel: "CANCEL",
        select_region_badge: "Select screen region",
        unlimited_label: "∞ Unlimited",
        history_clear_search_tooltip: "Clear search",
        history_open_media_folder_tooltip: "Open Media Folder",
        history_delete_tooltip: "Delete",
        history_copy_text_tooltip: "Copy Text",
    }
}
