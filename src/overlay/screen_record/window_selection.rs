// --- WINDOW SELECTION OVERLAY ---
// Native full-screen transparent overlay for picking a window to record.
// Spawns a dedicated OS thread with its own Win32 message loop and WRY WebView.

use crate::overlay::screen_record::SR_HWND;
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::num::NonZeroIsize;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Once;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{Rect, WebContext, WebViewBuilder};

// Must match the constant in mod.rs.
const WM_APP_RUN_SCRIPT: u32 = WM_USER + 112;

static REGISTER_SELECTOR_CLASS: Once = Once::new();
static SELECTOR_HWND: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    static SELECTOR_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> =
        const { std::cell::RefCell::new(None) };
    static SELECTOR_WEB_CONTEXT: std::cell::RefCell<Option<WebContext>> =
        const { std::cell::RefCell::new(None) };
}

struct HwndWrapper(HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0 .0 as isize;
        if hwnd == 0 {
            return Err(HandleError::Unavailable);
        }
        if let Some(non_zero) = NonZeroIsize::new(hwnd) {
            let mut handle = Win32WindowHandle::new(non_zero);
            handle.hinstance = None;
            let raw = RawWindowHandle::Win32(handle);
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        } else {
            Err(HandleError::Unavailable)
        }
    }
}

unsafe extern "system" fn selector_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            SELECTOR_WEBVIEW.with(|wv| {
                if let Some(webview) = wv.borrow().as_ref() {
                    let mut r = RECT::default();
                    let _ = GetClientRect(hwnd, &mut r);
                    let _ = webview.set_bounds(Rect {
                        position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(
                            0, 0,
                        )),
                        size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                            (r.right - r.left) as u32,
                            (r.bottom - r.top) as u32,
                        )),
                    });
                }
            });
            LRESULT(0)
        }
        WM_APP_RUN_SCRIPT => {
            let script_ptr = lparam.0 as *mut String;
            if !script_ptr.is_null() {
                let script = Box::from_raw(script_ptr);
                SELECTOR_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.evaluate_script(&script);
                    }
                });
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            SELECTOR_HWND.store(0, Ordering::SeqCst);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Returns true if the selector overlay has been closed (or was never opened).
pub fn selector_is_closed() -> bool {
    SELECTOR_HWND.load(Ordering::SeqCst) == 0
}

/// Push a thumbnail data-URL for a specific window into the live overlay.
pub fn post_thumbnail_update(window_id: usize, data_url: String) {
    let val = SELECTOR_HWND.load(Ordering::SeqCst);
    if val == 0 {
        return;
    }
    // Escape backticks so the data URL is safe inside a JS template literal.
    let safe_url = data_url.replace('`', "");
    let script = format!("window.updateThumb('{}',`{}`);", window_id, safe_url);
    let script_ptr = Box::into_raw(Box::new(script));
    unsafe {
        let _ = PostMessageW(
            Some(HWND(val as *mut _)),
            WM_APP_RUN_SCRIPT,
            WPARAM(0),
            LPARAM(script_ptr as isize),
        );
    }
}

