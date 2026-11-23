use crate::overlay::compositor_renderer::{
    RENDERER_BOOTSTRAP, add_document_script, attach_navigation_probe, attach_process_failure,
    create_composition_device, create_composition_webview,
};
use anyhow::Context;
use webview2_com::{
    ExecuteScriptCompletedHandler, FrameCreatedEventHandler, FrameNavigationStartingEventHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS, ICoreWebView2,
        ICoreWebView2_4, ICoreWebView2CompositionController, ICoreWebView2Controller,
        ICoreWebView2Controller3, ICoreWebView2Frame2,
    },
    NavigationStartingEventHandler, WebMessageReceivedEventHandler,
};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::UI::WindowsAndMessaging::HCURSOR;
use windows::core::{Interface, PCWSTR};

const NAVIGATION_PROBE: &str = "JSON.stringify({state:document.readyState,bridge:typeof window.ipc,apply:typeof window.applyHostCommand,frames:document.querySelectorAll('iframe').length})";

pub(super) struct DcompHost {
    heartbeat_at: std::cell::Cell<std::time::Instant>,
    visible: std::cell::Cell<bool>,
    controller_parent: HWND,
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
        if self.visible.get() == visible {
            return Ok(());
        }
        unsafe {
            let controller: ICoreWebView2Controller = self.comp.cast()?;
            controller.SetIsVisible(visible)?;
            self.visible.set(visible);
            Ok(())
        }
    }

    pub(super) fn update_display(
        &self,
        width: i32,
        height: i32,
        scale: f64,
    ) -> windows::core::Result<()> {
        crate::overlay::composition_anchor::resize(self.controller_parent, width, height)?;
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
        self.submit_script(script);
    }

    pub(super) fn submit_script(&self, script: &str) -> bool {
        let script = windows::core::HSTRING::from(script);
        let handler = ExecuteScriptCompletedHandler::create(Box::new(|code, _result| {
            if let Err(error) = code {
                report_script_failure(error);
            }
            Ok(())
        }));
        unsafe {
            match self
                .webview
                .ExecuteScript(PCWSTR(script.as_ptr()), &handler)
            {
                Ok(()) => true,
                Err(error) => {
                    report_script_failure(error);
                    false
                }
            }
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
) -> anyhow::Result<DcompHost> {
    unsafe {
        let device = create_composition_device(software_rendering)
            .context("create compositor graphics device")?;
        let target: IDCompositionTarget = device
            .CreateTargetForHwnd(hwnd, true)
            .context("create compositor window target")?;
        let root: IDCompositionVisual = device.CreateVisual()?;
        target.SetRoot(&root)?;

        let profile_name = if software_rendering {
            "result-compositor-software"
        } else {
            crate::overlay::webview_runtime::Profile::ResultCompositor.directory()
        };
        let user_data = crate::overlay::webview_runtime::data_dir_named(Some(profile_name));
        let controller_parent = crate::overlay::composition_anchor::create(hwnd, width, height)?;
        let (composition, controller) = create_composition_webview(
            &user_data,
            controller_parent,
            &root,
            (width, height),
            scale,
        )
        .context("create compositor WebView2 composition controller")?;
        controller.SetIsVisible(false)?;

        let webview = controller.CoreWebView2()?;
        // Shared overlay geometry is authored in physical pixels. Browser zoom
        // must not independently scale the scene underneath its native regions.
        controller.SetZoomFactor(1.0)?;
        let settings = webview.Settings()?;
        settings.SetIsZoomControlEnabled(false)?;
        let settings5: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings5 =
            settings.cast()?;
        settings5.SetIsPinchZoomEnabled(false)?;
        add_document_script(&webview, RENDERER_BOOTSTRAP)?;
        attach_ipc(&webview, page_url)?;
        attach_navigation_guard(&webview, page_url)?;
        attach_card_frame_guard(&webview)?;
        attach_process_failure(&webview, |kind| {
            super::child::emit_event(super::protocol::ChildEvent::RendererFailure { kind });
        })?;
        attach_navigation_probe(&webview, NAVIGATION_PROBE, |line| {
            crate::debug_log::log_debug(&format!("[ResultCompositor] {line}"));
        })?;

        let page_url_hstring = windows::core::HSTRING::from(page_url);
        webview.Navigate(PCWSTR(page_url_hstring.as_ptr()))?;
        device.Commit()?;

        Ok(DcompHost {
            heartbeat_at: std::cell::Cell::new(std::time::Instant::now()),
            visible: std::cell::Cell::new(false),
            controller_parent,
            _device: device,
            _target: target,
            _root: root,
            comp: composition,
            webview,
        })
    }
}

unsafe fn attach_navigation_guard(
    webview: &ICoreWebView2,
    page_url: &str,
) -> windows::core::Result<()> {
    let allowed_url = page_url.to_owned();
    unsafe {
        let handler = NavigationStartingEventHandler::create(Box::new(move |_webview, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut uri = windows::core::PWSTR::null();
            let allowed = if args.Uri(&mut uri).is_ok() && !uri.is_null() {
                webview2_com::CoTaskMemPWSTR::from(uri).to_string() == allowed_url
            } else {
                false
            };
            args.SetCancel(!allowed)
        }));
        let mut token = Default::default();
        webview.add_NavigationStarting(&handler, &mut token)
    }
}

