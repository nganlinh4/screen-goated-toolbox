use super::CHILD_FLAG;
use super::card_bridge::with_card_bridge;
use super::card_document::with_fit;
use super::delivery::{request_restart, send_command};
use super::diagnostics::{
    CardDiagnosticLog, log_card_diagnostic, log_fit_diagnostic, log_host_command,
};
use super::protocol::{
    ChildEvent, HostCommand, SceneAppearance, SceneCard, SceneFinalize, SceneGeometry, SceneRect,
    SceneStream, SceneTheme,
};
use crate::overlay::result::markdown_view::conversion::render_for_compositor;
use crate::overlay::result::state::WINDOW_STATES;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindow, IsWindowVisible};

const HEARTBEAT_TIMEOUT_MS: u64 = 5_000;

struct RendererProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    generation: u64,
}

pub(super) static SCENES: LazyLock<Mutex<HashMap<isize, SceneCard>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SCENE_DISPATCH: Mutex<()> = Mutex::new(());
static PENDING_GEOMETRY: LazyLock<Mutex<HashMap<isize, SceneGeometry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static GEOMETRY_SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::spawn(move || {
        while receiver.recv().is_ok() {
            while receiver.try_recv().is_ok() {}
            let cards: Vec<SceneGeometry> = {
                let mut pending = PENDING_GEOMETRY.lock().unwrap();
                pending.drain().map(|(_, geometry)| geometry).collect()
            };
            if !cards.is_empty() {
                send_command(HostCommand::Geometry { cards });
            }
        }
    });
    sender
});
static PROCESS: LazyLock<Mutex<Option<RendererProcess>>> = LazyLock::new(|| Mutex::new(None));
static STARTING: AtomicBool = AtomicBool::new(false);
pub(super) static DRAGGING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static LIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static READY_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);
static NEXT_STACK_ORDER: AtomicU64 = AtomicU64::new(1);
static WATCHDOG: Once = Once::new();
pub fn warmup() {
    super::delivery::warmup();
}

