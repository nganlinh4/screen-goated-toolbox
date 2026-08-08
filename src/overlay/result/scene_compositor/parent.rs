use super::CHILD_FLAG;
use super::card_bridge::with_card_bridge;
use super::diagnostics::{
    CardDiagnosticLog, log_card_diagnostic, log_fit_diagnostic, log_host_command,
};
use super::protocol::{
    ChildEvent, HostCommand, SceneCard, SceneFinalize, SceneGeometry, SceneRect, SceneStream,
};
use crate::overlay::result::markdown_view::conversion::markdown_to_html_for_compositor;
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

static SCENES: LazyLock<Mutex<HashMap<isize, SceneCard>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SCENE_DISPATCH: Mutex<()> = Mutex::new(());
static PENDING_GEOMETRY: LazyLock<Mutex<HashMap<isize, SceneGeometry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static GEOMETRY_SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::spawn(move || {
        while receiver.recv().is_ok() {
            std::thread::sleep(Duration::from_millis(8));
            while receiver.try_recv().is_ok() {}
            let cards = {
                let mut pending = PENDING_GEOMETRY.lock().unwrap();
                pending.drain().map(|(_, geometry)| geometry).collect()
            };
            send_command(HostCommand::Geometry { cards });
        }
    });
    sender
});
static PROCESS: LazyLock<Mutex<Option<RendererProcess>>> = LazyLock::new(|| Mutex::new(None));
static STARTING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static LIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);
static WATCHDOG: Once = Once::new();

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
        )
    };

    let Some(geometry) = read_geometry(hwnd, requested_visible) else {
        return;
    };
    let rendered =
        markdown_to_html_for_compositor(&snapshot.0, snapshot.1, &snapshot.2, &snapshot.3);
    let stream_body = document_body(&rendered);
    let html = with_card_bridge(with_fit(rendered, snapshot.6));
    let card = SceneCard {
        id: hwnd_key,
        rect: geometry.rect,
        html,
        background: format!("#{:06x}", snapshot.4 & 0x00ff_ffff),
        opacity: snapshot.5,
        visible: geometry.visible,
        streaming: snapshot.6,
    };

    let previous = SCENES.lock().unwrap().insert(hwnd_key, card.clone());
    let Some(command) = command_for_transition(previous.as_ref(), &card, stream_body) else {
        return;
    };
    start_watchdog();
    log_host_command(&command, snapshot.0.chars().count());
    send_command(command);
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
                    background: card.background.clone(),
                    opacity: card.opacity,
                    visible: card.visible,
                },
            },
            (Some(true), false) => HostCommand::Finalize {
                card: SceneFinalize {
                    id: card.id,
                    body: stream_body,
                    html: card.html.clone(),
                    background: card.background.clone(),
                    opacity: card.opacity,
                    visible: card.visible,
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

fn with_fit(mut html: String, streaming: bool) -> String {
    let script = crate::overlay::result::markdown_view::fit::runtime_fit_script();
    let injected = format!(
        "<script>window.__SGT_STREAMING__={streaming};window.__SGT_RUN_FIT__=function(streaming){{{script}}};</script>"
    );
    if let Some(position) = html.to_ascii_lowercase().rfind("</body>") {
        html.insert_str(position, &injected);
    } else {
        html.push_str(&injected);
    }
    html
}

fn document_body(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let Some(body_start) = lower.find("<body") else {
        return html.to_string();
    };
    let Some(tag_end) = lower[body_start..].find('>') else {
        return html.to_string();
    };
    let content_start = body_start + tag_end + 1;
    let Some(content_end) = lower[content_start..].rfind("</body>") else {
        return html[content_start..].to_string();
    };
    html[content_start..content_start + content_end].to_string()
}

fn send_command(command: HostCommand) {
    ensure_process();
    if write_command(&command).is_err() {
        restart_process();
        let _ = write_command(&HostCommand::Snapshot {
            cards: scene_snapshot(),
        });
    }
}

fn write_command(command: &HostCommand) -> anyhow::Result<()> {
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

fn ensure_process() {
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
            } => log_card_diagnostic(CardDiagnosticLog {
                id,
                phase,
                revision,
                visible,
                ready,
                payload_len,
                text_len,
                opacity,
                error,
            }),
            ChildEvent::CommandError { command, id, error } => crate::log_info!(
                "[ResultCompositor] command_failed command={command} id={id:?} error={error}"
            ),
            ChildEvent::Navigation {
                id,
                depth,
                max_depth,
            } => update_navigation_state(id, depth, max_depth),
            ChildEvent::FitDiagnostic { id, payload } => log_fit_diagnostic(id, &payload),
        }
    }
    if LIVE_GENERATION
        .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
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
        crate::overlay::result::button_canvas::update_window_position(hwnd);
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
                    restart_process();
                }
            }
        });
    });
}

