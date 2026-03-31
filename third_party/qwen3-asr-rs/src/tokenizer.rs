use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tokenizers::models::bpe::BPE;
use tokenizers::pre_tokenizers::byte_level::ByteLevel;
use tokenizers::{AddedToken, Tokenizer};

pub struct AsrTokenizer {
    tokenizer: Tokenizer,
}

impl AsrTokenizer {
    /// Load tokenizer from model directory.
    /// Expects either tokenizer.json or vocab.json + merges.txt
    pub fn from_dir(model_dir: &Path) -> Result<Self> {
        let tokenizer_path = model_dir.join("tokenizer.json");
        if tokenizer_path.exists() {
            let tokenizer = Tokenizer::from_file(&tokenizer_path)
                .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;
            return Ok(Self { tokenizer });
        }

        let tokenizer = Self::from_bpe_files(model_dir)?;
        let _ = tokenizer.save(tokenizer_path, false);
        Ok(Self { tokenizer })
    }

    /// Encode text to token IDs.
    pub fn encode(&self, text: &str) -> Result<Vec<i64>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;
        Ok(encoding.get_ids().iter().map(|&id| id as i64).collect())
    }

    /// Decode token IDs to text.
    pub fn decode(&self, ids: &[i64]) -> Result<String> {
        let u32_ids: Vec<u32> = ids.iter().map(|&id| id as u32).collect();
        let text = self
            .tokenizer
            .decode(&u32_ids, true)
            .map_err(|e| anyhow::anyhow!("Decoding failed: {}", e))?;
        Ok(text)
    }
}

#[derive(Default, Deserialize)]
struct TokenizerConfig {
    #[serde(default)]
    add_prefix_space: bool,
    #[serde(default)]
    added_tokens_decoder: BTreeMap<String, AddedTokenEntry>,
}

#[derive(Deserialize)]
struct AddedTokenEntry {
    content: String,
    #[serde(default)]
    single_word: bool,
    #[serde(default)]
    lstrip: bool,
    #[serde(default)]
    rstrip: bool,
    #[serde(default)]
    normalized: bool,
    #[serde(default)]
    special: bool,
}

impl AsrTokenizer {
    fn from_bpe_files(model_dir: &Path) -> Result<Tokenizer> {
        let vocab_path = model_dir.join("vocab.json");
        let merges_path = model_dir.join("merges.txt");
        let config_path = model_dir.join("tokenizer_config.json");

        if !vocab_path.exists() || !merges_path.exists() {
            anyhow::bail!(
                "tokenizer.json not found in {:?}, and fallback vocab.json/merges.txt files are missing",
                model_dir
            );
        }

        let config = if config_path.exists() {
            serde_json::from_str::<TokenizerConfig>(
                &fs::read_to_string(&config_path)
                    .with_context(|| format!("Failed to read {:?}", config_path))?,
            )
            .with_context(|| format!("Failed to parse {:?}", config_path))?
        } else {
            TokenizerConfig::default()
        };

        let vocab_path_str = vocab_path.to_string_lossy().into_owned();
        let merges_path_str = merges_path.to_string_lossy().into_owned();
        let bpe = BPE::from_file(&vocab_path_str, &merges_path_str)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build tokenizer from vocab/merges: {e}"))?;

        let mut tokenizer = Tokenizer::new(bpe);
        let byte_level = ByteLevel::new(config.add_prefix_space, true, true);
        tokenizer.with_pre_tokenizer(Some(byte_level));
        tokenizer.with_post_processor(Some(byte_level));
        tokenizer.with_decoder(Some(byte_level));

        if !config.added_tokens_decoder.is_empty() {
            let special_tokens: Vec<AddedToken> = config
                .added_tokens_decoder
                .into_values()
                .map(|entry| {
                    AddedToken::from(entry.content, entry.special)
                        .single_word(entry.single_word)
                        .lstrip(entry.lstrip)
                        .rstrip(entry.rstrip)
                        .normalized(entry.normalized)
                        .special(entry.special)
                })
                .collect();
            tokenizer.add_special_tokens(&special_tokens);
        }

        Ok(tokenizer)
    }
}

// Special token IDs for Qwen3-ASR
pub const IM_START_TOKEN_ID: i64 = 151644;
pub const IM_END_TOKEN_ID: i64 = 151645;
pub const ENDOFTEXT_TOKEN_ID: i64 = 151643;
pub const AUDIO_START_TOKEN_ID: i64 = 151669;
pub const AUDIO_END_TOKEN_ID: i64 = 151670;
pub const AUDIO_PAD_TOKEN_ID: i64 = 151676;
pub const ASR_TEXT_TOKEN_ID: i64 = 151704;