pub(crate) fn wait_until_ready(timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let live = LIVE_GENERATION.load(Ordering::SeqCst);
        if live != 0 && READY_GENERATION.load(Ordering::SeqCst) == live {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

pub fn register_window(hwnd: HWND) {
    sync_window(hwnd, false);
}

pub fn sync_window(hwnd: HWND, requested_visible: bool) {
    let _dispatch = SCENE_DISPATCH.lock().unwrap();
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        remove_window_locked(hwnd);
        return;
    }

    let hwnd_key = hwnd.0 as isize;
    let snapshot = {
        let states = WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get(&hwnd_key) else {
            return;
        };
        (
            state.full_text.clone(),
            state.is_refining,
            state.preset_prompt.clone(),
            state.input_text.clone(),
            state.bg_color,
            state.opacity_percent,
            state.is_streaming_active,
            state.streaming_enabled,
        )
    };

    let rendered = render_for_compositor(&snapshot.0, snapshot.1, &snapshot.2, &snapshot.3);
    let body = rendered.body;
    let document = rendered
        .isolated_document
        .map(|document| with_card_bridge(with_fit(document)));
    let Some(geometry) = read_geometry(hwnd, requested_visible) else {
        return;
    };
    let controls = super::controls::snapshot(hwnd_key).unwrap_or_default();
    let mut scenes = SCENES.lock().unwrap();
    let stack_order = scenes
        .get(&hwnd_key)
        .map(|card| card.stack_order)
        .unwrap_or_else(|| NEXT_STACK_ORDER.fetch_add(1, Ordering::SeqCst));
    let card = SceneCard {
        id: hwnd_key,
        rect: geometry.rect,
        control_rect: geometry.control_rect,
        body: body.clone(),
        document,
        refining: snapshot.1,
        background: format!("#{:06x}", snapshot.4 & 0x00ff_ffff),
        opacity: snapshot.5,
        visible: geometry.visible,
        streaming: snapshot.6,
        streaming_enabled: snapshot.7,
        stack_order,
        controls,
    };

    let previous = scenes.insert(hwnd_key, card.clone());
    drop(scenes);
    let Some(command) = command_for_transition(previous.as_ref(), &card, body) else {
        return;
    };
    start_watchdog();
    log_host_command(&command, snapshot.0.chars().count());
    send_command(command);
    if !snapshot.0.is_empty() {
        crate::overlay::result::latency::mark_window(hwnd, "compositor_command_queued");
    }
}

fn command_for_transition(
    previous: Option<&SceneCard>,
    card: &SceneCard,
    stream_body: String,
) -> Option<HostCommand> {
    if previous == Some(card) {
        return None;
    }
    Some(
        match (previous.map(|scene| scene.streaming), card.streaming) {
            (Some(true), true) => HostCommand::Stream {
                card: SceneStream {
                    id: card.id,
                    body: stream_body.clone(),
                    document: card.document.clone(),
                    refining: card.refining,
                    background: card.background.clone(),
                    opacity: card.opacity,
                    visible: card.visible,
                    streaming_enabled: card.streaming_enabled,
                    controls: card.controls.clone(),
                },
            },
            (Some(true), false) => HostCommand::Finalize {
                card: SceneFinalize {
                    id: card.id,
                    body: stream_body,
                    document: card.document.clone(),
                    refining: card.refining,
                    background: card.background.clone(),
                    opacity: card.opacity,
                    visible: card.visible,
                    streaming_enabled: card.streaming_enabled,
                    controls: card.controls.clone(),
                },
            },
            _ => HostCommand::Upsert { card: card.clone() },
        },
    )
}

pub fn sync_geometry(hwnd: HWND, requested_visible: bool) {
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        remove_window(hwnd);
        return;
    }
    let Some(geometry) = read_geometry(hwnd, requested_visible) else {
        return;
    };
    let updated = {
        let mut scenes = SCENES.lock().unwrap();
        let Some(card) = scenes.get_mut(&geometry.id) else {
            return;
        };
        card.rect = geometry.rect.clone();
        card.control_rect = geometry.control_rect.clone();
        card.visible = geometry.visible;
        true
    };
    if updated {
        PENDING_GEOMETRY
            .lock()
            .unwrap()
            .insert(geometry.id, geometry);
        let _ = GEOMETRY_SIGNAL.try_send(());
    }
}

fn read_geometry(hwnd: HWND, requested_visible: bool) -> Option<SceneGeometry> {
    let mut screen_rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut screen_rect) }.ok()?;
    let virtual_x = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
        )
    };
    let virtual_y = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
        )
    };
    Some(SceneGeometry {
        id: hwnd.0 as isize,
        rect: SceneRect {
            x: screen_rect.left - virtual_x + 4,
            y: screen_rect.top - virtual_y + 2,
            width: (screen_rect.right - screen_rect.left - 8).max(1),
            height: (screen_rect.bottom - screen_rect.top - 4).max(1),
        },
        control_rect: SceneRect {
            x: screen_rect.left - virtual_x,
            y: screen_rect.top - virtual_y,
            width: (screen_rect.right - screen_rect.left).max(1),
            height: (screen_rect.bottom - screen_rect.top).max(1),
        },
        visible: requested_visible && unsafe { IsWindowVisible(hwnd).as_bool() },
    })
}

pub fn remove_window(hwnd: HWND) {
    let _dispatch = SCENE_DISPATCH.lock().unwrap();
    remove_window_locked(hwnd);
}

fn remove_window_locked(hwnd: HWND) {
    let id = hwnd.0 as isize;
    PENDING_GEOMETRY.lock().unwrap().remove(&id);
    if SCENES.lock().unwrap().remove(&id).is_some() {
        crate::log_info!("[ResultCard] id={id} host=remove");
        send_command(HostCommand::Remove { id });
    }
}

