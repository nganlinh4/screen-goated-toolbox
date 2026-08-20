mod detector;
mod postprocess;
mod recognizer;
mod recognizer_cascade;
mod row_split;

use std::ffi::OsString;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::os::windows::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use detector::TextDetector;
use sgt_screen_text_detector_protocol::{
    ClientMessage, ServerMessage, WORKER_VERSION, read_client, write_server,
};

const RUNTIME_FILES: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
    "DirectML.dll",
];
const MODEL_FILES: &[&str] = &["detector.onnx", "detector.ort", "recognizers.json"];

fn main() {
    if let Err(error) = run() {
        eprintln!("screen-text detector worker failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if std::env::args_os().skip(1).collect::<Vec<_>>() != ["--stdio"] {
        bail!("the screen-text detector worker only accepts --stdio");
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let (hello_id, hello) = read_client(&mut reader).context("read detector handshake")?;
    let ClientMessage::Hello {
        nonce,
        runtime_dir,
        model_dir,
    } = hello
    else {
        bail!("the first detector frame must be a handshake");
    };

    let initialized = initialize(&runtime_dir, &model_dir);
    let mut detector = match initialized {
        Ok(detector) => detector,
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
            worker_version: WORKER_VERSION.to_string(),
        },
    )?;

    loop {
        let (request_id, message) = match read_client(&mut reader) {
            Ok(message) => message,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error).context("read detector request"),
        };
        let response = match message {
            ClientMessage::DetectJpeg(jpeg) => {
                detector
                    .detect_jpeg(&jpeg)
                    .map(|result| ServerMessage::Regions {
                        image_width: result.image_width,
                        image_height: result.image_height,
                        timings: result.timings,
                        regions: result.regions,
                    })
            }
            ClientMessage::Shutdown => {
                write_server(&mut writer, request_id, &ServerMessage::Ack)?;
                return Ok(());
            }
            ClientMessage::Hello { .. } => Err(anyhow::anyhow!("duplicate detector handshake")),
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

fn initialize(runtime_dir: &[u16], model_dir: &[u16]) -> Result<TextDetector> {
    let runtime_dir = validated_root(runtime_dir, RUNTIME_FILES, "runtime")?;
    let model_dir = validated_root(model_dir, MODEL_FILES, "model")?;
    let runtime_dll = runtime_dir.join("onnxruntime.dll");
    if !ort::init_from(&runtime_dll)
        .context("load the verified ONNX Runtime")?
        .commit()
    {
        bail!("ONNX Runtime was initialized before the detector handshake");
    }
    TextDetector::load(
        &model_dir.join("detector.ort"),
        &model_dir.join("detector.onnx"),
        &model_dir,
        &model_dir.join("recognizers.json"),
    )
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
        assert!(validated_root(&relative, MODEL_FILES, "model").is_err());

        let root = std::env::temp_dir().join(format!(
            "sgt-screen-text-detector-worker-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let wide = root.as_os_str().encode_wide().collect::<Vec<_>>();
        assert!(validated_root(&wide, MODEL_FILES, "model").is_err());
        fs::remove_dir(root).unwrap();
    }
}