fn restart_process() {
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
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
mod tests {
    use super::*;

    fn test_card(streaming: bool) -> SceneCard {
        SceneCard {
            id: 42,
            rect: SceneRect {
                x: 1,
                y: 2,
                width: 300,
                height: 100,
            },
            html: "<html><body>result</body></html>".to_string(),
            background: "#ffffff".to_string(),
            opacity: 90,
            visible: true,
            streaming,
        }
    }

    #[test]
    fn heartbeat_timeout_uses_saturating_elapsed_time() {
        assert!(!heartbeat_is_stale(5_000, 0));
        assert!(heartbeat_is_stale(5_001, 0));
        assert!(!heartbeat_is_stale(1, 2));
    }

    #[test]
    fn identical_completed_sync_is_not_dispatched_again() {
        let card = test_card(false);
        assert_eq!(
            command_for_transition(Some(&card), &card, "result".to_string()),
            None
        );
    }

    #[test]
    fn streaming_to_completed_transition_is_always_finalize() {
        let streaming = test_card(true);
        let completed = test_card(false);

        assert!(matches!(
            command_for_transition(Some(&streaming), &completed, "result".to_string()),
            Some(HostCommand::Finalize { .. })
        ));
    }

    #[test]
    fn final_fit_remains_callable_after_iframe_resize() {
        let fitted = with_fit("<html><body>result</body></html>".to_string(), false);
        let bridged = with_card_bridge(fitted);

        assert!(bridged.contains("window.__SGT_RUN_FIT__=function(streaming)"));
        assert!(bridged.contains("window.addEventListener('resize'"));
        assert!(bridged.contains("requestFit(window.__SGT_STREAMING__)"));
        assert!(bridged.contains("reportCardState('bridge_ready', null)"));
        assert!(bridged.contains("reportCardState('script_error'"));
    }

    #[test]
    fn card_content_stays_hidden_until_the_bundled_font_is_loaded() {
        let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

        assert!(bridged.contains("if (!fontReady)"));
        assert!(bridged.contains("document.fonts.load"));
        assert!(bridged.contains("classList.add('sgt-font-ready')"));
        assert!(bridged.contains("reportCardState('font_failed'"));
    }

    #[test]
    fn card_document_waits_for_activation_before_its_first_fit() {
        let fitted = with_fit("<html><body>result</body></html>".to_string(), true);

        assert!(fitted.contains("window.__SGT_RUN_FIT__=function(streaming)"));
        assert!(!fitted.contains("window.__SGT_RUN_FIT__(window.__SGT_STREAMING__)"));
    }

    #[test]
    fn streaming_cards_use_the_full_fitter() {
        let fitted = with_fit("<html><body>result</body></html>".to_string(), true);
        assert!(fitted.contains("fit_font_to_window_runtime"));
        assert!(fitted.contains("const isStreamingFit = Boolean(streaming)"));
        assert!(fitted.contains("window.__SGT_STREAMING__=true"));
    }

    #[test]
    fn finalization_reuses_the_loaded_document() {
        let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

        assert!(bridged.contains("event.data.type === 'finalize'"));
        assert!(bridged.contains("window.__SGT_APPLY_STREAM_UPDATE__"));
        assert!(bridged.contains("animateNewWords: false"));
        assert!(bridged.contains("window.__SGT_INIT_STREAM_GRIDS__()"));
        assert!(bridged.contains("requestFit(false)"));
    }

    #[test]
    fn streaming_keeps_the_legacy_word_reveal_contract() {
        let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

        assert!(bridged.contains("lastRevealedIndex"));
        assert!(bridged.contains("targetWordsPerSecond = 40"));
        assert!(bridged.contains("runInlineSizing: true"));
    }

    #[test]
    fn auto_fitted_streaming_content_remains_top_anchored() {
        let bridged = with_card_bridge("<html><body>result</body></html>".to_string());

        assert!(bridged.contains("window.scrollTo({"));
        assert!(bridged.contains("top: 0"));
        assert!(!bridged.contains("smoothScroll"));
    }

    #[test]
    fn stream_updates_extract_only_body_markup() {
        assert_eq!(
            document_body("<html><body class='result'><p>Hello</p></body></html>"),
            "<p>Hello</p>"
        );
    }
}
