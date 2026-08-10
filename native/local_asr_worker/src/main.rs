use std::ffi::OsString;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::os::windows::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use parakeet_rs::{
    ExecutionConfig, ExecutionProvider, ParakeetEOU, ParakeetTDT, TimestampMode, Transcriber,
};
use sgt_local_asr_protocol::{
    ClientMessage, Mode, ServerMessage, TimedToken, WORKER_VERSION, read_client, write_server,
};

const RUNTIME_FILES: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
    "DirectML.dll",
];
const EOU_MODEL_FILES: &[&str] = &["encoder.onnx", "decoder_joint.onnx", "tokenizer.json"];
const TDT_MODEL_FILES: &[&str] = &[
    "encoder-model.onnx",
    "encoder-model.onnx.data",
    "decoder_joint-model.onnx",
    "vocab.txt",
];

fn main() {
    if let Err(error) = run() {
        eprintln!("local ASR worker failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if std::env::args_os().skip(1).collect::<Vec<_>>() != ["--stdio"] {
        bail!("the local ASR worker only accepts --stdio");
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let (hello_id, hello) = read_client(&mut reader).context("read protocol handshake")?;
    let ClientMessage::Hello {
        nonce,
        mode,
        runtime_dir,
        model_dir,
    } = hello
    else {
        bail!("the first protocol frame must be a handshake");
    };

    let initialized = initialize(mode, &runtime_dir, &model_dir);
    let mut model = match initialized {
        Ok(model) => model,
        Err(error) => {
            let _ = write_server(
                &mut writer,
                hello_id,
                &ServerMessage::Error(format!("worker initialization failed: {error:#}")),
            );
            return Err(error);
        }
    };
    write_server(
        &mut writer,
        hello_id,
        &ServerMessage::Ready {
            nonce,
            mode,
            worker_version: WORKER_VERSION.to_string(),
        },
    )?;

    loop {
        let (request_id, message) = match read_client(&mut reader) {
            Ok(message) => message,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error).context("read request frame"),
        };
        let response = match (&mut model, message) {
            (WorkerModel::Eou(model), ClientMessage::EouChunk { samples }) => model
                .transcribe(&samples, false)
                .map(ServerMessage::EouText)
                .map_err(|error| anyhow::anyhow!("EOU inference failed: {error}")),
            (
                WorkerModel::Tdt(model),
                ClientMessage::TdtChunk {
                    sample_rate,
                    channels,
                    samples,
                },
            ) => model
                .transcribe_samples(samples, sample_rate, channels, Some(TimestampMode::Words))
                .map(|result| {
                    ServerMessage::TdtTokens(
                        result
                            .tokens
                            .into_iter()
                            .map(|token| TimedToken {
                                start: token.start,
                                end: token.end,
                                text: token.text,
                            })
                            .collect(),
                    )
                })
                .map_err(|error| anyhow::anyhow!("TDT inference failed: {error}")),
            (_, ClientMessage::Shutdown) => {
                write_server(&mut writer, request_id, &ServerMessage::Ack)?;
                return Ok(());
            }
            (_, ClientMessage::Hello { .. }) => Err(anyhow::anyhow!("duplicate handshake")),
            _ => Err(anyhow::anyhow!(
                "request does not match the selected ASR mode"
            )),
        };
        match response {
            Ok(message) => write_server(&mut writer, request_id, &message)?,
            Err(error) => write_server(
                &mut writer,
                request_id,
                &ServerMessage::Error(error.to_string()),
            )?,
        }
    }
}

enum WorkerModel {
    Eou(Box<ParakeetEOU>),
    Tdt(Box<ParakeetTDT>),
}

fn initialize(mode: Mode, runtime_dir: &[u16], model_dir: &[u16]) -> Result<WorkerModel> {
    let runtime_dir = validated_root(runtime_dir, RUNTIME_FILES, "runtime")?;
    let expected_models = match mode {
        Mode::RealtimeEou => EOU_MODEL_FILES,
        Mode::SubtitleTdt => TDT_MODEL_FILES,
    };
    let model_dir = validated_root(model_dir, expected_models, "model")?;
    let runtime_dll = runtime_dir.join("onnxruntime.dll");
    if !ort::init_from(&runtime_dll)
        .context("load the verified ONNX Runtime")?
        .commit()
    {
        bail!("ONNX Runtime was initialized before the worker handshake");
    }

    match mode {
        Mode::RealtimeEou => {
            let config =
                ExecutionConfig::new().with_execution_provider(ExecutionProvider::DirectML);
            Ok(WorkerModel::Eou(Box::new(
                ParakeetEOU::from_pretrained(&model_dir, Some(config))
                    .context("load the realtime EOU model")?,
            )))
        }
        Mode::SubtitleTdt => {
            let config = ExecutionConfig::new()
                .with_execution_provider(ExecutionProvider::DirectML)
                .with_intra_threads(4)
                .with_inter_threads(1);
            Ok(WorkerModel::Tdt(Box::new(
                ParakeetTDT::from_pretrained(&model_dir, Some(config))
                    .context("load the subtitle TDT model")?,
            )))
        }
    }
}

fn validated_root(raw: &[u16], files: &[&str], label: &str) -> Result<PathBuf> {
    let path = PathBuf::from(OsString::from_wide(raw));
    if !path.is_absolute() {
        bail!("{label} root is not absolute");
    }
    let root = fs::canonicalize(&path)
        .with_context(|| format!("canonicalize {label} root '{}'", path.display()))?;
    require_regular_directory(&root, label)?;
    for name in files {
        let file = root.join(name);
        let metadata = fs::symlink_metadata(&file)
            .with_context(|| format!("inspect required {label} file '{}'", file.display()))?;
        if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() == 0 {
            bail!(
                "required {label} file '{}' is unsafe or empty",
                file.display()
            );
        }
    }
    Ok(root)
}

fn require_regular_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("{label} root is not a regular directory");
    }
    Ok(())
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt as _;

    #[test]
    fn path_gate_rejects_relative_and_incomplete_roots() {
        let relative = OsString::from("models").encode_wide().collect::<Vec<_>>();
        assert!(validated_root(&relative, EOU_MODEL_FILES, "model").is_err());

        let root =
            std::env::temp_dir().join(format!("sgt-local-asr-worker-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let wide = root.as_os_str().encode_wide().collect::<Vec<_>>();
        assert!(validated_root(&wide, EOU_MODEL_FILES, "model").is_err());
        fs::remove_dir(root).unwrap();
    }
}
