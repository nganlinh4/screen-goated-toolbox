use super::super::*;
use super::text_state::LiveTranslateTextState;

#[derive(Clone, Copy)]
pub(super) struct DirectAudioGate<'a> {
    pub(super) playback: &'a Arc<dyn crate::api::tts::manager::RealtimeAudioSink>,
    pub(super) stop_signal: &'a AtomicBool,
    pub(super) session_id: u64,
    pub(super) engine_revision: u64,
    pub(super) epoch: u64,
    pub(super) visible: bool,
}

#[derive(Default)]
pub(super) struct LiveTranslateOutputState {
    received_audio_chunks: usize,
    generation_boundaries: u64,
    turn_boundaries: u64,
    interrupted_generations: u64,
    text_state: LiveTranslateTextState,
}

impl LiveTranslateOutputState {
    pub(super) fn apply_frame_effects(
        &mut self,
        frame: &LiveServerFrame,
        effects: Vec<LiveLifecycleEffect>,
        event_tx: &mpsc::Sender<S2sEvent>,
        playback: &crate::api::tts::player::audio_player::AudioPlayer,
        audio_gate: &DirectAudioGate<'_>,
    ) -> Result<()> {
        for effect in effects {
            match effect {
                LiveLifecycleEffect::DeliverContent { count } => {
                    let actual = LiveLifecycleFrame::from_server_frame(1, frame).content_count;
                    anyhow::ensure!(
                        count == actual,
                        "Live Translate content effect mismatch: expected {count}, frame has {actual}"
                    );
                    self.deliver_content(frame, event_tx, audio_gate);
                }
                LiveLifecycleEffect::FinalizeGeneration => {
                    self.generation_boundaries = self.generation_boundaries.saturating_add(1);
                }
                LiveLifecycleEffect::FinalizeTurn => {
                    self.turn_boundaries = self.turn_boundaries.saturating_add(1);
                }
                LiveLifecycleEffect::StopPlayback => {
                    playback.stop();
                    let _ = event_tx.send(S2sEvent::Interrupt);
                }
                LiveLifecycleEffect::DiscardQueuedOutput => playback.stop(),
                LiveLifecycleEffect::FinalizeInterruptedGeneration => {
                    self.interrupted_generations = self.interrupted_generations.saturating_add(1);
                }
                LiveLifecycleEffect::DispatchTools { .. }
                | LiveLifecycleEffect::CancelTools { .. } => {
                    anyhow::bail!("Live Translate does not support Gemini Live tool effects")
                }
                LiveLifecycleEffect::OpenSocket { .. }
                | LiveLifecycleEffect::SendSetup { .. }
                | LiveLifecycleEffect::CloseSocket { .. }
                | LiveLifecycleEffect::ScheduleReconnect { .. }
                | LiveLifecycleEffect::ReportFailure { .. }
                | LiveLifecycleEffect::CancelSession
                | LiveLifecycleEffect::FinalizeResponse { .. } => {
                    anyhow::bail!("transport lifecycle effect escaped the S2S adapter")
                }
            }
        }
        Ok(())
    }

    pub(super) fn received_audio_chunks(&self) -> usize {
        self.received_audio_chunks
    }

    pub(super) fn generation_boundaries(&self) -> u64 {
        self.generation_boundaries
    }

    pub(super) fn turn_boundaries(&self) -> u64 {
        self.turn_boundaries
    }

    pub(super) fn interrupted_generations(&self) -> u64 {
        self.interrupted_generations
    }

    fn deliver_content(
        &mut self,
        frame: &LiveServerFrame,
        event_tx: &mpsc::Sender<S2sEvent>,
        audio_gate: &DirectAudioGate<'_>,
    ) {
        let mut text_changed = false;
        if let Some(text) = frame.input_transcript.as_deref() {
            text_changed |= self.text_state.update_source(text);
        }
        if let Some(text) = frame.output_transcript.as_deref() {
            text_changed |= self.text_state.update_target(text);
        }
        if text_changed {
            let _ = event_tx.send(self.text_state.snapshot_event());
        }
        self.received_audio_chunks = self
            .received_audio_chunks
            .saturating_add(frame.audio_chunks.len());
        if audio_gate.visible {
            for bytes in &frame.audio_chunks {
                crate::overlay::realtime_webview::state::publish_if_engine_current(
                    audio_gate.session_id,
                    audio_gate.stop_signal,
                    audio_gate.engine_revision,
                    || {
                        TTS_MANAGER.admit_direct_realtime_audio(
                            audio_gate.playback,
                            bytes,
                            audio_gate.stop_signal,
                            &crate::overlay::realtime_webview::state::TRANS_VISIBLE,
                            audio_gate.epoch,
                        );
                    },
                );
            }
        }
    }
}
