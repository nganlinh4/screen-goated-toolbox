//! The two things the dictation stream does not own: the Live socket and the
//! clock. Production uses [`ReadyLiveSession`] and [`SystemClock`]; tests
//! drive the whole stream with a scripted session and a virtual clock.

use std::time::{Duration, Instant};

use crate::api::gemini_live::ready_session::{LivePoll, ReadyLiveSession};

/// A ready Gemini Live transcription session as the stream uses it.
pub(in super::super) trait StreamSession {
    fn send_audio(&mut self, samples: &[i16]) -> anyhow::Result<()>;
    fn end_audio_stream(&mut self) -> anyhow::Result<()>;
    fn poll(&mut self) -> anyhow::Result<LivePoll>;
    fn close(&mut self);
}

impl StreamSession for ReadyLiveSession {
    fn send_audio(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        self.send_audio_pcm(samples, 16_000)
    }

    fn end_audio_stream(&mut self) -> anyhow::Result<()> {
        ReadyLiveSession::end_audio_stream(self)
    }

    fn poll(&mut self) -> anyhow::Result<LivePoll> {
        ReadyLiveSession::poll(self)
    }

    fn close(&mut self) {
        let _ = ReadyLiveSession::close(self);
    }
}

/// Opens a replacement session, resuming `handle` when one is given, and
/// gives up as soon as the cancellation check returns true.
pub(in super::super) type Reopen<'a, S> =
    dyn FnMut(Option<&str>, &dyn Fn() -> bool) -> anyhow::Result<S> + 'a;

/// Time as the stream loop sees it.
pub(in super::super) trait StreamClock {
    fn now(&self) -> Instant;
    fn sleep(&self, duration: Duration);
}

pub(in super::super) struct SystemClock;

impl StreamClock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
