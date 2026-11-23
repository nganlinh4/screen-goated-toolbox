//! Drives the whole dictation stream with a scripted session and a virtual
//! clock: mode cycling, send failures, server errors, rotation and bounded
//! reconnects all run without a socket, credentials or real waiting.

use super::*;
use crate::api::gemini_live::ready_session::LiveServerError;
use crate::api::gemini_live::server_frame::{LiveGoAway, LiveServerFrame, LiveSessionResumption};
use crate::api::gemini_live::setup_failure::LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS;
use crate::model_config::GEMINI_LIVE_API_MODEL_2_5;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

const DEDICATED_MODEL: &str = "gemini-3.5-transcribe-live";
const SAMPLES_PER_MS: usize = 16;

/// Virtual time. Each sleep advances it and, like the capture callback,
/// appends that much numbered (never silent) audio to the capture buffer.
struct FakeClock {
    start: Instant,
    now: Cell<Instant>,
    run_for: Duration,
    capture: bool,
    next_sample: Cell<i16>,
    captured: RefCell<Vec<i16>>,
    audio: Arc<Mutex<Vec<i16>>>,
    stop: Arc<AtomicBool>,
}

impl FakeClock {
    fn elapsed(&self) -> Duration {
        self.now.get() - self.start
    }

    fn capture(&self, samples: usize) {
        let mut chunk = Vec::with_capacity(samples);
        for _ in 0..samples {
            let value = self.next_sample.get();
            chunk.push(value);
            self.next_sample
                .set(if value == i16::MAX { 1 } else { value + 1 });
        }
        self.captured.borrow_mut().extend_from_slice(&chunk);
        self.audio.lock().unwrap().extend(chunk);
    }
}

impl StreamClock for FakeClock {
    fn now(&self) -> Instant {
        self.now.get()
    }

    fn sleep(&self, duration: Duration) {
        self.now.set(self.now.get() + duration);
        if self.capture {
            self.capture(duration.as_millis() as usize * SAMPLES_PER_MS);
        }
        if self.elapsed() >= self.run_for {
            self.stop.store(true, Ordering::SeqCst);
        }
    }
}

/// Everything every scripted session put on, or saw happen to, the wire.
#[derive(Default)]
struct Wire {
    sent: Vec<(usize, Vec<i16>)>,
    closed: Vec<usize>,
    reopen_handles: Vec<Option<String>>,
}

struct FakeSession {
    id: usize,
    wire: Rc<RefCell<Wire>>,
    polls: VecDeque<anyhow::Result<LivePoll>>,
    sends_before_failure: Option<usize>,
}

impl FakeSession {
    fn new(id: usize, wire: &Rc<RefCell<Wire>>) -> Self {
        Self {
            id,
            wire: wire.clone(),
            polls: VecDeque::new(),
            sends_before_failure: None,
        }
    }
}

impl StreamSession for FakeSession {
    fn send_audio(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        if let Some(remaining) = &mut self.sends_before_failure {
            if *remaining == 0 {
                anyhow::bail!("connection reset by peer");
            }
            *remaining -= 1;
        }
        self.wire
            .borrow_mut()
            .sent
            .push((self.id, samples.to_vec()));
        Ok(())
    }

    fn end_audio_stream(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn poll(&mut self) -> anyhow::Result<LivePoll> {
        self.polls.pop_front().unwrap_or(Ok(LivePoll::Idle))
    }

    fn close(&mut self) {
        self.wire.borrow_mut().closed.push(self.id);
    }
}

struct Run {
    result: Option<anyhow::Error>,
    wire: Rc<RefCell<Wire>>,
    captured: Vec<i16>,
    /// Capture left for the caller's tail: unsent replay first, then new audio.
    leftover: Vec<i16>,
    accumulated: String,
    elapsed: Duration,
}

impl Run {
    /// Real audio in the order it reached the wire, across every session.
    fn sent_audio(&self) -> Vec<i16> {
        self.wire
            .borrow()
            .sent
            .iter()
            .flat_map(|(_, samples)| samples.iter().copied())
            .filter(|sample| *sample != 0)
            .collect()
    }

    fn assert_no_audio_lost(&self) {
        let mut delivered = self.sent_audio();
        delivered.extend_from_slice(&self.leftover);
        assert_eq!(
            delivered.len(),
            self.captured.len(),
            "audio was lost or duplicated"
        );
        assert!(delivered == self.captured, "audio was reordered");
    }
}

type OpenFirst = Box<dyn FnOnce(&Rc<RefCell<Wire>>) -> FakeSession>;
type Reopen = Box<dyn FnMut(usize, &Rc<RefCell<Wire>>) -> anyhow::Result<FakeSession>>;

struct Scenario {
    model: &'static str,
    run_for: Duration,
    capture: bool,
    seed_audio: usize,
    first: OpenFirst,
    reopen: Reopen,
}

impl Scenario {
    fn new(model: &'static str, run_for: Duration) -> Self {
        Self {
            model,
            run_for,
            capture: true,
            seed_audio: 0,
            first: Box::new(|wire| FakeSession::new(0, wire)),
            reopen: Box::new(|id, wire| Ok(FakeSession::new(id, wire))),
        }
    }

