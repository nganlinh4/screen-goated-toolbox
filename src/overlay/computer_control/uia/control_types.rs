//! Shared native control roles and observation capabilities.

use windows::Win32::UI::Accessibility::{
    UIA_ButtonControlTypeId, UIA_CONTROLTYPE_ID, UIA_CheckBoxControlTypeId,
    UIA_ComboBoxControlTypeId, UIA_CustomControlTypeId, UIA_DocumentControlTypeId,
    UIA_EditControlTypeId, UIA_GroupControlTypeId, UIA_HyperlinkControlTypeId,
    UIA_ImageControlTypeId, UIA_ListControlTypeId, UIA_ListItemControlTypeId,
    UIA_MenuBarControlTypeId, UIA_MenuControlTypeId, UIA_MenuItemControlTypeId,
    UIA_PaneControlTypeId, UIA_RadioButtonControlTypeId, UIA_SliderControlTypeId,
    UIA_SplitButtonControlTypeId, UIA_TabControlTypeId, UIA_TabItemControlTypeId,
    UIA_TableControlTypeId, UIA_TextControlTypeId, UIA_TitleBarControlTypeId,
    UIA_ToolBarControlTypeId, UIA_TreeControlTypeId, UIA_TreeItemControlTypeId,
    UIA_WindowControlTypeId,
};

struct ControlType {
    id: UIA_CONTROLTYPE_ID,
    name: &'static str,
    role: Option<&'static str>,
    text_value: bool,
}

const fn control(
    id: UIA_CONTROLTYPE_ID,
    name: &'static str,
    role: Option<&'static str>,
    text_value: bool,
) -> ControlType {
    ControlType {
        id,
        name,
        role,
        text_value,
    }
}

const TYPES: &[ControlType] = &[
    control(UIA_ButtonControlTypeId, "Button", Some("button"), false),
    control(
        UIA_CheckBoxControlTypeId,
        "CheckBox",
        Some("checkbox"),
        false,
    ),
    control(
        UIA_ComboBoxControlTypeId,
        "ComboBox",
        Some("combobox"),
        true,
    ),
    control(UIA_EditControlTypeId, "Edit", Some("textbox"), true),
    control(UIA_HyperlinkControlTypeId, "Hyperlink", Some("link"), false),
    control(UIA_ImageControlTypeId, "Image", None, false),
    control(
        UIA_ListItemControlTypeId,
        "ListItem",
        Some("listitem"),
        false,
    ),
    control(UIA_ListControlTypeId, "List", None, false),
    control(UIA_MenuControlTypeId, "Menu", None, false),
    control(UIA_MenuBarControlTypeId, "MenuBar", None, false),
    control(
        UIA_MenuItemControlTypeId,
        "MenuItem",
        Some("menuitem"),
        false,
    ),
    control(
        UIA_RadioButtonControlTypeId,
        "RadioButton",
        Some("radio"),
        false,
    ),
    control(UIA_SliderControlTypeId, "Slider", Some("slider"), false),
    control(UIA_TabControlTypeId, "Tab", None, false),
    control(UIA_TabItemControlTypeId, "TabItem", Some("tab"), false),
    control(UIA_TextControlTypeId, "Text", None, false),
    control(UIA_ToolBarControlTypeId, "ToolBar", None, false),
    control(UIA_TreeControlTypeId, "Tree", None, false),
    control(
        UIA_TreeItemControlTypeId,
        "TreeItem",
        Some("treeitem"),
        false,
    ),
    control(UIA_CustomControlTypeId, "Custom", None, false),
    control(UIA_GroupControlTypeId, "Group", None, false),
    control(UIA_DocumentControlTypeId, "Document", Some("textbox"), true),
    control(
        UIA_SplitButtonControlTypeId,
        "SplitButton",
        Some("button"),
        false,
    ),
    control(UIA_WindowControlTypeId, "Window", None, false),
    control(UIA_PaneControlTypeId, "Pane", None, false),
    control(UIA_TableControlTypeId, "Table", None, false),
    control(UIA_TitleBarControlTypeId, "TitleBar", None, false),
];

pub(super) fn name_for_id(id: i32) -> &'static str {
    TYPES
        .iter()
        .find(|entry| entry.id.0 == id)
        .map_or("Other", |entry| entry.name)
}

pub(super) fn role_for_id(id: i32) -> Option<&'static str> {
    TYPES.iter().find(|entry| entry.id.0 == id)?.role
}

pub(in crate::overlay::computer_control) fn role_for_name(name: &str) -> Option<&'static str> {
    TYPES.iter().find(|entry| entry.name == name)?.role
}

pub(super) fn is_grounding(id: i32) -> bool {
    id == UIA_TextControlTypeId.0 || role_for_id(id).is_some()
}

pub(super) fn has_text_value(id: i32) -> bool {
    TYPES
        .iter()
        .find(|entry| entry.id.0 == id)
        .is_some_and(|entry| entry.text_value)
}