pub fn go_back(hwnd: HWND) {
    send_command(HostCommand::NavigateBack {
        id: hwnd.0 as isize,
    });
}

pub fn go_forward(hwnd: HWND) {
    send_command(HostCommand::NavigateForward {
        id: hwnd.0 as isize,
    });
}

pub fn raise_window(hwnd: HWND) {
    raise_window_id(hwnd.0 as isize);
}

fn raise_window_id(id: isize) {
    let stack_order = NEXT_STACK_ORDER.fetch_add(1, Ordering::SeqCst);
    let updated = SCENES.lock().unwrap().get_mut(&id).is_some_and(|card| {
        card.stack_order = stack_order;
        true
    });
    if updated {
        send_command(HostCommand::Raise { id, stack_order });
    }
}

pub fn update_theme(is_dark: bool) {
    let _dispatch = SCENE_DISPATCH.lock().unwrap();
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        for state in states.values_mut() {
            state.bg_color = crate::overlay::result::window::remap_chain_color_for_theme(
                state.bg_color,
                is_dark,
            );
        }
    }
    let theme = theme_command(is_dark);
    send_command(HostCommand::Theme { theme });
}

fn theme_command(is_dark: bool) -> SceneTheme {
    let backgrounds: HashMap<isize, String> = WINDOW_STATES
        .lock()
        .unwrap()
        .iter()
        .map(|(id, state)| (*id, format!("#{:06x}", state.bg_color & 0x00ff_ffff)))
        .collect();
    let mut scenes = SCENES.lock().unwrap();
    let cards = scenes
        .values_mut()
        .map(|card| {
            if let Some(background) = backgrounds.get(&card.id) {
                card.background.clone_from(background);
            }
            SceneAppearance {
                id: card.id,
                background: card.background.clone(),
            }
        })
        .collect();
    SceneTheme {
        css: crate::overlay::result::markdown_view::css::get_theme_css(is_dark),
        controls_css: crate::overlay::result::button_canvas::theme_css(is_dark),
        cards,
    }
}

pub(super) fn write_command(command: &HostCommand) -> anyhow::Result<()> {
    let mut process = PROCESS.lock().unwrap();
    let renderer = process
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("result compositor is unavailable"))?;
    serde_json::to_writer(&mut renderer.stdin, command)?;
    renderer.stdin.write_all(b"\n")?;
    renderer.stdin.flush()?;
    Ok(())
}

fn scene_snapshot() -> Vec<SceneCard> {
    SCENES.lock().unwrap().values().cloned().collect()
}

pub(super) fn ensure_process() {
    if PROCESS.lock().unwrap().is_some() || STARTING.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Err(error) = spawn_process() {
        crate::log_info!("[ResultCompositor] failed to start renderer: {error:#}");
    }
    STARTING.store(false, Ordering::SeqCst);
}