    fn run(self) -> Run {
        let Scenario {
            model,
            run_for,
            capture,
            seed_audio,
            first,
            mut reopen,
        } = self;
        let wire = Rc::new(RefCell::new(Wire::default()));
        let audio = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        let clock = FakeClock {
            start,
            now: Cell::new(start),
            run_for,
            capture,
            next_sample: Cell::new(1),
            captured: RefCell::new(Vec::new()),
            audio: audio.clone(),
            stop: stop.clone(),
        };
        clock.capture(seed_audio);
        let preset = Preset {
            hide_recording_ui: true,
            auto_paste: false,
            auto_stop_recording: false,
            ..Default::default()
        };
        let accumulated = Arc::new(Mutex::new(String::new()));
        let pause = Arc::new(AtomicBool::new(false));
        let abort = Arc::new(AtomicBool::new(false));
        let auto_paste = crate::overlay::utils::StreamingAutoPaste::new(false, abort.clone());
        let mut transcript = crate::api::gemini_transcribe::TranscriptState::default();
        let mut session = first(&wire);
        let reopen_wire = wire.clone();
        let opened = Cell::new(0);
        let real_start = Instant::now();
        let result = run_streaming_loop(StreamingLoopContext {
            preset: &preset,
            session: &mut session,
            reopen: &mut |handle, _cancelled| {
                reopen_wire
                    .borrow_mut()
                    .reopen_handles
                    .push(handle.map(str::to_owned));
                opened.set(opened.get() + 1);
                reopen(opened.get(), &reopen_wire)
            },
            model,
            audio_buffer: &audio,
            accumulated_text: &accumulated,
            transcribe_text: &mut transcript,
            auto_paste: &auto_paste,
            stop_signal: &stop,
            pause_signal: &pause,
            abort_signal: &abort,
            overlay_hwnd: HWND::default(),
            update_stream_text: &|_: &str| {},
            clock: &clock,
        });
        assert!(
            real_start.elapsed() < Duration::from_secs(5),
            "virtual time leaked into a real wait"
        );
        let leftover = std::mem::take(&mut *audio.lock().unwrap());
        let accumulated = accumulated.lock().unwrap().clone();
        Run {
            result,
            wire,
            captured: clock.captured.take(),
            leftover,
            accumulated,
            elapsed: clock.elapsed(),
        }
    }
}

fn frame(update: impl FnOnce(&mut LiveServerFrame)) -> anyhow::Result<LivePoll> {
    let mut frame = LiveServerFrame::default();
    update(&mut frame);
    Ok(LivePoll::Frame(Box::new(frame)))
}

fn idle_polls(count: usize) -> impl Iterator<Item = anyhow::Result<LivePoll>> {
    (0..count).map(|_| Ok(LivePoll::Idle))
}

#[test]
fn fixtures_cover_both_transcription_protocols() {
    assert!(!crate::api::gemini_transcribe::is_live_transcribe(
        GEMINI_LIVE_API_MODEL_2_5
    ));
    assert!(crate::api::gemini_transcribe::is_live_transcribe(
        DEDICATED_MODEL
    ));
}

#[test]
fn general_live_model_cycles_silence_and_catch_up_without_losing_audio() {
    let run = Scenario::new(GEMINI_LIVE_API_MODEL_2_5, Duration::from_secs(25)).run();

    assert!(run.result.is_none());
    assert!(run.wire.borrow().reopen_handles.is_empty(), "no reconnect");
    run.assert_no_audio_lost();
    let wire = run.wire.borrow();
    let silence_frames = wire
        .sent
        .iter()
        .filter(|(_, samples)| samples.len() == 1_600 && samples.iter().all(|s| *s == 0))
        .count();
    // Twenty seconds of audio, then two seconds of 100 ms silence frames.
    assert!((19..=21).contains(&silence_frames), "{silence_frames}");
    // Catch-up drains the held audio at double rate before normal resumes.
    let catch_up = wire
        .sent
        .iter()
        .filter(|(_, samples)| samples.len() == 3_200 && samples[0] != 0)
        .count();
    assert!(catch_up >= 10, "{catch_up}");
    assert!(run.leftover.len() < 3_200, "{}", run.leftover.len());
}

#[test]
fn unanswered_flush_reconnects_once_and_replays_held_audio_first() {
    let run = Scenario::new(GEMINI_LIVE_API_MODEL_2_5, Duration::from_secs(31)).run();

    assert!(run.result.is_none());
    // The flush at 22 s plus the 8 s response window.
    assert_eq!(run.wire.borrow().reopen_handles, vec![None]);
    assert_eq!(run.wire.borrow().closed, vec![0]);
    run.assert_no_audio_lost();
    let wire = run.wire.borrow();
    let second = wire.sent.iter().position(|(id, _)| *id == 1).unwrap();
    assert!(wire.sent[..second].iter().all(|(id, _)| *id == 0));
}

#[test]
fn send_failure_requeues_unsent_audio_ahead_of_new_capture() {
    let mut scenario = Scenario::new(DEDICATED_MODEL, Duration::from_secs(3));
    scenario.first = Box::new(|wire| {
        let mut session = FakeSession::new(0, wire);
        session.sends_before_failure = Some(5);
        session
    });
    let run = scenario.run();

    assert!(run.result.is_none());
    assert_eq!(run.wire.borrow().reopen_handles, vec![None]);
    assert_eq!(run.wire.borrow().closed, vec![0]);
    assert!(run.wire.borrow().sent.iter().any(|(id, _)| *id == 1));
    run.assert_no_audio_lost();
}

#[test]
fn dedicated_go_away_rotates_with_the_latest_resumption_handle() {
    let mut scenario = Scenario::new(DEDICATED_MODEL, Duration::from_secs(2));
    scenario.first = Box::new(|wire| {
        let mut session = FakeSession::new(0, wire);
        session.polls.extend([
            frame(|frame| {
                frame.input_transcript = Some("hello".into());
                frame.session_resumption = Some(LiveSessionResumption {
                    handle: Some("h1".into()),
                    resumable: true,
                });
            }),
            frame(|frame| {
                frame.session_resumption = Some(LiveSessionResumption {
                    handle: Some("not-resumable".into()),
                    resumable: false,
                });
            }),
            frame(|frame| {
                frame.go_away = Some(LiveGoAway {
                    time_left: "1s".into(),
                })
            }),
        ]);
        session
    });
    let run = scenario.run();

    assert!(run.result.is_none());
    assert_eq!(run.wire.borrow().reopen_handles, vec![Some("h1".into())]);
    assert_eq!(run.accumulated, "hello");
    run.assert_no_audio_lost();
}

#[test]
fn non_retryable_server_error_ends_the_stream_with_that_error() {
    let mut scenario = Scenario::new(DEDICATED_MODEL, Duration::from_secs(60));
    scenario.first = Box::new(|wire| {
        let mut session = FakeSession::new(0, wire);
        session.polls.extend(idle_polls(30));
        session
            .polls
            .push_back(Ok(LivePoll::ServerError(LiveServerError {
                message: "quota exhausted".into(),
                retryable: false,
            })));
        session
    });
    let run = scenario.run();

    let error = run
        .result
        .expect("the stream must end with the server error");
    assert!(error.to_string().contains("quota exhausted"), "{error}");
    assert!(run.wire.borrow().reopen_handles.is_empty());
    assert!(run.elapsed < Duration::from_secs(1), "{:?}", run.elapsed);
}

#[test]
fn exhausted_reconnects_report_the_last_error_after_virtual_backoff() {
    let mut scenario = Scenario::new(DEDICATED_MODEL, Duration::from_secs(600));
    scenario.capture = false;
    scenario.seed_audio = 1_600;
    scenario.first = Box::new(|wire| {
        let mut session = FakeSession::new(0, wire);
        session.sends_before_failure = Some(0);
        session
    });
    scenario.reopen = Box::new(|_, _| Err(anyhow::anyhow!("network unreachable")));
    let run = scenario.run();

    let error = run.result.expect("bounded reconnect must give up");
    assert_eq!(error.to_string(), "network unreachable");
    assert_eq!(
        run.wire.borrow().reopen_handles.len(),
        LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS as usize
    );
    assert!(run.elapsed > Duration::from_secs(1), "{:?}", run.elapsed);
    // The unsent seed audio is handed back for the caller's tail, not dropped.
    assert_eq!(run.leftover, run.captured);
}

#[test]
fn tail_waits_past_its_grace_for_a_late_final_then_stops() {
    let wire = Rc::new(RefCell::new(Wire::default()));
    let mut session = FakeSession::new(0, &wire);
    // 20 ms per idle poll: the final lands after 1.0 s, inside the 1.2 s grace.
    session.polls.extend(idle_polls(50));
    session
        .polls
        .push_back(frame(|frame| frame.input_transcript = Some("done".into())));
    let start = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let clock = FakeClock {
        start,
        now: Cell::new(start),
        run_for: Duration::from_secs(60),
        capture: false,
        next_sample: Cell::new(1),
        captured: RefCell::new(Vec::new()),
        audio: Arc::new(Mutex::new(Vec::new())),
        stop,
    };
    let accumulated = Arc::new(Mutex::new(String::new()));
    let abort = AtomicBool::new(false);
    let auto_paste =
        crate::overlay::utils::StreamingAutoPaste::new(false, Arc::new(AtomicBool::new(false)));
    let mut transcript = crate::api::gemini_transcribe::TranscriptState::default();
    wait_for_final_transcriptions(FinalTranscriptionsContext {
        session: &mut session,
        accumulated_text: &accumulated,
        transcribe_text: &mut transcript,
        auto_paste: &auto_paste,
        uses_interim_transcripts: true,
        streaming_hwnd: None,
        abort_signal: &abort,
        clock: &clock,
    });

    assert_eq!(*accumulated.lock().unwrap(), "done");
    // The final extended the wait to 1.0 s + 0.7 s; nothing after it arrived.
    assert_eq!(clock.elapsed(), Duration::from_millis(1_700));
}
