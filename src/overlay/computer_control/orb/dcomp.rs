//! DirectComposition-hosted WebView2 for the Computer Control orb.
//!
//! Why this exists: `WDA_EXCLUDEFROMCAPTURE` (hiding the orb from the agent's own screen
//! capture) is incompatible with `WS_EX_LAYERED`/DWM alpha transparency — it forces
//! opaque composition (the "white box"). The only setup that gives BOTH a transparent
//! fullscreen surface AND capture-exclusion is a `WS_EX_NOREDIRECTIONBITMAP` window
//! composited via DirectComposition, hosting WebView2 in *composition* mode
//! (`CreateCoreWebView2CompositionController` + `RootVisualTarget`).
//!
//! wry only does windowed hosting, so this talks to WebView2 directly via `webview2-com`
//! (which binds `windows 0.61` — aliased `windows061` — so the whole DComp/WebView2 stack
//! lives on 0.61 to avoid cross-version COM-pointer casts).
//!
//! `build_host` is the COM/DComp plumbing; `window.rs` owns the window + message loop and
//! `wnd_proc.rs` the input/script forwarding.

use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS, COREWEBVIEW2_COLOR,
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2CompositionController,
        ICoreWebView2Controller, ICoreWebView2Controller2, ICoreWebView2Controller3,
        ICoreWebView2Environment3,
    },
    WebMessageReceivedEventHandler,
};
use windows061::Win32::Foundation::{HWND, RECT};
use windows061::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows061::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows061::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows061::Win32::Graphics::Dxgi::IDXGIDevice;
use windows061::core::{Interface, PCWSTR};

/// Bootstrap injected before the page loads: provide the `window.ipc` shim wry would
/// normally give us, so `orb.html` posts to us unchanged (`window.ipc.postMessage` →
/// `window.chrome.webview.postMessage`).
const ORB_BOOTSTRAP: &str =
    "window.ipc={postMessage:function(m){window.chrome.webview.postMessage(m);}};";

/// The live DComp host. The underscore fields are kept alive for the window's lifetime
/// (they form the DirectComposition chain); `comp` + `webview` are also handed to the
/// window proc via thread-locals (see `window.rs`).
pub(super) struct DcompHost {
    _device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _root: IDCompositionVisual,
    pub(super) comp: ICoreWebView2CompositionController,
    pub(super) webview: ICoreWebView2,
}