fn spawn_process() -> anyhow::Result<()> {
    let executable = std::env::current_exe()?;
    let mut child = Command::new(executable)
        .arg(CHILD_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("renderer stdin was not created"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("renderer stdout was not created"))?;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    LIVE_GENERATION.store(generation, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst);

    *PROCESS.lock().unwrap() = Some(RendererProcess {
        child,
        stdin: BufWriter::new(stdin),
        generation,
    });

    std::thread::spawn(move || read_events(stdout, generation));
    crate::log_info!("[ResultCompositor] renderer process spawned generation={generation}");
    write_command(&HostCommand::Snapshot {
        cards: scene_snapshot(),
    })?;
    write_command(&HostCommand::Theme {
        theme: theme_command(crate::overlay::is_dark_mode()),
    })?;
    Ok(())
}

fn read_events(stdout: std::process::ChildStdout, generation: u64) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let event = match serde_json::from_str::<ChildEvent>(&line) {
            Ok(event) => event,
            Err(error) => {
                crate::log_info!(
                    "[ResultCompositor] invalid child event generation={generation} bytes={} error={error}",
                    line.len()
                );
                continue;
            }
        };
        if LIVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        match event {
            ChildEvent::Ready => {
                LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst);
                READY_GENERATION.store(generation, Ordering::SeqCst);
                crate::log_info!("[ResultCompositor] renderer ready generation={generation}");
            }
            ChildEvent::Heartbeat => LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst),
            ChildEvent::FontReady { duration_ms } => crate::log_info!(
                "[ResultCompositor] bundled_font_ready generation={generation} duration_ms={duration_ms:.1}"
            ),
            ChildEvent::CardDiagnostic {
                id,
                phase,
                revision,
                visible,
                ready,
                payload_len,
                text_len,
                opacity,
                error,
            } => {
                crate::overlay::result::latency::mark_card_phase(id, &phase, payload_len, text_len);
                log_card_diagnostic(CardDiagnosticLog {
                    id,
                    phase,
                    revision,
                    visible,
                    ready,
                    payload_len,
                    text_len,
                    opacity,
                    error,
                });
            }
            ChildEvent::CommandError { command, id, error } => crate::log_info!(
                "[ResultCompositor] command_failed command={command} id={id:?} error={error}"
            ),
            ChildEvent::Navigation {
                id,
                depth,
                max_depth,
            } => update_navigation_state(id, depth, max_depth),
            ChildEvent::Interaction { id } => {
                raise_window_id(id);
            }
            ChildEvent::ButtonAction { id, action } => {
                crate::overlay::result::button_canvas::handle_action(id, action);
            }
            ChildEvent::DragStarted => DRAGGING.store(true, Ordering::SeqCst),
            ChildEvent::DragFinished {
                id,
                targets,
                outcome,
            } => {
                DRAGGING.store(false, Ordering::SeqCst);
                crate::overlay::result::button_canvas::handle_drag_finished(id, &targets, outcome);
                super::controls::sync_all();
            }
            ChildEvent::FitDiagnostic { id, payload } => log_fit_diagnostic(id, &payload),
        }
    }
    if LIVE_GENERATION
        .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        READY_GENERATION
            .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        DRAGGING.store(false, Ordering::SeqCst);
        crate::log_info!("[ResultCompositor] renderer process disconnected");
    }
}

fn update_navigation_state(id: isize, depth: usize, max_depth: usize) {
    let updated = {
        let mut states = WINDOW_STATES.lock().unwrap();
        states.get_mut(&id).is_some_and(|state| {
            state.navigation_depth = depth;
            state.max_navigation_depth = max_depth;
            state.is_browsing = depth > 0;
            if state.is_browsing {
                state.is_editing = false;
            }
            true
        })
    };
    if updated {
        let hwnd = HWND(id as *mut std::ffi::c_void);
        super::controls::sync(hwnd);
    }
}

fn start_watchdog() {
    WATCHDOG.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if SCENES.lock().unwrap().is_empty() {
                    continue;
                }
                let live = LIVE_GENERATION.load(Ordering::SeqCst) != 0;
                let stale = heartbeat_is_stale(now_ms(), LAST_HEARTBEAT_MS.load(Ordering::SeqCst));
                if !live || stale {
                    if stale {
                        crate::log_info!(
                            "[ResultCompositor] heartbeat timed out; restarting renderer"
                        );
                    }
                    request_restart();
                }
            }
        });
    });
}

pub(super) fn restart_process_now() {
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        READY_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        let _ = renderer.child.kill();
        let _ = renderer.child.wait();
    }
    drop(old);
    ensure_process();
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn heartbeat_is_stale(now: u64, last: u64) -> bool {
    now.saturating_sub(last) > HEARTBEAT_TIMEOUT_MS
}

#[cfg(test)]
#[path = "parent_tests.rs"]
mod tests;
