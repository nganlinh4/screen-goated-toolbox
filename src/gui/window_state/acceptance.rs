//! Opt-in, isolated native window acceptance without a remote desktop controller.
use anyhow::{Result, ensure};
use eframe::egui;
use serde::Deserialize;
use std::{
    ffi::OsStr,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use windows::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            GetClientRect, GetWindowPlacement, GetWindowRect, IsIconic, IsWindowVisible, IsZoomed,
            WINDOWPLACEMENT,
        },
    },
};

struct Probe {
    root: PathBuf,
    last_id: u64,
    ready_written: bool,
    busy: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct Request {
    id: u64,
    #[serde(flatten)]
    action: Action,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Action {
    Snapshot,
    TrayMenu,
    Resize {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Maximize,
    Restore,
    Quit,
}

static PROBE: LazyLock<Mutex<Option<Probe>>> = LazyLock::new(|| {
    let enabled = std::env::var_os("SGT_WINDOW_GEOMETRY_ACCEPTANCE");
    Mutex::new(
        enabled_root(enabled.as_deref(), crate::paths::runtime_state_root()).map(|root| Probe {
            last_id: existing_request_id(&root.join("request.json")),
            root,
            ready_written: false,
            busy: Arc::new(AtomicBool::new(false)),
        }),
    )
});

fn enabled_root(flag: Option<&OsStr>, isolated: Option<PathBuf>) -> Option<PathBuf> {
    let root = isolated.filter(|root| root.is_absolute())?;
    (flag == Some(OsStr::new("1"))).then(|| root.join("geometry-acceptance"))
}

fn existing_request_id(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.len() <= 4096)
        .and_then(|_| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Request>(&bytes).ok())
        .map_or(0, |request| request.id)
}

pub(crate) fn tick(ctx: &egui::Context) {
    let mut slot = PROBE.lock().unwrap_or_else(|error| error.into_inner());
    let Some(probe) = slot.as_mut() else { return };
    if let Err(error) = probe.tick(ctx) {
        crate::log_info!("[Window acceptance] {error:#}");
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

impl Probe {
    fn tick(&mut self, ctx: &egui::Context) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        for event in ctx.input(|input| input.events.clone()) {
            if let egui::Event::Screenshot {
                user_data, image, ..
            } = event
                && let Some(id) = user_data
                    .data
                    .as_ref()
                    .and_then(|data| data.downcast_ref::<u64>())
            {
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                let temporary = self.root.join(format!("screenshot-{id}.next.png"));
                image::save_buffer(
                    &temporary,
                    &pixels,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                )?;
                std::fs::rename(temporary, self.root.join(format!("screenshot-{id}.png")))?;
            }
        }
        if self.busy.load(Ordering::Acquire) {
            return Ok(());
        }
        let ready = self.root.join("ready.json");
        if !self.ready_written {
            crate::atomic_json::write_json_atomic(&ready, &snapshot()?)?;
            self.ready_written = true;
        }
        let request = self.root.join("request.json");
        if request.is_file() && std::fs::metadata(&request)?.len() <= 4096 {
            let request: Request = serde_json::from_slice(&std::fs::read(&request)?)?;
            if request.id > self.last_id {
                self.last_id = request.id;
                if let Action::Resize {
                    x,
                    y,
                    width,
                    height,
                } = &request.action
                {
                    ensure!(
                        [*x, *y, *width, *height].iter().all(|v| v.is_finite()),
                        "non-finite geometry"
                    );
                    ensure!(
                        x.abs() <= 131_072.0
                            && y.abs() <= 131_072.0
                            && (1.0..=32_768.0).contains(width)
                            && (1.0..=32_768.0).contains(height),
                        "geometry out of bounds"
                    );
                }
                match request.action {
                    Action::Resize {
                        x,
                        y,
                        width,
                        height,
                    } => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                            x, y,
                        )));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            width, height,
                        )));
                    }
                    Action::Maximize => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true))
                    }
                    Action::Restore => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false))
                    }
                    Action::Snapshot => {}
                    Action::TrayMenu => crate::overlay::tray_popup::show_tray_popup(),
                    Action::Quit => {
                        crate::atomic_json::write_json_atomic(
                            &self.root.join(format!("response-{}.json", request.id)),
                            &snapshot()?,
                        )?;
                        crate::gui::app::exit_app();
                        return Ok(());
                    }
                }
                self.busy.store(true, Ordering::Release);
                let root = self.root.clone();
                let busy = Arc::clone(&self.busy);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    // Observe before waking the UI: polling must not make a missing
                    // geometry-debounce repaint appear to work.
                    std::thread::sleep(Duration::from_millis(1200));
                    let result = snapshot().and_then(|state| {
                        crate::atomic_json::write_json_atomic(
                            &root.join(format!("response-{}.json", request.id)),
                            &state,
                        )?;
                        Ok(())
                    });
                    if let Err(error) = result {
                        crate::log_info!("[Window acceptance] snapshot failed: {error:#}");
                    }
                    busy.store(false, Ordering::Release);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                        request.id,
                    )));
                    ctx.request_repaint();
                });
                return Ok(());
            }
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        Ok(())
    }
}

