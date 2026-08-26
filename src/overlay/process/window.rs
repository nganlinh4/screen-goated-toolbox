use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, KillTimer, RegisterClassW, SetTimer, WM_CLOSE,
    WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

use super::surface::SurfaceSet;

const APPEAR_DURATION: Duration = Duration::from_millis(160);
const APPEAR_INTERVAL_MS: u32 = 16;

static REGISTER_CONTROLLER_CLASS: Once = Once::new();
static STATES: LazyLock<Mutex<HashMap<isize, ProcessingState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct ProcessingState {
    surfaces: SurfaceSet,
    started_at: Instant,
    fading: bool,
    appearing: bool,
    alpha: u8,
    scheduler: FrameScheduler,
}

unsafe impl Send for ProcessingState {}

impl ProcessingState {
    fn new(surfaces: SurfaceSet) -> Self {
        let scheduler = FrameScheduler::new(surfaces.pixel_count());
        Self {
            surfaces,
            started_at: Instant::now(),
            fading: false,
            appearing: true,
            alpha: 0,
            scheduler,
        }
    }
}

fn appearance_alpha(elapsed: Duration) -> u8 {
    let progress = (elapsed.as_secs_f32() / APPEAR_DURATION.as_secs_f32()).clamp(0.0, 1.0);
    let eased = progress * progress * (3.0 - 2.0 * progress);
    (eased * 255.0).round() as u8
}

#[derive(Clone, Debug)]
struct FrameScheduler {
    baseline_level: usize,
    level: usize,
    overload_count: u8,
    relaxed_count: u16,
}

impl FrameScheduler {
    const INTERVALS_MS: [u32; 4] = [16, 33, 50, 67];

    fn new(pixel_count: usize) -> Self {
        let baseline_level = match pixel_count {
            0..=120_000 => 0,
            120_001..=350_000 => 1,
            350_001..=750_000 => 2,
            _ => 3,
        };
        Self {
            baseline_level,
            level: baseline_level,
            overload_count: 0,
            relaxed_count: 0,
        }
    }

    fn interval_ms(&self) -> u32 {
        Self::INTERVALS_MS[self.level]
    }

    fn observe(&mut self, render_time: Duration) -> Option<u32> {
        let previous = self.interval_ms();
        let render_micros = render_time.as_micros();
        let interval_micros = u128::from(previous) * 1000;
        if render_micros * 4 >= interval_micros * 3 {
            self.overload_count = self.overload_count.saturating_add(1);
            self.relaxed_count = 0;
            if self.overload_count >= 2 && self.level + 1 < Self::INTERVALS_MS.len() {
                self.level += 1;
                self.overload_count = 0;
            }
        } else if render_micros * 5 <= interval_micros {
            self.overload_count = 0;
            self.relaxed_count = self.relaxed_count.saturating_add(1);
            if self.relaxed_count >= 90 && self.level > self.baseline_level {
                self.level -= 1;
                self.relaxed_count = 0;
            }
        } else {
            self.overload_count = 0;
            self.relaxed_count = 0;
        }
        (self.interval_ms() != previous).then(|| self.interval_ms())
    }
}

pub unsafe fn create_processing_window(rect: RECT) -> HWND {
    unsafe {
        if rect.left >= rect.right || rect.top >= rect.bottom {
            return HWND::default();
        }
        let module = match GetModuleHandleW(None) {
            Ok(module) => module,
            Err(_) => return HWND::default(),
        };
        REGISTER_CONTROLLER_CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(controller_wnd_proc),
                hInstance: module.into(),
                lpszClassName: w!("SGTProcessingOverlayController"),
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
        });
        let controller = match CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("SGTProcessingOverlayController"),
            w!(""),
            WS_POPUP,
            rect.left,
            rect.top,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(_) => return HWND::default(),
        };
        let Some(surfaces) = SurfaceSet::create(controller, module.into(), rect) else {
            let _ = DestroyWindow(controller);
            return HWND::default();
        };
        let state = ProcessingState::new(surfaces);
        STATES.lock().unwrap().insert(controller.0 as isize, state);
        SetTimer(Some(controller), 1, APPEAR_INTERVAL_MS, None);
        controller
    }
}

