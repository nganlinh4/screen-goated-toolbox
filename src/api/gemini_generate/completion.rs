//! Completion validation for the ordinary Gemini text response contract.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::io::BufRead;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::GeminiGenerateOutput;

#[derive(Default)]
struct Completion {
    text: String,
    stopped: bool,
    #[cfg(test)]
    usage: Option<Value>,
}

impl Completion {
    fn push(&mut self, root: &Value, on_delta: &mut impl FnMut(&str, bool)) -> Result<()> {
        if let Some(error) = root.get("error").filter(|v| !v.is_null()) {
            bail!("PROVIDER_RESPONSE_INVALID:Provider response error: {error}");
        }
        if let Some(reason) = root
            .pointer("/promptFeedback/blockReason")
            .and_then(Value::as_str)
            && reason != "BLOCK_REASON_UNSPECIFIED"
        {
            bail!("PROVIDER_RESPONSE_INVALID:Provider blocked response: {reason}");
        }
        #[cfg(test)]
        if let Some(usage) = root.get("usageMetadata") {
            self.usage = Some(usage.clone());
        }
        let Some(candidates) = root.get("candidates").and_then(Value::as_array) else {
            ensure!(
                root.get("usageMetadata").is_some(),
                "PROVIDER_RESPONSE_INVALID:Provider returned no candidates"
            );
            return Ok(());
        };
        let Some(candidate) = candidates
            .iter()
            .find(|c| c.get("index").is_none_or(|n| n.as_u64() == Some(0)))
        else {
            ensure!(
                root.get("usageMetadata").is_some(),
                "PROVIDER_RESPONSE_INVALID:Provider returned no candidates"
            );
            return Ok(());
        };
        ensure!(
            candidate.is_object(),
            "PROVIDER_RESPONSE_INVALID:Malformed provider candidate"
        );
        let stop = match candidate.get("finishReason").filter(|v| !v.is_null()) {
            None => false,
            Some(v) if v.as_str() == Some("STOP") => true,
            Some(v) if v.as_str() == Some("MAX_TOKENS") => {
                bail!("PROVIDER_RESPONSE_INVALID:Provider completion token limit reached")
            }
            Some(v) => bail!("PROVIDER_RESPONSE_INVALID:Provider completion did not succeed: {v}"),
        };
        let mut text = String::new();
        let mut thought = false;
        if let Some(content) = candidate.get("content") {
            let parts = content
                .get("parts")
                .and_then(Value::as_array)
                .context("PROVIDER_RESPONSE_INVALID:Malformed provider content")?;
            for part in parts {
                ensure!(
                    part.is_object(),
                    "PROVIDER_RESPONSE_INVALID:Malformed provider part"
                );
                if let Some(value) = part.get("text") {
                    let value = value
                        .as_str()
                        .context("PROVIDER_RESPONSE_INVALID:Provider content is not text")?;
                    if part
                        .get("thought")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        thought |= !value.is_empty();
                    } else {
                        text.push_str(value);
                    }
                } else {
                    ensure!(
                        part.get("thoughtSignature").is_some(),
                        "PROVIDER_RESPONSE_INVALID:Provider returned non-text content"
                    );
                }
            }
        }
        ensure!(
            !self.stopped || (text.is_empty() && !thought),
            "PROVIDER_RESPONSE_INVALID:Provider emitted output after completion"
        );
        self.stopped |= stop;
        self.text.push_str(&text);
        if !text.is_empty() {
            crate::api::client::first_token::received();
        }
        if !text.is_empty() || thought {
            on_delta(&text, thought);
        }
        Ok(())
    }

    fn finish(self) -> Result<GeminiGenerateOutput> {
        ensure!(
            self.stopped,
            "PROVIDER_RESPONSE_INVALID:Provider stream ended before completion"
        );
        ensure!(
            !self.text.trim().is_empty(),
            "PROVIDER_RESPONSE_INVALID:Provider returned no output content"
        );
        Ok(GeminiGenerateOutput {
            content: self.text,
            #[cfg(test)]
            usage_metadata: self.usage,
        })
    }
}

pub(super) fn parse(root: &Value) -> Result<GeminiGenerateOutput> {
    let mut completion = Completion::default();
    completion.push(root, &mut |_, _| {})?;
    completion.finish()
}

pub(super) fn consume<R: BufRead>(
    reader: R,
    cancel: &Option<Arc<AtomicBool>>,
    mut on_delta: impl FnMut(&str, bool),
) -> Result<GeminiGenerateOutput> {
    let mut completion = Completion::default();
    let mut event = Vec::new();
    let mut dispatch = |event: &mut Vec<String>| -> Result<()> {
        if event.is_empty() {
            return Ok(());
        }
        let data = event.join("\n");
        event.clear();
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let root: Value = serde_json::from_str(&data)
            .context("PROVIDER_RESPONSE_INVALID:Malformed provider stream event")?;
        completion.push(&root, &mut on_delta)
    };
    for line in reader.lines() {
        ensure!(
            !cancel.as_ref().is_some_and(|v| v.load(Ordering::Relaxed)),
            "Cancelled"
        );
        let line = line?;
        if line.is_empty() {
            dispatch(&mut event)?;
        } else if let Some(data) = line.strip_prefix("data:") {
            event.push(data.strip_prefix(' ').unwrap_or(data).to_string());
        }
    }
    dispatch(&mut event)?;
    ensure!(
        !cancel.as_ref().is_some_and(|v| v.load(Ordering::Relaxed)),
        "Cancelled"
    );
    completion.finish()
}

#[cfg(test)]
mod tests;