fn generate_html(
    windows: &[serde_json::Value],
    font_css: &str,
    is_dark: bool,
    title: &str,
    subtitle: &str,
) -> String {
    // Escape </script> sequences to prevent premature tag closure.
    let windows_json = serde_json::to_string(windows)
        .unwrap_or_else(|_| "[]".to_string())
        .replace("</", "<\\/");

    let count = windows.len();

    let (
        overlay_bg,
        card_bg,
        card_border,
        card_hover_border,
        thumb_bg,
        title_color,
        subtitle_color,
        proc_color,
        close_color,
        close_hover_color,
        wave_color,
        header_color,
        scroll_thumb,
    ) = if is_dark {
        (
            "rgba(10,10,12,0.88)",
            "rgba(255,255,255,0.07)",
            "rgba(255,255,255,0.10)",
            "rgba(59,130,246,0.70)",
            "rgba(255,255,255,0.04)",
            "#fff",
            "rgba(255,255,255,0.40)",
            "rgba(255,255,255,0.32)",
            "rgba(255,255,255,0.45)",
            "#fff",
            "#60a5fa",
            "rgba(255,255,255,0.52)",
            "rgba(255,255,255,0.18)",
        )
    } else {
        (
            "rgba(240,242,247,0.92)",
            "rgba(0,0,0,0.04)",
            "rgba(0,0,0,0.09)",
            "rgba(37,99,235,0.65)",
            "rgba(0,0,0,0.05)",
            "#0f172a",
            "rgba(0,0,0,0.42)",
            "rgba(0,0,0,0.38)",
            "rgba(0,0,0,0.38)",
            "#0f172a",
            "#1d4ed8",
            "rgba(0,0,0,0.42)",
            "rgba(0,0,0,0.22)",
        )
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en" data-theme="{theme}">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>Select Window</title>
<style>
{font_css}
*{{box-sizing:border-box;margin:0;padding:0}}
html,body{{width:100%;height:100%;overflow:hidden;font-family:'Google Sans Flex','Segoe UI',system-ui,sans-serif;font-variation-settings:'ROND' 100;background:transparent}}

::-webkit-scrollbar{{width:6px}}
::-webkit-scrollbar-track{{background:transparent}}
::-webkit-scrollbar-thumb{{background:{scroll_thumb};border-radius:3px}}

@keyframes overlayIn{{
  from{{opacity:0;transform:scale(0.98)}}
  to{{opacity:1;transform:scale(1)}}
}}
@keyframes cardIn{{
  from{{opacity:0;transform:translateY(10px)}}
  to{{opacity:1;transform:translateY(0)}}
}}
@keyframes waveColor{{
  0%,100%{{color:{header_color};font-variation-settings:'GRAD' 0,'wght' 600,'ROND' 100}}
  50%{{color:{wave_color};font-variation-settings:'GRAD' 200,'wght' 900,'ROND' 100}}
}}
@keyframes thumbFadeIn{{
  from{{opacity:0}}
  to{{opacity:1}}
}}

.overlay{{
  position:fixed;inset:0;
  background:{overlay_bg};
  backdrop-filter:blur(14px);-webkit-backdrop-filter:blur(14px);
  display:flex;flex-direction:column;align-items:center;
  padding:50px 28px 28px;overflow-y:auto;
  animation:overlayIn 0.22s cubic-bezier(0.2,0,0,1) forwards;
}}

.close-btn{{
  position:fixed;top:14px;right:18px;
  width:32px;height:32px;
  display:flex;align-items:center;justify-content:center;
  cursor:pointer;color:{close_color};font-size:20px;line-height:1;
  transition:color 0.12s;z-index:10;user-select:none;
  background:none;border:none;padding:0;
}}
.close-btn:hover{{color:{close_hover_color}}}

.header{{text-align:center;margin-bottom:24px;flex-shrink:0}}
.title{{
  color:{title_color};
  font-size:28px;font-weight:600;
  font-stretch:130%;text-transform:uppercase;
  letter-spacing:0.12em;
  margin-bottom:6px;
  font-family:'Google Sans Flex','Segoe UI',system-ui,sans-serif;
  font-variation-settings:'wght' 600,'ROND' 100;
  line-height:1.15
}}
.subtitle{{
  color:{subtitle_color};font-size:12px;
  font-variation-settings:'wght' 400,'ROND' 80;
  line-height:1.7
}}
.win-count{{
  display:block;color:{proc_color};font-size:11px;
  font-variation-settings:'wght' 400,'ROND' 80
}}

/* align-items:start = each card is only as tall as its content,
   prevents grid from stretching shorter cards to the tallest row height */
.grid{{
  display:grid;
  grid-template-columns:repeat(auto-fill,minmax(200px,1fr));
  gap:12px;width:100%;max-width:1280px;
  align-items:start
}}

.card{{
  background:{card_bg};border:1px solid {card_border};
  border-radius:10px;overflow:hidden;cursor:pointer;
  transition:background 0.12s,border-color 0.12s,transform 0.12s,box-shadow 0.12s;
  user-select:none;
  opacity:0; /* set by JS stagger animation */
}}
.card:hover{{
  border-color:{card_hover_border};
  transform:translateY(-3px);
  box-shadow:0 10px 28px rgba(0,0,0,0.22)
}}
.card:active{{transform:translateY(-1px)}}
.card.admin-blocked{{opacity:0.35!important;cursor:not-allowed}}
.card.admin-blocked:hover{{transform:none;box-shadow:none;border-color:{card_border}}}

.thumb{{
  width:100%;background:{thumb_bg};
  display:flex;align-items:center;justify-content:center;
  overflow:hidden;position:relative;
  /* Hard cap: no thumbnail taller than 160px regardless of aspect ratio */
  max-height:160px
}}
.thumb img{{
  width:100%;height:100%;object-fit:cover;display:block;
  /* Show the top of the window — most useful region for identification */
  object-position:center top;
  animation:thumbFadeIn 0.3s ease forwards
}}
.thumb-ph{{
  position:absolute;inset:0;
  display:flex;align-items:center;justify-content:center;
  color:rgba(128,128,128,0.20);font-size:24px
}}

.info{{padding:8px 10px;display:flex;align-items:center;gap:7px}}
.icon{{width:16px;height:16px;flex-shrink:0;border-radius:2px;object-fit:contain}}
.icon-ph{{width:16px;height:16px;background:rgba(128,128,128,0.12);border-radius:2px;flex-shrink:0}}
.text{{flex:1;min-width:0}}
.win-title{{
  color:{title_color};font-size:11.5px;font-weight:500;
  white-space:nowrap;overflow:hidden;text-overflow:ellipsis;
  font-variation-settings:'wght' 500,'ROND' 100
}}
.proc-name{{
  color:{proc_color};font-size:10px;
  white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-top:1px
}}
.admin-badge{{
  background:rgba(239,68,68,0.16);color:rgb(239,68,68);
  border:1px solid rgba(239,68,68,0.28);
  border-radius:3px;font-size:8px;font-weight:700;
  padding:1px 4px;letter-spacing:0.07em;text-transform:uppercase;flex-shrink:0
}}
</style>
</head>
<body>
<button class="close-btn" id="close-btn" title="Cancel" aria-label="Cancel">&#x2715;</button>
<div class="overlay" id="overlay">
  <div class="header">
    <div class="title" id="title-text">{title}</div>
    <div class="subtitle">{subtitle}<span class="win-count">{count} windows</span></div>
  </div>
  <div class="grid" id="grid"></div>
</div>
<script>
var windows={windows_json};

// Wave-animate the title characters on load
(function(){{
  var el=document.getElementById('title-text');
  if(!el) return;
  el.innerHTML=el.innerText.split('').map(function(ch,i){{
    return '<span style="display:inline-block;animation:waveColor 0.65s ease forwards '+(0.08+i*0.035).toFixed(3)+'s">'+
      (ch===' '?'&nbsp;':ch.replace(/&/g,'&amp;').replace(/</g,'&lt;'))+'</span>';
  }}).join('');
}})();

function selectWindow(id){{window.ipc.postMessage('select:'+id);}}
function cancel(){{window.ipc.postMessage('cancel');}}

var grid=document.getElementById('grid');
var overlay=document.getElementById('overlay');

document.getElementById('close-btn').addEventListener('click',cancel);

windows.forEach(function(w,idx){{
  // Clamp aspect ratio: min 1.0 (square) … max 2.8 (ultra-wide).
  // Very tall windows (chat bars, tool panels) would otherwise dominate the row.
  var raw=(w.winW&&w.winH)?(w.winW/w.winH):(16/9);
  var ar=Math.min(Math.max(raw,1.0),2.8);

  var card=document.createElement('div');
  card.className='card'+(w.isAdmin?' admin-blocked':'');
  card.style.animation='cardIn 0.22s cubic-bezier(0.2,0,0,1) forwards '+(0.04+idx*0.022).toFixed(3)+'s';

  // Thumbnail area — clamped window aspect ratio
  var thumb=document.createElement('div');
  thumb.className='thumb';
  thumb.style.aspectRatio=ar.toFixed(4);
  thumb.id='thumb-wrap-'+w.id;

  var ph=document.createElement('div');ph.className='thumb-ph';ph.textContent='\u25a3';
  thumb.appendChild(ph);

  // Pre-populate if thumbnail already present (future-proofing)
  if(w.previewDataUrl){{
    var img=document.createElement('img');img.src=w.previewDataUrl;img.alt='';
    thumb.appendChild(img);ph.style.display='none';
  }}

  var info=document.createElement('div');info.className='info';
  if(w.iconDataUrl){{
    var ic=document.createElement('img');ic.className='icon';ic.src=w.iconDataUrl;ic.alt='';info.appendChild(ic);
  }}else{{
    var iph=document.createElement('div');iph.className='icon-ph';info.appendChild(iph);
  }}
  var text=document.createElement('div');text.className='text';
  var t=document.createElement('div');t.className='win-title';t.textContent=w.title;t.title=w.title;
  var p=document.createElement('div');p.className='proc-name';p.textContent=w.processName;
  text.appendChild(t);text.appendChild(p);info.appendChild(text);
  if(w.isAdmin){{
    var badge=document.createElement('span');badge.className='admin-badge';badge.textContent='ADMIN';info.appendChild(badge);
  }}
  card.appendChild(thumb);card.appendChild(info);
  if(!w.isAdmin){{card.addEventListener('click',function(){{selectWindow(w.id);}});}}
  grid.appendChild(card);
}});

// Called by Rust background thread once each thumbnail is ready
window.updateThumb=function(id,dataUrl){{
  var wrap=document.getElementById('thumb-wrap-'+id);
  if(!wrap) return;
  var ph=wrap.querySelector('.thumb-ph');
  if(ph) ph.style.display='none';
  var existing=wrap.querySelector('img');
  if(existing){{existing.src=dataUrl;return;}}
  var img=document.createElement('img');img.src=dataUrl;img.alt='';
  wrap.appendChild(img);
}};

document.addEventListener('keydown',function(e){{if(e.key==='Escape')cancel();}});
overlay.addEventListener('click',function(e){{if(e.target===overlay)cancel();}});
</script>
</body>
</html>"##,
        theme = if is_dark { "dark" } else { "light" },
        font_css = font_css,
        overlay_bg = overlay_bg,
        card_bg = card_bg,
        card_border = card_border,
        card_hover_border = card_hover_border,
        thumb_bg = thumb_bg,
        title_color = title_color,
        subtitle_color = subtitle_color,
        proc_color = proc_color,
        close_color = close_color,
        close_hover_color = close_hover_color,
        wave_color = wave_color,
        header_color = header_color,
        scroll_thumb = scroll_thumb,
        title = title,
        subtitle = subtitle,
        count = count,
        windows_json = windows_json,
    )
}

