use std::time::Instant;

use super::{
    LiveEvent, LiveResponse,
    wait::{WaitOptions, receive_live_event},
};

#[cfg(test)]
mod tests;

pub(super) fn collect_response(
    response: LiveResponse,
    options: &WaitOptions,
    streaming: bool,
    ui_language: &str,
    mut on_chunk: impl FnMut(&str),
) -> anyhow::Result<String> {
    let locale = crate::gui::locale::LocaleText::get(ui_language);
    let mut content = String::new();
    let mut thinking_shown = false;
    let mut content_started = false;
    loop {
        let event = receive_live_event(&response, options)?.ok_or_else(|| {
            anyhow::anyhow!("Gemini Live channel closed before response completion")
        })?;
        match event {
            LiveEvent::Thinking => {
                if !thinking_shown && !content_started {
                    if streaming {
                        options.check(response.is_retired(), Instant::now())?;
                        on_chunk(locale.global_settings.model_thinking);
                    }
                    thinking_shown = true;
                }
            }
            LiveEvent::TextChunk(chunk) => {
                if chunk.is_empty() {
                    continue;
                }
                content.push_str(&chunk);
                if streaming {
                    options.check(response.is_retired(), Instant::now())?;
                    if !content_started && thinking_shown {
                        on_chunk(&format!("{}{}", crate::api::WIPE_SIGNAL, content));
                    } else {
                        on_chunk(&chunk);
                    }
                }
                content_started = true;
            }
            LiveEvent::TurnComplete => {
                anyhow::ensure!(
                    !content.trim().is_empty(),
                    "Gemini Live completed without nonblank output"
                );
                options.check(response.is_retired(), Instant::now())?;
                if !streaming {
                    on_chunk(&content);
                }
                return Ok(content);
            }
            LiveEvent::Error(error) => {
                if error.contains("NO_API_KEY") || error.contains("INVALID_API_KEY") {
                    crate::overlay::utils::show_api_key_error_notification(&error, ui_language);
                    anyhow::bail!("{error}");
                }
                anyhow::bail!("Gemini Live error: {error}");
            }
        }
    }
}
