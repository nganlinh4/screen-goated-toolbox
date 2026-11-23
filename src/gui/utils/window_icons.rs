//! Per-window ownership for app-created window icons.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIcon, DestroyIcon, GetSystemMetrics, GetWindowThreadProcessId, HICON, ICON_BIG,
    ICON_SMALL, IsWindow, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SendMessageW,
    WM_NCDESTROY, WM_SETICON,
};

const SUBCLASS_ID: usize = 0x5347_5449;
const MAX_CACHED_ICONS: usize = 8;
const MAX_ICON_SIDE: i32 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IconKey {
    dark: bool,
    width: i32,
    height: i32,
}

struct Cache<T> {
    entries: Vec<(IconKey, T)>,
}
impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
impl<T> Cache<T> {
    fn get(&self, key: IconKey) -> Option<&T> {
        self.entries
            .iter()
            .find(|(existing, _)| *existing == key)
            .map(|(_, value)| value)
    }
    fn insert(&mut self, key: IconKey, value: T) -> Result<(), T> {
        if self.entries.len() >= MAX_CACHED_ICONS || self.get(key).is_some() {
            return Err(value);
        }
        self.entries.push((key, value));
        Ok(())
    }
}

struct OwnedIcon(HICON);
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

#[derive(Default)]
struct WindowIcons {
    alive: Cell<bool>,
    cache: RefCell<Cache<OwnedIcon>>,
}

impl Drop for WindowIcons {
    fn drop(&mut self) {
        if self.alive.get() {
            // Thread-local teardown can precede the OS destroying its windows.
            // Retain uncertain handles until process exit rather than freeing an
            // icon that a still-live window may be using.
            for (_, icon) in self.cache.get_mut().entries.drain(..) {
                std::mem::forget(icon);
            }
        }
    }
}

thread_local! {
    static WINDOWS: RefCell<HashMap<isize, Rc<WindowIcons>>> = RefCell::new(HashMap::new());
}

fn begin_retirement(
    windows: &HashMap<isize, Rc<WindowIcons>>,
    key: isize,
) -> Option<Rc<WindowIcons>> {
    let state = windows.get(&key)?.clone();
    state.alive.set(false);
    Some(state)
}

fn finish_retirement(
    windows: &mut HashMap<isize, Rc<WindowIcons>>,
    key: isize,
    retiring: &Rc<WindowIcons>,
) {
    if windows
        .get(&key)
        .is_some_and(|current| Rc::ptr_eq(current, retiring))
    {
        windows.remove(&key);
    }
}

/// CreateIcon's monochrome bitmap scanlines are padded to WORD boundaries.
fn mask_len(width: i32, height: i32) -> Option<usize> {
    if !(1..=MAX_ICON_SIDE).contains(&width) || !(1..=MAX_ICON_SIDE).contains(&height) {
        return None;
    }
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    width
        .checked_add(15)?
        .checked_div(16)?
        .checked_mul(2)?
        .checked_mul(height)
}

fn create(key: IconKey) -> Option<OwnedIcon> {
    let mask_len = mask_len(key.width, key.height)?;
    let bytes: &[u8] = if key.dark {
        include_bytes!("../../../assets/app-icon-small.png")
    } else {
        include_bytes!("../../../assets/app-icon-small-light.png")
    };
    let rgba = image::load_from_memory(bytes)
        .ok()?
        .resize_exact(
            key.width as u32,
            key.height as u32,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    let mut bgra = rgba.into_raw();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let mask = vec![0_u8; mask_len];
    let icon = unsafe {
        CreateIcon(
            None,
            key.width,
            key.height,
            1,
            32,
            mask.as_ptr(),
            bgra.as_ptr(),
        )
        .ok()?
    };
    (!icon.is_invalid()).then_some(OwnedIcon(icon))
}

fn window_state(hwnd: HWND) -> Option<Rc<WindowIcons>> {
    if hwnd.is_invalid()
        || !unsafe { IsWindow(Some(hwnd)).as_bool() }
        || unsafe { GetWindowThreadProcessId(hwnd, None) } != unsafe { GetCurrentThreadId() }
    {
        return None;
    }
    let key = hwnd.0 as isize;
    if let Some(existing) = WINDOWS.with_borrow(|windows| windows.get(&key).cloned()) {
        return existing.alive.get().then_some(existing);
    }
    let state = Rc::new(WindowIcons {
        alive: Cell::new(true),
        cache: RefCell::new(Cache::default()),
    });
    if !unsafe { SetWindowSubclass(hwnd, Some(icon_subclass), SUBCLASS_ID, 0).as_bool() } {
        return None;
    }
    WINDOWS.with_borrow_mut(|windows| {
        windows.insert(key, Rc::clone(&state));
    });
    Some(state)
}

fn icon(state: &WindowIcons, key: IconKey) -> Option<HICON> {
    if !state.alive.get() {
        return None;
    }
    if let Some(icon) = state.cache.borrow().get(key) {
        return Some(icon.0);
    }
    let created = create(key)?;
    if !state.alive.get() {
        return None;
    }
    let mut cache = state.cache.borrow_mut();
    if let Some(existing) = cache.get(key) {
        return Some(existing.0);
    }
    let handle = created.0;
    cache.insert(key, created).ok()?;
    Some(handle)
}

pub(super) fn set(hwnd: HWND, dark: bool) {
    let Some(state) = window_state(hwnd) else {
        return;
    };
    for (kind, width_metric, height_metric) in [
        (ICON_SMALL, SM_CXSMICON, SM_CYSMICON),
        (ICON_BIG, SM_CXICON, SM_CYICON),
    ] {
        if !state.alive.get() {
            return;
        }
        let key = IconKey {
            dark,
            width: unsafe { GetSystemMetrics(width_metric) },
            height: unsafe { GetSystemMetrics(height_metric) },
        };
        let Some(handle) = icon(&state, key) else {
            continue;
        };
        // Retain all our handles until this exact window is destroyed; the
        // previously returned handle may be borrowed or externally owned.
        if state.alive.get() {
            unsafe {
                let _ = SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(handle.0 as isize)),
                );
            }
        }
    }
}

unsafe extern "system" fn icon_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message != WM_NCDESTROY {
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    let key = hwnd.0 as isize;
    let owned = WINDOWS.with_borrow(|windows| begin_retirement(windows, key));
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    unsafe {
        let _ = RemoveWindowSubclass(hwnd, Some(icon_subclass), SUBCLASS_ID);
    }
    if let Some(state) = &owned {
        WINDOWS.with_borrow_mut(|windows| finish_retirement(windows, key, state));
    }
    drop(owned);
    result
}

#[cfg(test)]
#[path = "window_icons_tests.rs"]
mod tests;