#[derive(Default)]
struct CardFrameNavigationGuard {
    started: bool,
}

impl CardFrameNavigationGuard {
    fn allow(&mut self, uri: &str) -> bool {
        let base = uri.split_once('#').map_or(uri, |(base, _)| base);
        if base == "about:srcdoc" || (base == "about:blank" && !self.started) {
            self.started = true;
            true
        } else {
            false
        }
    }
}

fn card_frame_guard_unavailable() -> ! {
    crate::crash_report::exit(1, "result-card-frame-guard-unavailable")
}

unsafe fn attach_card_frame_guard(webview: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let webview4: ICoreWebView2_4 = webview.cast()?;
        let handler = FrameCreatedEventHandler::create(Box::new(|_webview, args| {
            let Some(args) = args else {
                card_frame_guard_unavailable()
            };
            let frame = args
                .Frame()
                .unwrap_or_else(|_| card_frame_guard_unavailable());
            let mut name = windows::core::PWSTR::null();
            if frame.Name(&mut name).is_err() || name.is_null() {
                card_frame_guard_unavailable();
            }
            let name = webview2_com::CoTaskMemPWSTR::from(name).to_string();
            if !name.starts_with("sgt-result-card-") {
                return Ok(());
            }
            let frame: ICoreWebView2Frame2 = frame
                .cast()
                .unwrap_or_else(|_| card_frame_guard_unavailable());
            let mut guard = CardFrameNavigationGuard::default();
            let handler =
                FrameNavigationStartingEventHandler::create(Box::new(move |_frame, args| {
                    let Some(args) = args else {
                        card_frame_guard_unavailable()
                    };
                    let mut uri = windows::core::PWSTR::null();
                    let allowed = if args.Uri(&mut uri).is_ok() && !uri.is_null() {
                        guard.allow(&webview2_com::CoTaskMemPWSTR::from(uri).to_string())
                    } else {
                        false
                    };
                    args.SetCancel(!allowed)
                        .unwrap_or_else(|_| card_frame_guard_unavailable());
                    Ok(())
                }));
            let mut token = Default::default();
            frame
                .add_NavigationStarting(&handler, &mut token)
                .unwrap_or_else(|_| card_frame_guard_unavailable());
            Ok(())
        }));
        let mut token = Default::default();
        webview4.add_FrameCreated(&handler, &mut token)
    }
}

unsafe fn attach_ipc(webview: &ICoreWebView2, page_url: &str) -> windows::core::Result<()> {
    let allowed_url = page_url.to_owned();
    unsafe {
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
            if let Some(args) = args {
                let mut source = windows::core::PWSTR::null();
                if args.Source(&mut source).is_err() || source.is_null() {
                    return Ok(());
                }
                if webview2_com::CoTaskMemPWSTR::from(source).to_string() != allowed_url {
                    return Ok(());
                }
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

fn report_script_failure(error: windows::core::Error) {
    super::child::emit_event(super::protocol::ChildEvent::CommandError {
        command: "execute_script".into(),
        id: None,
        error: error.to_string(),
    });
}

#[cfg(test)]
mod frame_navigation_tests {
    use super::CardFrameNavigationGuard;

    #[test]
    fn card_frames_stay_on_their_own_document() {
        let mut guard = CardFrameNavigationGuard::default();
        assert!(guard.allow("about:blank"));
        assert!(guard.allow("about:srcdoc"));
        assert!(guard.allow("about:srcdoc#section"));
        for denied in [
            "https://example.com/",
            "data:text/html,hello",
            "javascript:alert(1)",
            "about:srcdoc?next=https://example.com",
            "about:blank",
        ] {
            assert!(!guard.allow(denied));
        }
    }
}