unsafe extern "system" fn controller_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_CLOSE => {
                begin_fade(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                render_timer(hwnd, wparam.0);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_DESTROY => {
                STATES.lock().unwrap().remove(&(hwnd.0 as isize));
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

unsafe fn begin_fade(hwnd: HWND) {
    let mut states = STATES.lock().unwrap();
    let Some(state) = states.get_mut(&(hwnd.0 as isize)) else {
        drop(states);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    };
    if !state.fading {
        state.fading = true;
        unsafe {
            let _ = KillTimer(Some(hwnd), 1);
            SetTimer(Some(hwnd), 2, 25, None);
        }
    }
}

unsafe fn render_timer(hwnd: HWND, timer_id: usize) {
    let mut destroy = false;
    let mut next_interval = None;
    {
        let mut states = STATES.lock().unwrap();
        let Some(state) = states.get_mut(&(hwnd.0 as isize)) else {
            return;
        };
        if timer_id == 2 || state.fading {
            if state.alpha > 20 {
                state.alpha -= 20;
                unsafe {
                    state
                        .surfaces
                        .present(state.started_at.elapsed(), state.alpha, false);
                }
            } else {
                state.alpha = 0;
                destroy = true;
            }
        } else {
            let started = Instant::now();
            let elapsed = state.started_at.elapsed();
            if state.appearing {
                state.alpha = appearance_alpha(elapsed);
                if state.alpha == 255 {
                    state.appearing = false;
                    next_interval = Some(state.scheduler.interval_ms());
                }
            }
            unsafe {
                state.surfaces.present(elapsed, state.alpha, true);
            }
            if !state.appearing && next_interval.is_none() {
                next_interval = state.scheduler.observe(started.elapsed());
            }
        }
    }
    if let Some(interval) = next_interval {
        unsafe {
            let _ = KillTimer(Some(hwnd), 1);
            SetTimer(Some(hwnd), 1, interval, None);
        }
    }
    if destroy {
        unsafe {
            let _ = KillTimer(Some(hwnd), 1);
            let _ = KillTimer(Some(hwnd), 2);
            let _ = DestroyWindow(hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        APPEAR_DURATION, FrameScheduler, STATES, appearance_alpha, create_processing_window,
    };
    use std::time::Duration;
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, IsWindow, SendMessageW, WM_TIMER,
    };

    #[test]
    fn workload_selects_a_bounded_visual_frame_rate() {
        assert_eq!(FrameScheduler::new(100_000).interval_ms(), 16);
        assert_eq!(FrameScheduler::new(200_000).interval_ms(), 33);
        assert_eq!(FrameScheduler::new(500_000).interval_ms(), 50);
        assert_eq!(FrameScheduler::new(900_000).interval_ms(), 67);
    }

    #[test]
    fn appearance_is_eased_monotonic_and_reaches_full_alpha() {
        assert_eq!(appearance_alpha(Duration::ZERO), 0);
        let quarter = appearance_alpha(APPEAR_DURATION / 4);
        let middle = appearance_alpha(APPEAR_DURATION / 2);
        let three_quarters = appearance_alpha(APPEAR_DURATION * 3 / 4);
        assert!(quarter > 0 && quarter < middle);
        assert!(middle < three_quarters && three_quarters < 255);
        assert_eq!(appearance_alpha(APPEAR_DURATION), 255);
    }

    #[test]
    fn sustained_overload_degrades_and_sustained_headroom_recovers() {
        let mut scheduler = FrameScheduler::new(100_000);
        assert_eq!(scheduler.observe(Duration::from_millis(15)), None);
        assert_eq!(scheduler.observe(Duration::from_millis(15)), Some(33));
        for _ in 0..89 {
            assert_eq!(scheduler.observe(Duration::from_millis(1)), None);
        }
        assert_eq!(scheduler.observe(Duration::from_millis(1)), Some(16));
    }

    #[test]
    fn real_offscreen_edge_surfaces_present_and_cleanup() {
        for (index, (width, height)) in [(1, 1), (4000, 24), (24, 4000), (640, 360)]
            .into_iter()
            .enumerate()
        {
            let origin = 100_000 + index as i32 * 5_000;
            let hwnd = unsafe {
                create_processing_window(RECT {
                    left: origin,
                    top: 100_000,
                    right: origin + width,
                    bottom: 100_000 + height,
                })
            };
            assert!(unsafe { IsWindow(Some(hwnd)).as_bool() });
            assert!(
                STATES
                    .lock()
                    .unwrap()
                    .get(&(hwnd.0 as isize))
                    .is_some_and(|state| state.surfaces.pixel_count() > 0)
            );
            unsafe {
                let _ = SendMessageW(hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
                let _ = DestroyWindow(hwnd);
            }
            assert!(!unsafe { IsWindow(Some(hwnd)).as_bool() });
            assert!(!STATES.lock().unwrap().contains_key(&(hwnd.0 as isize)));
        }
    }
}
