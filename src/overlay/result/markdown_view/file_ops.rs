//! File operations for markdown view (save HTML)

use super::conversion::markdown_to_html;

/// Generate a filename using the centralized default text model.
///
/// Routes through the shared text path so the filename request follows whichever
/// provider the default text model belongs to instead of one hardcoded endpoint.
pub fn generate_filename(content: &str) -> String {
    let default_name = "result.html".to_string();

    let Some(default_model) =
        crate::model_config::get_model_by_id(crate::model_config::DEFAULT_TEXT_MODEL_ID)
    else {
        return default_name;
    };
    if crate::model_config::model_is_non_llm(&default_model.id) {
        return default_name;
    }

    let Ok(app) = crate::APP.lock() else {
        return default_name;
    };
    let groq_api_key =
        crate::api::provider_credentials::resolve("GROQ_API_KEY", &app.config.api_key);
    let gemini_api_key =
        crate::api::provider_credentials::resolve("GEMINI_API_KEY", &app.config.gemini_api_key);
    let ui_language = app.config.ui_language.clone();
    drop(app);

    // Truncate to avoid token limits (first 4000 chars is enough for context).
    // Slice by chars, not bytes — a byte cut would panic mid-UTF-8 on ko/vi content.
    let prompt_content: String = content.chars().take(4000).collect();

    let instruction = "Generate a short, kebab-case filename (without extension) for the following content.         Do NOT include 'html' in the name.         The filename must be descriptive but concise (max 5 words).         Output ONLY the filename, nothing else. No markdown, no quotes, no explanations."
        .to_string();

    let response = crate::api::text::translate_text_streaming(
        crate::api::text::TranslateTextRequest {
            groq_api_key: &groq_api_key,
            gemini_api_key: &gemini_api_key,
            text: prompt_content,
            instruction,
            model: default_model.full_name.clone(),
            provider: default_model.provider.clone(),
            streaming_enabled: false,
            use_json_format: false,
            response_schema: None,
            search_label: None,
            ui_language: &ui_language,
            cancel_token: None,
            request_timeout: None,
            target_language: None,
        },
        |_| {},
    );

    match response {
        Ok(raw) => sanitize_generated_filename(&raw).unwrap_or(default_name),
        Err(e) => {
            eprintln!("Failed to generate filename: {}", e);
            default_name
        }
    }
}

/// Turns a model's raw filename suggestion into a safe `.html` filename.
fn sanitize_generated_filename(raw: &str) -> Option<String> {
    // Clean up quotes/markdown
    let mut name = raw.trim().replace(['"', '\'', '`'], "");

    // Remove potential .html extension if the model disobeyed
    if name.to_lowercase().ends_with(".html") {
        name.truncate(name.len() - 5);
    }

    // Remove trailing -html or _html if present to avoid redundancy
    let lower_name = name.to_lowercase();
    if lower_name.ends_with("-html") || lower_name.ends_with("_html") {
        name.truncate(name.len() - 5);
    }

    // Basic validation: remove invalid characters for Windows filenames
    const INVALID_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    name = name
        .chars()
        .filter(|c| !INVALID_CHARS.contains(c))
        .collect();

    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(format!("{name}.html"))
}

/// Save the current content as HTML file using Windows File Save dialog
/// Returns true if file was saved successfully
pub fn save_html_file(markdown_text: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG;
    use windows::Win32::UI::Shell::{
        FOLDERID_Downloads, FOS_OVERWRITEPROMPT, FOS_STRICTFILETYPES, FileSaveDialog,
        IFileSaveDialog, IShellItem, SHCreateItemFromParsingName, SHGetKnownFolderPath,
        SIGDN_FILESYSPATH,
    };
    use windows::core::PCWSTR;

    unsafe {
        // Initialize COM
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Create file dialog
        let dialog: IFileSaveDialog = match CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL) {
            Ok(d) => d,
            Err(_) => {
                CoUninitialize();
                return false;
            }
        };

        // Set file type filter - HTML files
        let filter_name: Vec<u16> = OsStr::new("HTML Files (*.html)")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let filter_pattern: Vec<u16> = OsStr::new("*.html")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let file_types = [COMDLG_FILTERSPEC {
            pszName: windows::core::PCWSTR(filter_name.as_ptr()),
            pszSpec: windows::core::PCWSTR(filter_pattern.as_ptr()),
        }];

        let _ = dialog.SetFileTypes(&file_types);
        let _ = dialog.SetFileTypeIndex(1);

        // Set default folder to Downloads
        if let Ok(downloads_path) =
            SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None)
            && let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(downloads_path.0), None)
        {
            let _ = dialog.SetFolder(&folder_item);
        }

        // Set default extension
        let default_ext: Vec<u16> = OsStr::new("html")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let _ = dialog.SetDefaultExtension(windows::core::PCWSTR(default_ext.as_ptr()));

        // Set default filename
        let filename = generate_filename(markdown_text);
        let default_name: Vec<u16> = OsStr::new(&filename)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let _ = dialog.SetFileName(windows::core::PCWSTR(default_name.as_ptr()));

        // Set options
        let _ = dialog.SetOptions(FOS_OVERWRITEPROMPT | FOS_STRICTFILETYPES);

        // Show dialog
        if dialog.Show(None).is_err() {
            CoUninitialize();
            return false; // User cancelled
        }

        // Get result
        let result: windows::Win32::UI::Shell::IShellItem = match dialog.GetResult() {
            Ok(r) => r,
            Err(_) => {
                CoUninitialize();
                return false;
            }
        };

        // Get file path
        let path: windows::core::PWSTR = match result.GetDisplayName(SIGDN_FILESYSPATH) {
            Ok(p) => p,
            Err(_) => {
                CoUninitialize();
                return false;
            }
        };

        // Convert path to String
        let path_str = path.to_string().unwrap_or_default();

        // Free the path memory
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));

        CoUninitialize();

        // Generate HTML content
        let html_content = markdown_to_html(markdown_text, false, "", "");

        // Write to file
        std::fs::write(&path_str, html_content).is_ok()
    }
}
