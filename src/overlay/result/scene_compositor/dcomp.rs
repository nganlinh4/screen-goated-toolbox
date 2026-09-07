use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS, COREWEBVIEW2_COLOR, COREWEBVIEW2_MOUSE_EVENT_KIND,
        COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2,
        ICoreWebView2CompositionController, ICoreWebView2Controller, ICoreWebView2Controller2,
        ICoreWebView2Controller3, ICoreWebView2Environment3,
    },
    NavigationCompletedEventHandler, ProcessFailedEventHandler, WebMessageReceivedEventHandler,
};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::UI::WindowsAndMessaging::HCURSOR;
use windows::core::{BOOL, Interface, PCWSTR};

const BOOTSTRAP: &str = r#"
window.ipc={postMessage:function(m){window.chrome.webview.postMessage(m);}};
window.addEventListener('error',function(e){window.ipc.postMessage(JSON.stringify({type:'renderer_error',source:'page',error:String(e.message)+' @ '+e.lineno+':'+e.colno}));});
window.addEventListener('unhandledrejection',function(e){window.ipc.postMessage(JSON.stringify({type:'renderer_error',source:'promise',error:String(e.reason)}));});
"#;

pub(super) struct DcompHost {
    heartbeat_at: std::cell::Cell<std::time::Instant>,
    _device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _root: IDCompositionVisual,
    pub comp: ICoreWebView2CompositionController,
    pub webview: ICoreWebView2,
}

impl DcompHost {
    pub(super) fn poll_heartbeat(&self) {
        // Native scheduling also probes hidden pages whose JavaScript timers are throttled.
        if self.heartbeat_at.get().elapsed() >= std::time::Duration::from_secs(1) {
            self.heartbeat_at.set(std::time::Instant::now());
            self.evaluate_script("window.ipc.postMessage('renderer_heartbeat');");
        }
    }

    pub(super) fn set_visible(&self, visible: bool) -> windows::core::Result<()> {
        unsafe {
            let controller: ICoreWebView2Controller = self.comp.cast()?;
            controller.SetIsVisible(visible)?;
            controller.NotifyParentWindowPositionChanged()
        }
    }

    pub(super) fn update_display(
        &self,
        width: i32,
        height: i32,
        scale: f64,
    ) -> windows::core::Result<()> {
        unsafe {
            let controller: ICoreWebView2Controller = self.comp.cast()?;
            if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
                let _ = controller3.SetRasterizationScale(scale);
            }
            controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            })
        }
    }

    pub(super) fn evaluate_script(&self, script: &str) {
        let script = windows::core::HSTRING::from(script);
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|_code, _result| Ok(())));
        unsafe {
            let _ = self
                .webview
                .ExecuteScript(PCWSTR(script.as_ptr()), &handler);
        }
    }

    pub(super) fn send_mouse_input(
        &self,
        kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
        virtual_keys: COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
        mouse_data: u32,
        point: POINT,
    ) {
        unsafe {
            let _ = self
                .comp
                .SendMouseInput(kind, virtual_keys, mouse_data, point);
        }
    }

    pub(super) fn cursor(&self, cur: &mut HCURSOR) -> windows::core::Result<()> {
        unsafe { self.comp.Cursor(cur) }
    }

    pub(super) fn move_focus(&self) {
        unsafe {
            if let Ok(controller) = self.comp.cast::<ICoreWebView2Controller>() {
                let _ = controller.MoveFocus(
                    webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
                );
            }
        }
    }
}

pub(super) fn build_host(
    hwnd: HWND,
    software_rendering: bool,
    width: i32,
    height: i32,
    scale: f64,
    page_url: &str,
) -> windows::core::Result<DcompHost> {
    unsafe {
        let device = create_composition_device(software_rendering)?;
        let target: IDCompositionTarget = device.CreateTargetForHwnd(hwnd, true)?;
        let root: IDCompositionVisual = device.CreateVisual()?;
        target.SetRoot(&root)?;

        let profile_name = if software_rendering {
            "result-compositor-software"
        } else {
            crate::overlay::webview_runtime::Profile::ResultCompositor.directory()
        };
        let user_data = crate::overlay::webview_runtime::data_dir_named(Some(profile_name));
        let user_data = windows::core::HSTRING::from(user_data.to_string_lossy().as_ref());

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
                        .CreateCoreWebView2CompositionController(hwnd, &handler)
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
        composition.SetRootVisualTarget(&root)?;

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
        controller.SetIsVisible(false)?;

        let webview = controller.CoreWebView2()?;
        inject_bootstrap(&webview)?;
        attach_ipc(&webview)?;
        attach_process_failure(&webview)?;
        attach_navigation_probe(&webview)?;

        let page_url_hstring = windows::core::HSTRING::from(page_url);
        webview.Navigate(PCWSTR(page_url_hstring.as_ptr()))?;
        device.Commit()?;

        Ok(DcompHost {
            heartbeat_at: std::cell::Cell::new(std::time::Instant::now()),
            _device: device,
            _target: target,
            _root: root,
            comp: composition,
            webview,
        })
    }
}

