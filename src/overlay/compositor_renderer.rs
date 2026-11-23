//! WebView2 renderer plumbing shared by every compositor child (result scenes,
//! status overlay and realtime overlay): the renderer-failure vocabulary, the
//! process-failure probe, the DirectComposition device and the transparent
//! composition-hosted WebView.

use serde::{Deserialize, Serialize};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS, COREWEBVIEW2_COLOR, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2CompositionController,
    ICoreWebView2Controller, ICoreWebView2Controller2, ICoreWebView2Controller3,
    ICoreWebView2Environment3,
};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
    NavigationCompletedEventHandler, ProcessFailedEventHandler,
};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::core::{BOOL, Interface, PCWSTR};

/// Installed before any page script: the page bridge plus uncaught-error
/// reporting through it.
pub(crate) const RENDERER_BOOTSTRAP: &str = r#"
window.ipc={postMessage:function(m){window.chrome.webview.postMessage(m);}};
window.addEventListener('error',function(e){window.ipc.postMessage(JSON.stringify({type:'renderer_error',source:'page',error:String(e.message)+' @ '+e.lineno+':'+e.colno}));});
window.addEventListener('unhandledrejection',function(e){window.ipc.postMessage(JSON.stringify({type:'renderer_error',source:'promise',error:String(e.reason)}));});
"#;

/// A WebView2 process failure that requires the supervisor to act.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererFailureKind {
    BrowserProcessExited,
    RenderProcessExited,
    RenderProcessUnresponsive,
    FrameRenderProcessExited,
    GpuProcessExited,
}

impl RendererFailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BrowserProcessExited => "browser process exited",
            Self::RenderProcessExited => "render process exited",
            Self::RenderProcessUnresponsive => "render process unresponsive",
            Self::FrameRenderProcessExited => "frame render process exited",
            Self::GpuProcessExited => "GPU process exited",
        }
    }

    /// Maps a WebView2 failure; kinds the renderer survives map to `None`.
    pub(crate) fn from_process_failure(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> Option<Self> {
        match kind {
            COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => {
                Some(Self::BrowserProcessExited)
            }
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => {
                Some(Self::RenderProcessExited)
            }
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => {
                Some(Self::RenderProcessUnresponsive)
            }
            COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => {
                Some(Self::FrameRenderProcessExited)
            }
            COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => Some(Self::GpuProcessExited),
            _ => None,
        }
    }
}

/// Reports every actionable process failure of `webview` through `emit`.
pub(crate) fn attach_process_failure(
    webview: &ICoreWebView2,
    emit: fn(RendererFailureKind),
) -> windows::core::Result<()> {
    let handler = ProcessFailedEventHandler::create(Box::new(move |_webview, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
        unsafe { args.ProcessFailedKind(&mut kind)? };
        if let Some(kind) = RendererFailureKind::from_process_failure(kind) {
            emit(kind);
        }
        Ok(())
    }));
    let mut token = Default::default();
    unsafe { webview.add_ProcessFailed(&handler, &mut token) }
}

/// Runs `script` before any page script in every document the WebView loads.
pub(crate) fn add_document_script(
    webview: &ICoreWebView2,
    script: &str,
) -> windows::core::Result<()> {
    let script = windows::core::HSTRING::from(script);
    let webview = webview.clone();
    AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            webview
                .AddScriptToExecuteOnDocumentCreated(PCWSTR(script.as_ptr()), &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(|code, _| code),
    )
    .map_err(webview_error)
}

/// After each navigation, evaluates `probe` (a JSON-returning expression that
/// describes the page bridge) and hands one diagnostic line to `report`.
pub(crate) fn attach_navigation_probe(
    webview: &ICoreWebView2,
    probe: &'static str,
    report: fn(String),
) -> windows::core::Result<()> {
    let handler = NavigationCompletedEventHandler::create(Box::new(move |webview, args| {
        let mut success_value = BOOL::default();
        let success = args.as_ref().is_some_and(|args| unsafe {
            args.IsSuccess(&mut success_value).is_ok() && success_value.as_bool()
        });
        let Some(webview) = webview else {
            report(format!(
                "navigation completed without a WebView success={success}"
            ));
            return Ok(());
        };
        let completion = ExecuteScriptCompletedHandler::create(Box::new(move |code, result| {
            report(format!(
                "navigation success={success} script_code={code:?} probe={result}"
            ));
            Ok(())
        }));
        let probe = windows::core::HSTRING::from(probe);
        unsafe { webview.ExecuteScript(PCWSTR(probe.as_ptr()), &completion) }
    }));
    let mut token = Default::default();
    unsafe { webview.add_NavigationCompleted(&handler, &mut token) }
}