/// Opens a full-screen transparent native overlay window displaying the given list of capturable windows.
/// When the user picks a window, fires `external-window-selected` in the SR WebView.
/// When cancelled, simply closes the overlay.
pub fn show_window_selector(windows_data: Vec<serde_json::Value>, is_dark: bool, lang: String) {
    // Close any existing selector first.
    let existing = SELECTOR_HWND.load(Ordering::SeqCst);
    if existing != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(existing as *mut _)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    std::thread::spawn(move || unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => return,
        };

        REGISTER_SELECTOR_CLASS.call_once(|| {
            let _ = RegisterClassExW(&WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(selector_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: windows::core::w!("SRWindowSelectorClass"),
                // Null brush — window is fully transparent via DWM layering.
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            });
        });

        // Cover the entire virtual desktop (all monitors).
        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let hwnd = match CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            windows::core::w!("SRWindowSelectorClass"),
            windows::core::w!(""),
            WS_POPUP | WS_VISIBLE,
            screen_x,
            screen_y,
            screen_w,
            screen_h,
            None,
            None,
            Some(hinstance.into()),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[WindowSelector] CreateWindowExW failed: {e}");
                return;
            }
        };

        // Enable per-pixel alpha compositing so the WebView can show a transparent/blurred background.
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        SELECTOR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // Capture SR HWND value for posting events back to the main SR WebView.
        // SR_HWND is SendHwnd(HWND), so .0 gives HWND and .0.0 gives *mut c_void.
        let sr_hwnd_val = std::ptr::addr_of!(SR_HWND).read().0 .0 as isize;

        // Use the shared WebContext (same profile as other overlays).
        SELECTOR_WEB_CONTEXT.with(|c| {
            if c.borrow().is_none() {
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *c.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        // Resolve locale strings for the current language.
        let locale = crate::gui::locale::LocaleText::get(&lang);
        let title = locale.win_select_title.to_string();
        let subtitle = locale.win_select_subtitle.to_string();

        // Build HTML with Google Sans Flex font embedded via font manager.
        let font_css = crate::overlay::html_components::font_manager::get_font_css();
        let html = generate_html(&windows_data, &font_css, is_dark, &title, &subtitle);

        // Serve HTML via font manager's local HTTP server for same-origin font loading.
        let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
            .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

        let wrapper = HwndWrapper(hwnd);

        let webview_result = {
            let _lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
            SELECTOR_WEB_CONTEXT.with(|ctx_cell| {
                let mut ctx_ref = ctx_cell.borrow_mut();
                let builder = WebViewBuilder::new_with_web_context(ctx_ref.as_mut().unwrap());
                let builder =
                    crate::overlay::html_components::font_manager::configure_webview(builder);
                builder
                    .with_transparent(true)
                    .with_background_color((0, 0, 0, 0))
                    .with_url(&page_url)
                    .with_ipc_handler(move |msg: wry::http::Request<String>| {
                        let body = msg.body().as_str().to_string();
                        let sel_hwnd_val = SELECTOR_HWND.load(Ordering::SeqCst);

                        if let Some(window_id) = body.strip_prefix("select:") {
                            // Sanitise the window ID (numeric string, safe to interpolate).
                            let window_id: String =
                                window_id.chars().filter(|c| c.is_ascii_digit()).collect();
                            let script = format!(
                                "window.dispatchEvent(new CustomEvent(\
                                 'external-window-selected',\
                                 {{detail:{{windowId:'{}'}}}}))",
                                window_id
                            );
                            let script_ptr = Box::into_raw(Box::new(script));
                            let _ = PostMessageW(
                                Some(HWND(sr_hwnd_val as *mut _)),
                                WM_APP_RUN_SCRIPT,
                                WPARAM(0),
                                LPARAM(script_ptr as isize),
                            );
                        }

                        // Close the overlay after any IPC message (select or cancel).
                        if sel_hwnd_val != 0 {
                            let _ = PostMessageW(
                                Some(HWND(sel_hwnd_val as *mut _)),
                                WM_CLOSE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                    })
                    .build_as_child(&wrapper)
            })
        };

        let webview = match webview_result {
            Ok(wv) => wv,
            Err(e) => {
                eprintln!("[WindowSelector] WebView build failed: {e}");
                SELECTOR_HWND.store(0, Ordering::SeqCst);
                let _ = DestroyWindow(hwnd);
                return;
            }
        };

        // Size the WebView to fill the window.
        let _ = webview.set_bounds(Rect {
            position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                screen_w as u32,
                screen_h as u32,
            )),
        });

        SELECTOR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = Some(webview);
        });

        // Run the message loop for this window.
        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                -1 | 0 => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        // Drop WebView then WebContext (order matters for WebView2 cleanup).
        SELECTOR_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = None;
        });
        SELECTOR_WEB_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = None;
        });
    });
}