unsafe fn attach_process_failure(webview: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let handler = ProcessFailedEventHandler::create(Box::new(move |_webview, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            args.ProcessFailedKind(&mut kind)?;
            let kind = match kind {
                COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => {
                    Some(super::protocol::RendererFailureKind::BrowserProcessExited)
                }
                COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => {
                    Some(super::protocol::RendererFailureKind::RenderProcessExited)
                }
                COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => {
                    Some(super::protocol::RendererFailureKind::RenderProcessUnresponsive)
                }
                COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => {
                    Some(super::protocol::RendererFailureKind::FrameRenderProcessExited)
                }
                COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => {
                    Some(super::protocol::RendererFailureKind::GpuProcessExited)
                }
                _ => None,
            };
            if let Some(kind) = kind {
                super::child::emit_event(super::protocol::ChildEvent::RendererFailure { kind });
            }
            Ok(())
        }));
        let mut token = Default::default();
        webview.add_ProcessFailed(&handler, &mut token)
    }
}

fn create_composition_device(
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
    if software_rendering {
        try_create(D3D_DRIVER_TYPE_WARP)
    } else {
        let [hardware, warp] = driver_candidates();
        try_create(hardware).or_else(|_hardware_error| try_create(warp))
    }
}

fn driver_candidates() -> [D3D_DRIVER_TYPE; 2] {
    [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP]
}

unsafe fn attach_navigation_probe(webview: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let handler = NavigationCompletedEventHandler::create(Box::new(move |webview, args| {
            let mut success_value = BOOL::default();
            let success = args.as_ref().is_some_and(|args| {
                args.IsSuccess(&mut success_value).is_ok() && success_value.as_bool()
            });
            let Some(webview) = webview else {
                return Ok(());
            };
            let probe = windows::core::HSTRING::from(
                "JSON.stringify({state:document.readyState,bridge:typeof window.ipc,apply:typeof window.applyHostCommand,frames:document.querySelectorAll('iframe').length})",
            );
            let completion = ExecuteScriptCompletedHandler::create(Box::new(
                move |code, result| {
                    crate::debug_log::log_debug(&format!(
                        "[ResultCompositor] navigation success={success} script_code={code:?} probe={result}"
                    ));
                    Ok(())
                },
            ));
            webview.ExecuteScript(PCWSTR(probe.as_ptr()), &completion)
        }));
        let mut token = Default::default();
        webview.add_NavigationCompleted(&handler, &mut token)
    }
}

unsafe fn inject_bootstrap(webview: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let script = windows::core::HSTRING::from(BOOTSTRAP);
        let webview = webview.clone();
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                webview
                    .AddScriptToExecuteOnDocumentCreated(PCWSTR(script.as_ptr()), &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(|code, _| code),
        )
        .map_err(webview_error)
    }
}

unsafe fn attach_ipc(webview: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
            if let Some(args) = args {
                let mut message = windows::core::PWSTR(std::ptr::null_mut());
                if args.TryGetWebMessageAsString(&mut message).is_ok() && !message.is_null() {
                    let body = webview2_com::CoTaskMemPWSTR::from(message).to_string();
                    super::child::handle_renderer_event(&body);
                }
            }
            Ok(())
        }));
        let mut token = Default::default();
        webview.add_WebMessageReceived(&handler, &mut token)
    }
}

fn webview_error(error: webview2_com::Error) -> windows::core::Error {
    match error {
        webview2_com::Error::WindowsError(error) => error,
        other => {
            windows::core::Error::new(windows::Win32::Foundation::E_FAIL, format!("{other:?}"))
        }
    }
}

fn pointer_error() -> windows::core::Error {
    windows::core::Error::from(windows::Win32::Foundation::E_POINTER)
}