pub(crate) fn webview_error(error: webview2_com::Error) -> windows::core::Error {
    match error {
        webview2_com::Error::WindowsError(error) => error,
        other => {
            windows::core::Error::new(windows::Win32::Foundation::E_FAIL, format!("{other:?}"))
        }
    }
}

pub(crate) fn pointer_error() -> windows::core::Error {
    windows::core::Error::from(windows::Win32::Foundation::E_POINTER)
}

/// Creates a WebView2 composition controller under `controller_parent` that
/// draws into `root` in raw physical pixels on a transparent background,
/// sized `width` x `height` at `scale`. Visibility is left to the caller.
pub(crate) fn create_composition_webview(
    user_data: &std::path::Path,
    controller_parent: HWND,
    root: &IDCompositionVisual,
    (width, height): (i32, i32),
    scale: f64,
) -> windows::core::Result<(ICoreWebView2CompositionController, ICoreWebView2Controller)> {
    let user_data = windows::core::HSTRING::from(user_data.to_string_lossy().as_ref());
    unsafe {
        let environment = {
            let (sender, receiver) = std::sync::mpsc::channel();
            CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
                Box::new(move |handler| {
                    CreateCoreWebView2EnvironmentWithOptions(
                        PCWSTR::null(),
                        PCWSTR(user_data.as_ptr()),
                        None,
                        &handler,
                    )
                    .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |code, environment| {
                    code?;
                    let _ = sender.send(environment);
                    Ok(())
                }),
            )
            .map_err(webview_error)?;
            receiver.recv().ok().flatten().ok_or_else(pointer_error)?
        };
        let environment3: ICoreWebView2Environment3 = environment.cast()?;
        let composition = {
            let (sender, receiver) = std::sync::mpsc::channel();
            CreateCoreWebView2CompositionControllerCompletedHandler::wait_for_async_operation(
                Box::new(move |handler| {
                    environment3
                        .CreateCoreWebView2CompositionController(controller_parent, &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |code, controller| {
                    code?;
                    let _ = sender.send(controller);
                    Ok(())
                }),
            )
            .map_err(webview_error)?;
            receiver.recv().ok().flatten().ok_or_else(pointer_error)?
        };
        composition.SetRootVisualTarget(root)?;

        let controller: ICoreWebView2Controller = composition.cast()?;
        if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
            controller3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS)?;
            controller3.SetShouldDetectMonitorScaleChanges(false)?;
            controller3.SetRasterizationScale(scale)?;
        }
        controller.SetBounds(RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        })?;
        let controller2: ICoreWebView2Controller2 = controller.cast()?;
        controller2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
            A: 0,
            R: 0,
            G: 0,
            B: 0,
        })?;
        Ok((composition, controller))
    }
}

/// Drivers tried in order: software rendering skips the hardware device.
fn driver_candidates(software_rendering: bool) -> &'static [D3D_DRIVER_TYPE] {
    if software_rendering {
        &[D3D_DRIVER_TYPE_WARP]
    } else {
        &[D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP]
    }
}

pub(crate) fn create_composition_device(
    software_rendering: bool,
) -> windows::core::Result<IDCompositionDevice> {
    let try_create = |driver_type: D3D_DRIVER_TYPE| unsafe {
        let mut device = None;
        D3D11CreateDevice(
            None,
            driver_type,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
        let dxgi: IDXGIDevice = device.ok_or_else(pointer_error)?.cast()?;
        DCompositionCreateDevice(&dxgi)
    };
    let mut last_error = None;
    for &driver in driver_candidates(software_rendering) {
        match try_create(driver) {
            Ok(device) => return Ok(device),
            Err(error) => {
                eprintln!("D3D driver {driver:?} unavailable for composition: {error}");
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| windows::Win32::Foundation::E_FAIL.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_falls_back_to_warp_and_software_rendering_skips_hardware() {
        assert_eq!(
            driver_candidates(false),
            [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP]
        );
        assert_eq!(driver_candidates(true), [D3D_DRIVER_TYPE_WARP]);
    }

    #[test]
    fn only_actionable_process_failures_are_reported() {
        assert_eq!(
            RendererFailureKind::from_process_failure(
                COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED
            ),
            Some(RendererFailureKind::GpuProcessExited)
        );
        assert_eq!(
            RendererFailureKind::from_process_failure(
                COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
            ),
            Some(RendererFailureKind::BrowserProcessExited)
        );
        assert_eq!(
            RendererFailureKind::from_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND(99)),
            None
        );
    }

    #[test]
    fn failure_kinds_keep_their_wire_names() {
        assert_eq!(
            serde_json::to_string(&RendererFailureKind::GpuProcessExited).unwrap(),
            "\"gpu_process_exited\""
        );
    }
}
