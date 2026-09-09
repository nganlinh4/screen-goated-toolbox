mod input;
mod layout;
mod localization;
mod recognition;

use anyhow::{Context, Result, bail};
use sgt_screen_text_detector_protocol::stream::{self, Event, Request};
use std::ffi::OsString;
use std::io::{BufReader, BufWriter};
use std::os::windows::{ffi::OsStringExt, fs::MetadataExt};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("screen translate worker: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if std::env::args_os().skip(1).collect::<Vec<_>>() != ["--stdio"] {
        bail!("worker only accepts --stdio");
    }
    let mut reader = BufReader::new(std::io::stdin());
    let mut writer = BufWriter::new(std::io::stdout());
    let (hello_id, Request::Hello(hello)) = stream::read_request(&mut reader)? else {
        bail!("missing handshake");
    };
    let startup_cancel = Arc::new(AtomicBool::new(false));
    let active = Arc::new(Mutex::new((hello_id, Arc::clone(&startup_cancel))));
    let incoming = input::start(reader, Arc::clone(&active), hello_id);
    let initialized = (|| -> Result<_> {
        let runtime = path(&hello.runtime_dir, true)?;
        let detector = path(&hello.detector_model, false)?;
        let model = path(&hello.reader_catalog, false)?;
        let initialized = std::time::Instant::now();
        let locator = localization::Locator::load(&runtime, &detector)?;
        let (recognizer, layout) = std::thread::scope(|scope| -> Result<_> {
            let layout_job =
                scope.spawn(|| layout::Layout::load(&detector.with_file_name("layout.onnx")));
            let recognizer = recognition::Recognizer::load(&model)?;
            let layout = layout_job
                .join()
                .map_err(|_| anyhow::anyhow!("layout initialization thread failed"))??;
            Ok((recognizer, layout))
        })?;
        eprintln!(
            "[DetectorPerf] worker_ready_ms={:.1}",
            initialized.elapsed().as_secs_f64() * 1000.0
        );
        Ok((locator, recognizer, layout))
    })();
    let (mut locator, mut recognizer, mut layout) = match initialized {
        Ok(value) => value,
        Err(error) => {
            stream::write_event(
                &mut writer,
                hello_id,
                &Event::Error {
                    message: format!("{error:#}"),
                },
            )?;
            return Err(error);
        }
    };
    if startup_cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    active.lock().unwrap().0 = 0;
    stream::write_event(
        &mut writer,
        hello_id,
        &Event::Ready {
            nonce: hello.nonce,
            version: stream::WORKER_VERSION.into(),
        },
    )?;
    while let Ok(incoming) = incoming.recv() {
        let input::Incoming::Capture {
            id,
            jpeg,
            cancelled,
        } = incoming
        else {
            break;
        };
        let result = (|| -> Result<usize> {
            let started = std::time::Instant::now();
            let image = localization::decode(&jpeg)?;
            let (regions, layout_regions) = std::thread::scope(|scope| -> Result<_> {
                let layout_job = scope.spawn(|| -> Result<_> {
                    let started = std::time::Instant::now();
                    let regions = layout.detect(&image)?;
                    eprintln!(
                        "[DetectorPerf] capture={id} layout_ms={:.1}",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                    Ok(regions)
                });
                let located_at = std::time::Instant::now();
                let located = locator.locate(&image)?;
                eprintln!(
                    "[DetectorPerf] capture={id} locator_ms={:.1}",
                    located_at.elapsed().as_secs_f64() * 1000.0
                );
                let advisory = layout_job
                    .join()
                    .map_err(|_| anyhow::anyhow!("layout thread failed"))??;
                Ok((located, advisory))
            })?;
            if cancelled.load(Ordering::Acquire) {
                bail!("capture cancelled");
            }
            eprintln!(
                "[DetectorPerf] capture={id} geometry_ms={:.1} regions={}",
                started.elapsed().as_secs_f64() * 1000.0,
                regions.len()
            );
            stream::write_event(
                &mut writer,
                id,
                &Event::Geometry {
                    width: image.width(),
                    height: image.height(),
                    regions: regions.clone(),
                    layout: layout_regions,
                },
            )?;
            recognizer.recognize(id, &image, &regions, &cancelled, |completion| {
                stream::write_event(
                    &mut writer,
                    id,
                    &Event::Reading {
                        completion: completion.clone(),
                    },
                )?;
                Ok(())
            })?;
            Ok(regions.len())
        })();
        let terminal = if cancelled.load(Ordering::Acquire) {
            Event::Cancelled
        } else {
            match result {
                Ok(count) => Event::Finished { count },
                Err(error) => Event::Error {
                    message: format!("{error:#}"),
                },
            }
        };
        // Clear admission before sending the terminal event; a following request
        // must not race against the previous capture's busy state.
        {
            let mut current = active.lock().unwrap();
            if current.0 == id {
                current.0 = 0;
            }
        }
        stream::write_event(&mut writer, id, &terminal)?;
    }
    Ok(())
}

fn path(raw: &[u16], directory: bool) -> Result<PathBuf> {
    let path = PathBuf::from(OsString::from_wide(raw));
    if !path.is_absolute() {
        bail!("worker resource path must be absolute");
    }
    let metadata = std::fs::symlink_metadata(&path).context("inspect worker resource")?;
    if metadata.file_attributes() & 0x400 != 0
        || (directory && !metadata.is_dir())
        || (!directory && (!metadata.is_file() || metadata.len() == 0))
    {
        bail!("unsafe worker resource");
    }
    std::fs::canonicalize(path).context("resolve worker resource")
}