fn rect(rect: RECT) -> [i32; 4] {
    [rect.left, rect.top, rect.right, rect.bottom]
}

fn snapshot() -> Result<serde_json::Value> {
    unsafe {
        let hwnd = crate::gui::app::main_window_hwnd();
        let mut outer = RECT::default();
        let mut client = RECT::default();
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        GetWindowRect(hwnd, &mut outer)?;
        GetClientRect(hwnd, &mut client)?;
        GetWindowPlacement(hwnd, &mut placement)?;
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .ok()?;
        let saved = std::fs::read(super::state_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        Ok(serde_json::json!({
            "utc": chrono::Utc::now().to_rfc3339(), "processId": std::process::id(),
            "dpi": GetDpiForWindow(hwnd), "visible": IsWindowVisible(hwnd).as_bool(),
            "maximized": IsZoomed(hwnd).as_bool(), "minimized": IsIconic(hwnd).as_bool(),
            "outerPixels": rect(outer), "clientPixels": rect(client), "normalPixels": rect(placement.rcNormalPosition),
            "monitorPixels": rect(monitor.rcMonitor), "workPixels": rect(monitor.rcWork), "saved": saved,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_acceptance_requires_both_opt_in_and_isolation() {
        let root = std::env::temp_dir().join("geometry-acceptance-test");
        assert!(enabled_root(None, Some(root.clone())).is_none());
        assert!(enabled_root(Some(OsStr::new("1")), None).is_none());
        assert!(enabled_root(Some(OsStr::new("1")), Some(PathBuf::from("relative"))).is_none());
        assert_eq!(
            enabled_root(Some(OsStr::new("1")), Some(root.clone())),
            Some(root.join("geometry-acceptance"))
        );
    }

    #[test]
    fn geometry_acceptance_does_not_replay_a_previous_process_command() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "sgt-geometry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&directory)?;
        let path = directory.join("request.json");
        std::fs::write(&path, br#"{"id":42,"action":"maximize"}"#)?;
        let existing = existing_request_id(&path);
        std::fs::write(&path, vec![b' '; 4097])?;
        let oversized = existing_request_id(&path);
        std::fs::remove_file(&path)?;
        std::fs::remove_dir(&directory)?;
        assert_eq!(existing, 42);
        assert_eq!(oversized, 0);
        Ok(())
    }

    #[test]
    #[ignore = "requires a native DX12 adapter"]
    fn native_capture_pipeline_layout_accepts_passthrough_shaders() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("native DX12 adapter");
        let (device, _queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::PASSTHROUGH_SHADERS,
            ..Default::default()
        }))
        .expect("native capture device");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture-layout-test"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _capture = eframe::egui_wgpu::capture::CaptureState::new(&device, &texture);
        let error = block_on(scope.pop());
        assert!(error.is_none(), "capture pipeline must validate: {error:?}");
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        struct Unpark(std::thread::Thread);
        impl std::task::Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(output) => return output,
                std::task::Poll::Pending => std::thread::park(),
            }
        }
    }
}
