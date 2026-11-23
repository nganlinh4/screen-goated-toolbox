use crate::overlay::compositor_renderer::{
    RENDERER_BOOTSTRAP, add_document_script, attach_navigation_probe, attach_process_failure,
    create_composition_device, create_composition_webview,
};
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2CompositionController, ICoreWebView2Controller,
        ICoreWebView2Controller3,
    },
    WebMessageReceivedEventHandler,
};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::core::{Interface, PCWSTR};

const NAVIGATION_PROBE: &str = "JSON.stringify({state:document.readyState,bridge:typeof window.ipc,apply:typeof window.applyStatusCommand,frames:document.querySelectorAll('iframe').length})";

pub(super) struct DcompHost {
    controller_parent: HWND,
    _device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _root: IDCompositionVisual,
    pub comp: ICoreWebView2CompositionController,
    pub webview: ICoreWebView2,
}

impl DcompHost {
    pub(super) fn update_display(
        &self,
        display: super::DisplayMetrics,
    ) -> windows::core::Result<()> {
        crate::overlay::composition_anchor::resize(
            self.controller_parent,
            display.width,
            display.height,
        )?;
        unsafe {
            let controller: ICoreWebView2Controller = self.comp.cast()?;
            if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
                controller3.SetRasterizationScale(display.scale)?;
            }
            controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: display.width,
                bottom: display.height,
            })
        }
    }
}

pub(super) fn build_host(hwnd: HWND) -> windows::core::Result<DcompHost> {
    unsafe {
        let device = create_composition_device(false)?;
        let target: IDCompositionTarget = device.CreateTargetForHwnd(hwnd, true)?;
        let root: IDCompositionVisual = device.CreateVisual()?;
        target.SetRoot(&root)?;

        let user_data = crate::overlay::webview_runtime::data_dir(
            crate::overlay::webview_runtime::Profile::StatusCompositor,
        );
        let display = super::display_metrics(hwnd);
        let controller_parent =
            crate::overlay::composition_anchor::create(hwnd, display.width, display.height)?;
        let (composition, controller) = create_composition_webview(
            &user_data,
            controller_parent,
            &root,
            (display.width, display.height),
            display.scale,
        )?;
        controller.SetIsVisible(true)?;

        let webview = controller.CoreWebView2()?;
        add_document_script(&webview, RENDERER_BOOTSTRAP)?;
        attach_ipc(&webview)?;
        attach_process_failure(&webview, |kind| {
            super::child::emit_event(super::protocol::ChildEvent::RendererFailure { kind });
        })?;
        attach_navigation_probe(&webview, NAVIGATION_PROBE, |line| eprintln!("{line}"))?;
        let html = super::html::document();
        let page_url = crate::overlay::html_components::font_manager::store_html_page(html)
            .unwrap_or_else(|| "about:blank".to_string());
        let page_url = windows::core::HSTRING::from(page_url);
        webview.Navigate(PCWSTR(page_url.as_ptr()))?;
        device.Commit()?;

        Ok(DcompHost {
            controller_parent,
            _device: device,
            _target: target,
            _root: root,
            comp: composition,
            webview,
        })
    }
}

unsafe fn attach_ipc(webview: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
            if let Some(args) = args {
                let mut message = windows::core::PWSTR(std::ptr::null_mut());
                if args.TryGetWebMessageAsString(&mut message).is_ok() && !message.is_null() {
                    let body = webview2_com::CoTaskMemPWSTR::from(message).to_string();
                    super::child::handle_renderer_message(&body);
                }
            }
            Ok(())
        }));
        let mut token = Default::default();
        webview.add_WebMessageReceived(&handler, &mut token)
    }
}
