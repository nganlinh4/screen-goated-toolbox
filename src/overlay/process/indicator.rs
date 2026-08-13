use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, IsWindow, MSG, PostMessageW, SendMessageW, TranslateMessage,
    WM_CLOSE, WM_TIMER,
};

pub struct ProcessingIndicator {
    hwnd: usize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ProcessingIndicator {
    pub fn show(rect: RECT, graphics_mode: String) -> Result<Self> {
        if rect.left >= rect.right || rect.top >= rect.bottom {
            bail!("processing indicator rectangle is empty");
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("sgt-processing-indicator".to_string())
            .spawn(move || {
                let hwnd = unsafe { super::window::create_processing_window(rect, graphics_mode) };
                let raw = hwnd.0 as usize;
                let _ = sender.send(raw);
                if raw == 0 {
                    return;
                }
                unsafe {
                    let _ = SendMessageW(hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
                }
                pump_until_closed(hwnd);
            })
            .context("processing indicator thread could not start")?;
        let hwnd = receiver
            .recv()
            .context("processing indicator stopped before presentation")?;
        if hwnd == 0 {
            let _ = thread.join();
            bail!("processing indicator window could not be created");
        }
        Ok(Self {
            hwnd,
            thread: Some(thread),
        })
    }

    pub fn close(mut self) {
        self.request_close();
    }

    fn request_close(&mut self) {
        if self.hwnd != 0 {
            let hwnd = HWND(self.hwnd as *mut std::ffi::c_void);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            self.hwnd = 0;
        }
        self.thread.take();
    }
}

impl Drop for ProcessingIndicator {
    fn drop(&mut self) {
        self.request_close();
    }
}

fn pump_until_closed(hwnd: HWND) {
    let mut message = MSG::default();
    while unsafe { IsWindow(Some(hwnd)).as_bool() }
        && unsafe { GetMessageW(&mut message, None, 0, 0).into() }
    {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
