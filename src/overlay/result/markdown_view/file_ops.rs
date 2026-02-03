//! File operations for markdown view (save HTML)

use super::conversion::markdown_to_html;

/// Generate a filename using Cerebras' gpt-oss-120b model
pub fn generate_filename(content: &str) -> String {
    let default_name = "result.html".to_string();

    // Get API Key
    let cerebras_key = if let Ok(app) = crate::APP.lock() {
        app.config.cerebras_api_key.clone()
    } else {
        return default_name;
    };

    if cerebras_key.is_empty() {
        return default_name;
    }

    // Truncate content to avoid token limits (first 4000 chars should be enough for context)
    let prompt_content = if content.len() > 4000 {
        &content[..4000]
    } else {
        content
    };

    let prompt = format!(
        "Generate a short, kebab-case filename (without extension) for the following content. \
        Do NOT include 'html' in the name. \
        The filename must be descriptive but concise (max 5 words). \
        Output ONLY the filename, nothing else. No markdown, no quotes, no explanations.\n\nContent:\n{}",
        prompt_content
    );

    let payload = serde_json::json!({
        "model": "gpt-oss-120b",
        "messages": [
            { "role": "user", "content": prompt }
        ],
        "temperature": 0.3,
        "max_tokens": 60
    });

    match crate::api::client::UREQ_AGENT
        .post("https://api.cerebras.ai/v1/chat/completions")
        .header("Authorization", &format!("Bearer {}", cerebras_key))
        .send_json(payload)
    {
        Ok(resp) => {
            if let Ok(json) = resp.into_body().read_json::<serde_json::Value>() {
                if let Some(choice) = json
                    .get("choices")
                    .and_then(|c| c.as_array())
                    .and_then(|c| c.first())
                {
                    if let Some(content) = choice
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|s| s.as_str())
                    {
                        let mut name = content.trim().to_string();

                        // Clean up quotes/markdown
                        name = name.replace('"', "").replace('\'', "").replace('`', "");

                        // Remove potential .html extension if the model disobeyed
                        if name.to_lowercase().ends_with(".html") {
                            name = name[..name.len() - 5].to_string();
                        }

                        // Remove trailing -html or _html if present to avoid redundancy
                        if name.to_lowercase().ends_with("-html") {
                            name = name[..name.len() - 5].to_string();
                        } else if name.to_lowercase().ends_with("_html") {
                            name = name[..name.len() - 5].to_string();
                        }

                        // Basic validation: remove invalid characters for Windows filenames
                        let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
                        name = name
                            .chars()
                            .filter(|c| !invalid_chars.contains(c))
                            .collect();

                        if name.is_empty() {
                            return default_name;
                        }

                        // Always append .html
                        name.push_str(".html");

                        return name;
                    }
                }
            }
            default_name
        }
        Err(e) => {
            eprintln!("Failed to generate filename: {}", e);
            default_name
        }
    }
}

/// Save the current content as HTML file using Windows File Save dialog
/// Returns true if file was saved successfully
pub fn save_html_file(markdown_text: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG;
    use windows::Win32::UI::Shell::{
        FileSaveDialog, IFileSaveDialog, IShellItem, SHCreateItemFromParsingName,
        SHGetKnownFolderPath, FOLDERID_Downloads, FOS_OVERWRITEPROMPT, FOS_STRICTFILETYPES,
        SIGDN_FILESYSPATH,
    };

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
        {
            if let Ok(folder_item) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(downloads_path.0), None)
            {
                let _ = dialog.SetFolder(&folder_item);
            }
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
        match std::fs::write(&path_str, html_content) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}