/// Build the DirectComposition device + visual tree bound to `hwnd` and host WebView2 in
/// composition mode over it, navigated to the orb page. The window stays hidden; `show_orb`
/// reveals it after the page reports `orbReady`.
pub(super) fn build_host(hwnd: HWND) -> windows061::core::Result<DcompHost> {
    unsafe {
        // --- DirectComposition device + visual tree bound to the window ---
        let mut d3d: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )?;
        let dxgi: IDXGIDevice = d3d.unwrap().cast()?;

        let device: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
        let target: IDCompositionTarget = device.CreateTargetForHwnd(hwnd, true)?;
        let root: IDCompositionVisual = device.CreateVisual()?;
        target.SetRoot(&root)?;

        // --- WebView2 environment ---
        // The orb gets its OWN user-data folder, NOT the shared "common" one the wry overlays use.
        // WebView2 requires every environment sharing a user-data folder to be created with identical
        // options; this raw environment passes different options than wry's, so sharing "common" fails
        // with ERROR_INVALID_STATE (0x8007139F). A private folder sidesteps the constraint entirely.
        let user_data = crate::overlay::get_shared_webview_data_dir(Some("cc-orb"));
        let user_data = windows061::core::HSTRING::from(user_data.to_string_lossy().as_ref());
        let environment = {
            let (tx, rx) = std::sync::mpsc::channel();
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
                Box::new(move |code, env| {
                    code?;
                    let _ = tx.send(env);
                    Ok(())
                }),
            )
            .map_err(to_win_err)?;
            rx.recv().ok().flatten().ok_or_else(err_pointer)?
        };

        // --- composition controller (visual hosting) ---
        let env3: ICoreWebView2Environment3 = environment.cast()?;
        let comp: ICoreWebView2CompositionController = {
            let (tx, rx) = std::sync::mpsc::channel();
            CreateCoreWebView2CompositionControllerCompletedHandler::wait_for_async_operation(
                Box::new(move |handler| {
                    env3.CreateCoreWebView2CompositionController(hwnd, &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |code, controller| {
                    code?;
                    let _ = tx.send(controller);
                    Ok(())
                }),
            )
            .map_err(to_win_err)?;
            rx.recv().ok().flatten().ok_or_else(err_pointer)?
        };

        // Attach the browser's visual tree to our DComp visual.
        comp.SetRootVisualTarget(&root)?;

        let controller: ICoreWebView2Controller = comp.cast()?;
        // DPI: composition hosting isn't DPI-aware by default. Match windowed WebView2 (and
        // button_canvas) — bounds stay raw (physical) px, RasterizationScale = the monitor scale —
        // so the page reports LOGICAL px and the orbRegion → physical-region mapping lines up.
        if let Ok(c3) = controller.cast::<ICoreWebView2Controller3>() {
            let _ = c3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS);
            let _ = c3.SetShouldDetectMonitorScaleChanges(false);
            let _ = c3.SetRasterizationScale(super::get_dpi_scale());
        }
        let (_, _, vw, vh) = super::virtual_screen();
        controller.SetBounds(RECT {
            left: 0,
            top: 0,
            right: vw,
            bottom: vh,
        })?;
        // Transparent default background → the page composites over the desktop.
        let controller2: ICoreWebView2Controller2 = controller.cast()?;
        controller2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
            A: 0,
            R: 0,
            G: 0,
            B: 0,
        })?;
        controller.SetIsVisible(true)?;

        let webview: ICoreWebView2 = controller.CoreWebView2()?;
        inject_bootstrap(&webview)?;
        attach_ipc(&webview, hwnd)?;

        // Serve the orb page from the local font server (same origin as the font CSS).
        let html = super::html::generate_orb_html();
        let url = crate::overlay::html_components::font_manager::store_html_page(html)
            .unwrap_or_else(|| "about:blank".to_string());
        let url = windows061::core::HSTRING::from(url);
        webview.Navigate(PCWSTR(url.as_ptr()))?;

        device.Commit()?;

        Ok(DcompHost {
            _device: device,
            _target: target,
            _root: root,
            comp,
            webview,
        })
    }
}

/// Inject the `window.ipc` shim before any page script runs.
unsafe fn inject_bootstrap(webview: &ICoreWebView2) -> windows061::core::Result<()> {
    unsafe {
        let js = windows061::core::HSTRING::from(ORB_BOOTSTRAP);
        let webview = webview.clone(); // owned clone → the 'static completion handler can hold it
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                webview
                    .AddScriptToExecuteOnDocumentCreated(PCWSTR(js.as_ptr()), &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(|code, _id| code),
        )
        .map_err(to_win_err)
    }
}

/// Route the page's `window.ipc.postMessage` calls (region / placement / ready) to the
/// orb's IPC handler. The page posts `JSON.stringify(...)` — a STRING. Read it as a string,
/// NOT as JSON (`WebMessageAsJson` would double-encode it into `"\"{...}\""` and parsing
/// would fail).
unsafe fn attach_ipc(webview: &ICoreWebView2, hwnd: HWND) -> windows061::core::Result<()> {
    unsafe {
        let hwnd_val = hwnd.0 as isize; // capture as isize (HWND's raw pointer isn't Send)
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_wv, args| {
            if let Some(args) = args {
                let mut msg = windows061::core::PWSTR(std::ptr::null_mut());
                if args.TryGetWebMessageAsString(&mut msg).is_ok() && !msg.is_null() {
                    let body = webview2_com::CoTaskMemPWSTR::from(msg).to_string();
                    super::ipc::handle_orb_ipc(HWND(hwnd_val as *mut std::ffi::c_void), &body);
                }
            }
            Ok(())
        }));
        let mut token = Default::default(); // type inferred from add_WebMessageReceived's signature
        webview.add_WebMessageReceived(&handler, &mut token)
    }
}

fn to_win_err(e: webview2_com::Error) -> windows061::core::Error {
    match e {
        webview2_com::Error::WindowsError(err) => err,
        other => windows061::core::Error::new(
            windows061::Win32::Foundation::E_FAIL,
            format!("{other:?}"),
        ),
    }
}

fn err_pointer() -> windows061::core::Error {
    windows061::core::Error::from(windows061::Win32::Foundation::E_POINTER)
}
